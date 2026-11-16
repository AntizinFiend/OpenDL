use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT_RANGES, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, ETAG, LAST_MODIFIED, RANGE};
use reqwest::Client;
use std::str::FromStr;
use tracing::{debug, info};
use url::Url;

use crate::types::{DownloadMetadata, DownloadOptions};

pub async fn probe_url(client: &Client, options: &DownloadOptions) -> Result<DownloadMetadata> {
    info!(url = %options.url, "Probing download URL for range support and content metadata");

    let mut headers = HeaderMap::new();
    for (k, v) in &options.headers {
        if let (Ok(hname), Ok(hval)) = (HeaderName::from_str(k), HeaderValue::from_str(v)) {
            headers.insert(hname, hval);
        }
    }

    // Default browser headers if not present
    if !headers.contains_key(reqwest::header::USER_AGENT) {
        if let Ok(v) = HeaderValue::from_str(crate::resolver::DEFAULT_BROWSER_UA) {
            headers.insert(reqwest::header::USER_AGENT, v);
        }
    }
    if !headers.contains_key(reqwest::header::ACCEPT) {
        if let Ok(v) = HeaderValue::from_str("text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8") {
            headers.insert(reqwest::header::ACCEPT, v);
        }
    }

    // Check if URL can be resolved to a direct stream (e.g. Mediafire, Google Drive, Dropbox)
    let (target_url, resolved_referrer, resolved_cookies) = if crate::resolver::is_mediafire_url(&options.url)
        || crate::resolver::is_google_drive_url(&options.url)
        || crate::resolver::is_dropbox_url(&options.url)
    {
        match crate::resolver::resolve_download_url(&options.url).await {
            Ok(resolved) => {
                info!(original = %options.url, direct = %resolved.direct_url, "Resolved direct download URL via engine resolver");
                for (k, v) in &resolved.headers {
                    if let (Ok(hname), Ok(hval)) = (HeaderName::from_str(k), HeaderValue::from_str(v)) {
                        headers.entry(hname).or_insert(hval);
                    }
                }
                (resolved.direct_url, resolved.referrer, resolved.cookies)
            }
            Err(e) => {
                debug!(error = %e, "Resolver failed to extract direct link; proceeding with original URL");
                (options.url.clone(), None, None)
            }
        }
    } else {
        (options.url.clone(), None, None)
    };

    // Try HEAD request first
    let head_res = client
        .head(&target_url)
        .headers(headers.clone())
        .send()
        .await;

    match head_res {
        Ok(res) if res.status().is_success() => {
            let final_url = res.url().to_string();
            let res_headers = res.headers();

            let content_length = res_headers
                .get(CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());

            let content_type = res_headers
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok());

            let accepts_ranges = res_headers
                .get(ACCEPT_RANGES)
                .and_then(|v| v.to_str().ok())
                .map(|v| v.eq_ignore_ascii_case("bytes"))
                .unwrap_or(false);

            let etag = res_headers
                .get(ETAG)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            let last_modified = res_headers
                .get(LAST_MODIFIED)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            let suggested_filename = extract_filename(
                res_headers.get(CONTENT_DISPOSITION).and_then(|v| v.to_str().ok()),
                content_type,
                &final_url,
            );

            let is_webpage = content_type.map(|c| c.contains("text/html")).unwrap_or(false)
                && (suggested_filename == "download.bin" || suggested_filename.ends_with(".html") || suggested_filename.ends_with(".php"))
                && content_length.map(|l| l < 500_000).unwrap_or(true);

            // If HEAD didn't explicitly say accept-ranges or didn't provide content-length, verify with ranged GET probe
            if content_length.is_none() || !accepts_ranges {
                debug!("HEAD response inconclusive for range support, performing ranged GET probe (bytes=0-0)");
                if let Ok(ranged_meta) = probe_ranged_get(client, &final_url, &headers).await {
                    return Ok(DownloadMetadata {
                        url: options.url.clone(),
                        final_url: ranged_meta.final_url,
                        content_length: ranged_meta.content_length.or(content_length),
                        accepts_ranges: ranged_meta.accepts_ranges,
                        etag: ranged_meta.etag.or(etag),
                        last_modified: ranged_meta.last_modified.or(last_modified),
                        suggested_filename: if suggested_filename != "download.bin" {
                            suggested_filename
                        } else {
                            ranged_meta.suggested_filename
                        },
                        referrer: resolved_referrer.or(ranged_meta.referrer),
                        cookies: resolved_cookies.or(ranged_meta.cookies),
                        is_webpage: ranged_meta.is_webpage,
                    });
                }
            }

            Ok(DownloadMetadata {
                url: options.url.clone(),
                final_url,
                content_length,
                accepts_ranges,
                etag,
                last_modified,
                suggested_filename,
                referrer: resolved_referrer,
                cookies: resolved_cookies,
                is_webpage,
            })
        }
        _ => {
            // HEAD failed or server does not support HEAD. Fallback directly to ranged GET probe.
            debug!("HEAD failed or returned non-success; falling back to ranged GET probe");
            let ranged_meta = probe_ranged_get(client, &target_url, &headers)
                .await
                .context("Ranged GET probe failed")?;

            Ok(DownloadMetadata {
                url: options.url.clone(),
                referrer: resolved_referrer.or(ranged_meta.referrer),
                cookies: resolved_cookies.or(ranged_meta.cookies),
                ..ranged_meta
            })
        }
    }
}


