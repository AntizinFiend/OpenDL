// OpenDL In-Page Link & Magnet Interceptor
function interceptLinkClick(e) {
  const link = e.target.closest("a");
  if (!link || !link.href) return;

  const urlStr = link.href.trim();

  // Intercept magnet: URIs directly (Chromium downloads API does not handle magnet: schemes)
  if (urlStr.startsWith("magnet:")) {
    e.preventDefault();
    e.stopPropagation();

    let suggestedFilename = "Magnet Download";
    try {
      const match = urlStr.match(/dn=([^&]+)/i);
      if (match && match[1]) {
        suggestedFilename = decodeURIComponent(match[1].replace(/\+/g, " "));
      }
    } catch (_) {}

    chrome.runtime.sendMessage({
      type: "MAGNET_LINK_CLICKED",
      url: urlStr,
      suggestedFilename: suggestedFilename,
      referrer: window.location.href
    });
    return;
  }

  // NOTE: We deliberately do NOT preventDefault on standard HTTP/HTTPS download links.
  // Websites like Mediafire, Mega, Google Drive, Rapidgator, etc. require their in-page
  // event listeners and scripts to execute (to establish session tokens, cookies, and Cloudflare challenges).
  // Chromium's chrome.downloads.onDeterminingFilename in background.js will cleanly intercept
  // the fully-negotiated download stream with 100% of the session cookies, tokens, and headers!
}

document.addEventListener("click", interceptLinkClick, true);
document.addEventListener("auxclick", interceptLinkClick, true);
