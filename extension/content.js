// OpenDL In-Page Download Link Interceptor
const DOWNLOAD_EXTENSIONS = new Set([
  "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso", "bin", "dmg", "pkg",
  "exe", "msi", "apk", "deb", "rpm",
  "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv",
  "mp3", "wav", "flac", "aac", "ogg", "m4a",
  "pdf", "epub", "torrent"
]);

document.addEventListener("click", (e) => {
  const link = e.target.closest("a");
  if (!link || !link.href) return;

  const urlStr = link.href;
  if (!urlStr.startsWith("http://") && !urlStr.startsWith("https://")) return;

  let isDownload = false;
  let filename = link.getAttribute("download") || "";

  if (link.hasAttribute("download")) {
    isDownload = true;
  } else {
    try {
      const parsed = new URL(urlStr);
      const pathname = parsed.pathname;
      const lastDot = pathname.lastIndexOf(".");
      if (lastDot !== -1) {
        const ext = pathname.substring(lastDot + 1).toLowerCase();
        if (DOWNLOAD_EXTENSIONS.has(ext)) {
          isDownload = true;
          const slash = pathname.lastIndexOf("/");
          if (slash !== -1) {
            filename = decodeURIComponent(pathname.substring(slash + 1));
          }
        }
      }
    } catch (_) {}
  }

  if (isDownload) {
    e.preventDefault();
    e.stopPropagation();

    chrome.runtime.sendMessage({
      type: "PAGE_LINK_CLICKED",
      url: urlStr,
      suggestedFilename: filename,
      referrer: window.location.href
    });
  }
}, true);