async fn probe_ranged_get(client: &Client, url: &str, base_headers: &HeaderMap) -> Result<DownloadMetadata> {
    let mut headers = base_headers.clone();
    headers.insert(RANGE, HeaderValue::from_static("bytes=0-0"));

    let res = client.get(url).headers(headers).send().await?;
    let final_url = res.url().to_string();
    let status = res.status();
    let res_headers = res.headers();

    let etag = res_headers
        .get(ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let last_modified = res_headers
        .get(LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let content_type = res_headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok());

    let suggested_filename = extract_filename(
        res_headers.get(CONTENT_DISPOSITION).and_then(|v| v.to_str().ok()),
        content_type,
        &final_url,
    );

    let is_webpage = content_type.map(|c| c.contains("text/html")).unwrap_or(false)
        && (suggested_filename == "download.bin" || suggested_filename.ends_with(".html") || suggested_filename.ends_with(".php"));

    if status == reqwest::StatusCode::PARTIAL_CONTENT {
        // HTTP 206 Partial Content -> Range is supported!
        let total_size = res_headers
            .get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_content_range_total);

        Ok(DownloadMetadata {
            url: url.to_string(),
            final_url,
            content_length: total_size,
            accepts_ranges: true,
            etag,
            last_modified,
            suggested_filename,
            referrer: None,
            cookies: None,
            is_webpage,
        })
    } else {
        // HTTP 200 OK -> Server ignored range, only single stream supported
        let content_length = res_headers
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());

        Ok(DownloadMetadata {
            url: url.to_string(),
            final_url,
            content_length,
            accepts_ranges: false,
            etag,
            last_modified,
            suggested_filename,
            referrer: None,
            cookies: None,
            is_webpage,
        })
    }

}

fn parse_content_range_total(header: &str) -> Option<u64> {
    // Expected format: "bytes 0-0/104857600" or "bytes 0-0/*"
    let slash_pos = header.rfind('/')?;
    let total_part = &header[slash_pos + 1..].trim();
    total_part.parse::<u64>().ok()
}

