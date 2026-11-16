use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, ACCEPT, ACCEPT_LANGUAGE, SET_COOKIE};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tracing::info;
use url::Url;

pub const DEFAULT_BROWSER_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedUrl {
    pub original_url: String,
    pub direct_url: String,
    pub suggested_filename: Option<String>,
    pub referrer: Option<String>,
    pub cookies: Option<String>,
    pub user_agent: Option<String>,
    pub headers: HashMap<String, String>,
}

/// Helper to build a standard scraping/resolving reqwest client
fn build_resolver_client() -> Result<Client> {
    Client::builder()
        .user_agent(DEFAULT_BROWSER_UA)
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .context("Failed to build resolver HTTP client")
}

/// Extract cookie string from reqwest response headers
fn extract_response_cookies(headers: &HeaderMap) -> Option<String> {
    let mut cookies = Vec::new();
    for val in headers.get_all(SET_COOKIE) {
        if let Ok(s) = val.to_str() {
            if let Some(cookie_pair) = s.split(';').next() {
                let trimmed = cookie_pair.trim();
                if !trimmed.is_empty() {
                    cookies.push(trimmed.to_string());
                }
            }
        }
    }
    if cookies.is_empty() {
        None
    } else {
        Some(cookies.join("; "))
    }
}

/// Check if a URL belongs to Mediafire
pub fn is_mediafire_url(url: &str) -> bool {
    if let Ok(parsed) = Url::parse(url) {
        if let Some(host) = parsed.host_str() {
            let h = host.to_lowercase();
            return (h == "mediafire.com" || h.ends_with(".mediafire.com"))
                && (parsed.path().starts_with("/file")
                    || parsed.path().starts_with("/download")
                    || parsed.path().starts_with("/view")
                    || parsed.query().map(|q| q.contains("quickkey=")).unwrap_or(false));
        }
    }
    false
}

/// Check if a URL belongs to Google Drive
pub fn is_google_drive_url(url: &str) -> bool {
    if let Ok(parsed) = Url::parse(url) {
        if let Some(host) = parsed.host_str() {
            let h = host.to_lowercase();
            return (h == "drive.google.com" || h == "docs.google.com")
                && (parsed.path().contains("/file/d/")
                    || parsed.path().contains("/open")
                    || parsed.path().contains("/uc"));
        }
    }
    false
}

/// Check if a URL belongs to Dropbox
pub fn is_dropbox_url(url: &str) -> bool {
    if let Ok(parsed) = Url::parse(url) {
        if let Some(host) = parsed.host_str() {
            let h = host.to_lowercase();
            return (h == "dropbox.com" || h.ends_with(".dropbox.com"))
                && (parsed.path().starts_with("/s/") || parsed.path().starts_with("/scl/"));
        }
    }
    false
}