pub fn extract_filename(disposition: Option<&str>, content_type: Option<&str>, url_str: &str) -> String {
    // 1. Content-Disposition header
    if let Some(disp) = disposition {
        if let Some(fname) = parse_filename_from_disposition(disp) {
            let sanitized = sanitize_filename(&fname);
            if sanitized != "download.bin" {
                return sanitized;
            }
        }
    }

    let parsed_url = Url::parse(url_str).ok();

    // 2. Query parameters (e.g. response-content-disposition, filename, title, mime)
    let mut query_title: Option<String> = None;
    let mut query_filename: Option<String> = None;
    let mut query_mime: Option<String> = None;

    if let Some(ref u) = parsed_url {
        for (k, v) in u.query_pairs() {
            let key_lower = k.to_lowercase();
            if key_lower == "response-content-disposition" {
                if let Some(fname) = parse_filename_from_disposition(&v) {
                    let sanitized = sanitize_filename(&fname);
                    if sanitized != "download.bin" {
                        return sanitized;
                    }
                }
            } else if key_lower == "filename" || key_lower == "file_name" || key_lower == "fn" {
                if !v.trim().is_empty() {
                    query_filename = Some(v.to_string());
                }
            } else if key_lower == "title" {
                if !v.trim().is_empty() {
                    query_title = Some(v.to_string());
                }
            } else if key_lower == "mime" {
                if !v.trim().is_empty() {
                    query_mime = Some(v.to_string());
                }
            }
        }
    }

    // Determine extension from Content-Type or query mime
    let ext_from_mime = content_type
        .and_then(mime_to_extension)
        .or_else(|| query_mime.as_deref().and_then(mime_to_extension));

    // If query gave an explicit filename with an extension, prioritize it
    if let Some(ref q_fn) = query_filename {
        let cleaned = sanitize_filename(q_fn);
        if cleaned.contains('.') && cleaned != "download.bin" {
            return cleaned;
        }
    }

    // 3. Inspect URL path segment
    let path_segment = parsed_url.as_ref().and_then(|u| {
        u.path_segments().and_then(|mut segs| {
            segs.next_back().map(|last| {
                let decoded = urlencoding_decode(last);
                decoded.split('?').next().unwrap_or(&decoded).trim().to_string()
            })
        })
    });

    let script_exts = [".php", ".jsp", ".asp", ".aspx", ".cgi", ".do", ".action", ".html", ".htm"];
    let generic_tokens = ["videoplayback", "stream", "play", "blob", "media", "video", "get_video", "segment", "master"];

    if let Some(ref seg) = path_segment {
        let seg_lower = seg.to_lowercase();
        let is_script = script_exts.iter().any(|ext| seg_lower.ends_with(ext));
        let is_generic = generic_tokens.iter().any(|t| seg_lower == *t || seg_lower.starts_with(&format!("{}_", t)));
        let has_ext = seg.contains('.') && !seg.ends_with('.');

        if !is_script && !is_generic && has_ext {
            let sanitized = sanitize_filename(seg);
            if sanitized != "download.bin" {
                return sanitized;
            }
        }

        // If path segment is generic (like videoplayback) or a script or has no extension
        if let Some(ref title) = query_title {
            let sanitized_title = sanitize_filename(title);
            if sanitized_title.contains('.') {
                return sanitized_title;
            } else if let Some(ext) = ext_from_mime {
                return format!("{}.{}", sanitized_title, ext);
            }
        }

        if let Some(ref q_fn) = query_filename {
            let sanitized = sanitize_filename(q_fn);
            if let Some(ext) = ext_from_mime {
                if !sanitized.contains('.') {
                    return format!("{}.{}", sanitized, ext);
                }
            }
            return sanitized;
        }

        if let Some(ext) = ext_from_mime {
            if is_generic || seg.is_empty() {
                let default_prefix = if ext == "mp4" || ext == "webm" || ext == "mkv" || ext == "mov" || ext == "avi" || ext == "flv" || ext == "ts" {
                    "video"
                } else if ext == "mp3" || ext == "m4a" || ext == "ogg" || ext == "wav" || ext == "flac" {
                    "audio"
                } else {
                    "download"
                };
                return format!("{}.{}", default_prefix, ext);
            } else if !has_ext {
                return format!("{}.{}", sanitize_filename(seg), ext);
            }
        }
    }

    // 4. Fallback if no valid path segment found
    if let Some(ref title) = query_title {
        let sanitized = sanitize_filename(title);
        if let Some(ext) = ext_from_mime {
            return format!("{}.{}", sanitized, ext);
        }
        return sanitized;
    }

    if let Some(ext) = ext_from_mime {
        let prefix = if ext == "mp4" || ext == "webm" || ext == "mkv" { "video" } else { "download" };
        return format!("{}.{}", prefix, ext);
    }

    "download.bin".to_string()
}

pub fn mime_to_extension(mime: &str) -> Option<&'static str> {
    let clean = mime.split(';').next().unwrap_or(mime).trim().to_lowercase();
    match clean.as_str() {
        "video/mp4" | "video/mp4v-es" => Some("mp4"),
        "video/webm" => Some("webm"),
        "video/x-matroska" | "video/mkv" => Some("mkv"),
        "video/quicktime" => Some("mov"),
        "video/x-msvideo" | "video/avi" => Some("avi"),
        "video/x-flv" | "video/flv" => Some("flv"),
        "video/3gpp" | "video/3gpp2" => Some("3gp"),
        "video/ogg" | "video/ogv" => Some("ogv"),
        "video/mp2t" => Some("ts"),
        "video/x-ms-wmv" => Some("wmv"),

        "audio/mp4" | "audio/m4a" | "audio/x-m4a" => Some("m4a"),
        "audio/mpeg" | "audio/mp3" => Some("mp3"),
        "audio/ogg" | "audio/opus" => Some("ogg"),
        "audio/wav" | "audio/x-wav" => Some("wav"),
        "audio/flac" | "audio/x-flac" => Some("flac"),
        "audio/webm" => Some("weba"),
        "audio/aac" => Some("aac"),

        "application/pdf" => Some("pdf"),
        "application/zip" | "application/x-zip-compressed" => Some("zip"),
        "application/x-rar-compressed" | "application/vnd.rar" | "application/x-rar" => Some("rar"),
        "application/x-7z-compressed" => Some("7z"),
        "application/x-tar" => Some("tar"),
        "application/gzip" | "application/x-gzip" => Some("gz"),
        "application/vnd.android.package-archive" => Some("apk"),
        "application/x-msdownload" | "application/x-msdos-program" | "application/exe" | "application/octet-stream-exe" => Some("exe"),
        "application/x-iso9660-image" => Some("iso"),

        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/svg+xml" => Some("svg"),

        _ => None,
    }
}

fn parse_filename_from_disposition(disp: &str) -> Option<String> {
    // Look for filename*=UTF-8''... first
    if let Some(pos) = disp.find("filename*=") {
        let rest = &disp[pos + 10..].trim();
        let val = rest.split(';').next().unwrap_or(rest).trim();
        if let Some(quote_stripped) = val.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
            return Some(urlencoding_decode(quote_stripped));
        }
        if let Some(rest) = val.to_lowercase().strip_prefix("utf-8''") {
            let offset = val.len() - rest.len();
            return Some(urlencoding_decode(&val[offset..]));
        }
    }

    // Look for filename="..."
    if let Some(pos) = disp.find("filename=") {
        let rest = &disp[pos + 9..].trim();
        let val = rest.split(';').next().unwrap_or(rest).trim();
        let trimmed = val.trim_matches('"').trim_matches('\'');
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    None
}

fn urlencoding_decode(input: &str) -> String {
    let mut bytes = Vec::new();
    let in_bytes = input.as_bytes();
    let mut i = 0;
    while i < in_bytes.len() {
        if in_bytes[i] == b'%' && i + 2 < in_bytes.len() {
            if let Ok(b) = u8::from_str_radix(std::str::from_utf8(&in_bytes[i + 1..i + 3]).unwrap_or(""), 16) {
                bytes.push(b);
                i += 3;
                continue;
            }
        } else if in_bytes[i] == b'+' {
            bytes.push(b' ');
            i += 1;
            continue;
        }
        bytes.push(in_bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&bytes).to_string()
}

fn sanitize_filename(name: &str) -> String {
    let invalid = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    let sanitized: String = name.chars().map(|c| if invalid.contains(&c) { '_' } else { c }).collect();
    if sanitized.is_empty() {
        "download.bin".to_string()
    } else {
        sanitized
    }
}