/// Resolve Mediafire page link to direct CDN download link
pub async fn resolve_mediafire(client: &Client, url: &str) -> Result<ResolvedUrl> {
    info!(url = %url, "Resolving Mediafire direct download stream");

    let res = client
        .get(url)
        .header(ACCEPT, "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8")
        .header(ACCEPT_LANGUAGE, "en-US,en;q=0.9")
        .header("Sec-Fetch-Dest", "document")
        .header("Sec-Fetch-Mode", "navigate")
        .header("Sec-Fetch-Site", "none")
        .header("Sec-Fetch-User", "?1")
        .header("Upgrade-Insecure-Requests", "1")
        .send()
        .await
        .with_context(|| format!("Failed to fetch Mediafire page at {}", url))?;

    let final_fetch_url = res.url().to_string();
    let res_headers = res.headers().clone();
    let cookies = extract_response_cookies(&res_headers);

    let html = res
        .text()
        .await
        .with_context(|| "Failed to read Mediafire response text")?;

    // 1. Try to find direct download link in HTML
    let direct_url = extract_mediafire_direct_link(&html)
        .or_else(|| {
            // Check if final URL itself is already direct download
            if final_fetch_url.contains("download") && final_fetch_url.contains(".mediafire.com") && !final_fetch_url.contains("/file/") {
                Some(final_fetch_url.clone())
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow::anyhow!("Could not extract direct download link from Mediafire page"))?;

    // 2. Extract suggested filename
    let filename = extract_mediafire_filename(&html, &direct_url);

    let mut headers = HashMap::new();
    headers.insert("Referer".to_string(), url.to_string());
    headers.insert("User-Agent".to_string(), DEFAULT_BROWSER_UA.to_string());
    if let Some(ref c) = cookies {
        headers.insert("Cookie".to_string(), c.clone());
    }

    Ok(ResolvedUrl {
        original_url: url.to_string(),
        direct_url,
        suggested_filename: filename,
        referrer: Some(url.to_string()),
        cookies,
        user_agent: Some(DEFAULT_BROWSER_UA.to_string()),
        headers,
    })
}

/// Helper regex-like extraction of Mediafire direct link from HTML
fn extract_mediafire_direct_link(html: &str) -> Option<String> {
    // Pattern 1: href="https://download*.mediafire.com/..."
    if let Some(pos) = html.find("href=\"https://download") {
        let start = pos + 6;
        if let Some(end) = html[start..].find('"') {
            return Some(html[start..start + end].to_string());
        }
    }
    if let Some(pos) = html.find("href='https://download") {
        let start = pos + 6;
        if let Some(end) = html[start..].find('\'') {
            return Some(html[start..start + end].to_string());
        }
    }

    // Pattern 2: id="downloadButton" ... href="..."
    if let Some(btn_pos) = html.find("id=\"downloadButton\"") {
        let slice = &html[btn_pos..btn_pos.saturating_add(500).min(html.len())];
        if let Some(h_pos) = slice.find("href=\"") {
            let start = h_pos + 6;
            if let Some(end) = slice[start..].find('"') {
                let link = &slice[start..start + end];
                if link.starts_with("http") {
                    return Some(link.to_string());
                }
            }
        }
    }

    // Pattern 3: aria-label="Download file" ... href="..."
    if let Some(aria_pos) = html.find("aria-label=\"Download file\"") {
        let slice = &html[aria_pos..aria_pos.saturating_add(500).min(html.len())];
        if let Some(h_pos) = slice.find("href=\"") {
            let start = h_pos + 6;
            if let Some(end) = slice[start..].find('"') {
                let link = &slice[start..start + end];
                if link.starts_with("http") {
                    return Some(link.to_string());
                }
            }
        }
    }

    // Pattern 4: kNO = "..." or window.location = "..."
    for key in &["kNO = \"", "kNO=\"", "kNO = '", "kNO='"] {
        if let Some(pos) = html.find(key) {
            let start = pos + key.len();
            let quote = if key.ends_with('\'') { '\'' } else { '"' };
            if let Some(end) = html[start..].find(quote) {
                let link = &html[start..start + end];
                if link.starts_with("http") {
                    return Some(link.to_string());
                }
            }
        }
    }

    None
}

/// Helper extraction of Mediafire filename from HTML or direct link
fn extract_mediafire_filename(html: &str, direct_url: &str) -> Option<String> {
    // 1. Look for <div class="filename">(.*?)</div>
    if let Some(pos) = html.find("class=\"filename\">") {
        let start = pos + 17;
        if let Some(end) = html[start..].find('<') {
            let name = html[start..start + end].trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }

    // 2. Look for title="(.*?) in download button label
    if let Some(pos) = html.find("class=\"dl-btn-label\" title=\"") {
        let start = pos + 28;
        if let Some(end) = html[start..].find('"') {
            let name = html[start..start + end].trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }

    // 3. Fallback to path of direct download URL
    if let Ok(parsed) = Url::parse(direct_url) {
        if let Some(last_seg) = parsed.path_segments().and_then(|mut s| s.next_back()) {
            if !last_seg.is_empty() {
                return Some(last_seg.to_string());
            }
        }
    }

    None
}

/// Resolve Google Drive link to direct download link
pub async fn resolve_google_drive(client: &Client, url: &str) -> Result<ResolvedUrl> {
    info!(url = %url, "Resolving Google Drive direct download stream");

    // Extract file ID
    let file_id = extract_google_drive_file_id(url)
        .ok_or_else(|| anyhow::anyhow!("Could not extract Google Drive file ID from URL: {}", url))?;

    let export_url = format!("https://drive.google.com/uc?export=download&id={}&confirm=t", file_id);

    let res = client
        .get(&export_url)
        .header(ACCEPT, "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
        .header(ACCEPT_LANGUAGE, "en-US,en;q=0.9")
        .send()
        .await
        .with_context(|| format!("Failed to request Google Drive export link for ID {}", file_id))?;

    let final_url = res.url().to_string();
    let res_headers = res.headers().clone();
    let cookies = extract_response_cookies(&res_headers);

    let content_type = res_headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // If response is not HTML, it's already the direct download stream!
    if !content_type.contains("text/html") {
        let mut headers = HashMap::new();
        headers.insert("Referer".to_string(), export_url.clone());
        headers.insert("User-Agent".to_string(), DEFAULT_BROWSER_UA.to_string());
        if let Some(ref c) = cookies {
            headers.insert("Cookie".to_string(), c.clone());
        }

        return Ok(ResolvedUrl {
            original_url: url.to_string(),
            direct_url: final_url,
            suggested_filename: None,
            referrer: Some(export_url),
            cookies,
            user_agent: Some(DEFAULT_BROWSER_UA.to_string()),
            headers,
        });
    }

    // Google Drive returned a virus scan warning confirmation page (for files > 100MB)
    let html = res.text().await.unwrap_or_default();

    // Look for <a id="uc-download-link" href="..."> or <form id="download-form" action="...">
    let confirm_url = extract_google_drive_confirm_link(&html, &file_id)
        .unwrap_or_else(|| export_url.clone());

    let mut headers = HashMap::new();
    headers.insert("Referer".to_string(), export_url.clone());
    headers.insert("User-Agent".to_string(), DEFAULT_BROWSER_UA.to_string());
    if let Some(ref c) = cookies {
        headers.insert("Cookie".to_string(), c.clone());
    }

    Ok(ResolvedUrl {
        original_url: url.to_string(),
        direct_url: confirm_url,
        suggested_filename: None,
        referrer: Some(export_url),
        cookies,
        user_agent: Some(DEFAULT_BROWSER_UA.to_string()),
        headers,
    })
}

fn extract_google_drive_file_id(url: &str) -> Option<String> {
    if let Ok(parsed) = Url::parse(url) {
        // Form 1: /file/d/<id>/view
        if let Some(pos) = parsed.path().find("/file/d/") {
            let rest = &parsed.path()[pos + 8..];
            let id = rest.split('/').next().unwrap_or("");
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }
        // Form 2: query id=...
        for (k, v) in parsed.query_pairs() {
            if k == "id" && !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn extract_google_drive_confirm_link(html: &str, file_id: &str) -> Option<String> {
    // 1. Look for id="uc-download-link" href="..."
    if let Some(pos) = html.find("id=\"uc-download-link\"") {
        let slice = &html[pos..pos.saturating_add(500).min(html.len())];
        if let Some(h_pos) = slice.find("href=\"") {
            let start = h_pos + 6;
            if let Some(end) = slice[start..].find('"') {
                let link = &slice[start..start + end];
                if link.starts_with("http") {
                    return Some(link.to_string());
                } else if link.starts_with('/') {
                    return Some(format!("https://drive.google.com{}", link));
                }
            }
        }
    }

    // 2. Look for action="..." in download-form
    if let Some(pos) = html.find("id=\"download-form\"") {
        let slice = &html[pos..pos.saturating_add(500).min(html.len())];
        if let Some(a_pos) = slice.find("action=\"") {
            let start = a_pos + 8;
            if let Some(end) = slice[start..].find('"') {
                let action = &slice[start..start + end];
                if action.starts_with("http") {
                    return Some(action.to_string());
                } else if action.starts_with('/') {
                    return Some(format!("https://drive.google.com{}", action));
                }
            }
        }
    }

    // 3. Look for confirm query parameter in any link
    if let Some(pos) = html.find("confirm=") {
        let start = pos;
        if let Some(end) = html[start..].find(['"', '\'', '&', ' ']) {
            let token = &html[start + 8..start + end];
            if !token.is_empty() {
                return Some(format!(
                    "https://drive.google.com/uc?export=download&id={}&confirm={}",
                    file_id, token
                ));
            }
        }
    }

    None
}

/// Resolve Dropbox link to direct download link
pub fn resolve_dropbox(url: &str) -> Option<ResolvedUrl> {
    if let Ok(mut parsed) = Url::parse(url) {
        let mut pairs: Vec<(String, String)> = parsed
            .query_pairs()
            .filter(|(k, _)| k != "dl")
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        pairs.push(("dl".to_string(), "1".to_string()));

        let query_str = pairs
            .into_iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");

        parsed.set_query(Some(&query_str));

        let direct_url = parsed.to_string();
        let mut headers = HashMap::new();
        headers.insert("Referer".to_string(), url.to_string());
        headers.insert("User-Agent".to_string(), DEFAULT_BROWSER_UA.to_string());

        return Some(ResolvedUrl {
            original_url: url.to_string(),
            direct_url,
            suggested_filename: None,
            referrer: Some(url.to_string()),
            cookies: None,
            user_agent: Some(DEFAULT_BROWSER_UA.to_string()),
            headers,
        });
    }
    None
}

/// Master function to resolve any download URL (Mediafire, Google Drive, Dropbox, or direct)
pub async fn resolve_download_url(url: &str) -> Result<ResolvedUrl> {
    let client = build_resolver_client()?;

    if is_mediafire_url(url) {
        return resolve_mediafire(&client, url).await;
    }

    if is_google_drive_url(url) {
        return resolve_google_drive(&client, url).await;
    }

    if is_dropbox_url(url) {
        if let Some(resolved) = resolve_dropbox(url) {
            return Ok(resolved);
        }
    }

    // Direct / Generic URL fallback: Follow redirects and capture cookies and headers
    let res = client
        .head(url)
        .send()
        .await;

    let (final_url, cookies) = match res {
        Ok(r) => {
            let f = r.url().to_string();
            let c = extract_response_cookies(r.headers());
            (f, c)
        }
        Err(_) => (url.to_string(), None),
    };

    let mut headers = HashMap::new();
    headers.insert("Referer".to_string(), url.to_string());
    headers.insert("User-Agent".to_string(), DEFAULT_BROWSER_UA.to_string());
    if let Some(ref c) = cookies {
        headers.insert("Cookie".to_string(), c.clone());
    }

    Ok(ResolvedUrl {
        original_url: url.to_string(),
        direct_url: final_url,
        suggested_filename: None,
        referrer: Some(url.to_string()),
        cookies,
        user_agent: Some(DEFAULT_BROWSER_UA.to_string()),
        headers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_mediafire_url() {
        assert!(is_mediafire_url("https://www.mediafire.com/file/o6x9zexample/test.zip/file"));
        assert!(is_mediafire_url("http://mediafire.com/file/abc123xyz/sample.rar"));
        assert!(is_mediafire_url("https://www.mediafire.com/download/o6x9zexample"));
        assert!(!is_mediafire_url("https://google.com/test.zip"));
    }

    #[test]
    fn test_extract_mediafire_direct_link() {
        let sample_html = r#"
            <html>
                <body>
                    <div class="filename">MyArchive.zip</div>
                    <a class="input popsok" id="downloadButton" href="https://download1584.mediafire.com/xyz123token/MyArchive.zip">Download (120 MB)</a>
                </body>
            </html>
        "#;
        let direct = extract_mediafire_direct_link(sample_html);
        assert_eq!(
            direct.as_deref(),
            Some("https://download1584.mediafire.com/xyz123token/MyArchive.zip")
        );

        let filename = extract_mediafire_filename(sample_html, direct.as_deref().unwrap());
        assert_eq!(filename.as_deref(), Some("MyArchive.zip"));
    }

    #[test]
    fn test_google_drive_file_id() {
        let u1 = "https://drive.google.com/file/d/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms/view?usp=sharing";
        assert_eq!(
            extract_google_drive_file_id(u1).as_deref(),
            Some("1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms")
        );

        let u2 = "https://drive.google.com/uc?id=1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms&export=download";
        assert_eq!(
            extract_google_drive_file_id(u2).as_deref(),
            Some("1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms")
        );
    }

    #[test]
    fn test_dropbox_dl_query() {
        let u = "https://www.dropbox.com/s/sample123/file.pdf?dl=0";
        let resolved = resolve_dropbox(u).unwrap();
        assert!(resolved.direct_url.contains("dl=1"));
    }
}
