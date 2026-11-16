const NATIVE_HOST = "org.opendl.native";
const interceptedDownloadIds = new Set();

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "opendl_download_link",
    title: "Download with OpenDL",
    contexts: ["link", "video", "audio", "image"]
  });

  chrome.storage.local.get(["intercept_enabled"], (res) => {
    if (res.intercept_enabled === undefined) {
      chrome.storage.local.set({ intercept_enabled: true });
    }
  });
});

// 1. Right-Click Context Menu
chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  const url = info.linkUrl || info.srcUrl;
  if (!url) return;

  const referrer = info.pageUrl || (tab && tab.url) || "";
  const cookies = await getCookiesForUrl(url, referrer);

  let suggestedFilename = undefined;
  if (url.startsWith("magnet:")) {
    try {
      const match = url.match(/dn=([^&]+)/i);
      if (match && match[1]) suggestedFilename = decodeURIComponent(match[1].replace(/\+/g, " "));
    } catch (_) {}
    if (!suggestedFilename) suggestedFilename = "Magnet Download";
  } else if (info.srcUrl && tab && tab.title) {
    let cleanTitle = tab.title.split(" - YouTube")[0].trim();
    if (cleanTitle) {
      suggestedFilename = cleanTitle;
    }
  }

  triggerOpenDlDownload({
    action: "download",
    url: url,
    filename: suggestedFilename,
    referrer: referrer,
    cookies: cookies,
    user_agent: navigator.userAgent,
    headers: {
      "Referer": referrer,
      "User-Agent": navigator.userAgent,
      "Cookie": cookies,
      "Accept-Language": navigator.language || "en-US,en;q=0.9"
    }
  });
});

// 2. In-Page Content Script Magnet Link Click Messages
chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  if (msg && (msg.type === "MAGNET_LINK_CLICKED" || msg.type === "PAGE_LINK_CLICKED")) {
    chrome.storage.local.get(["intercept_enabled"], async (res) => {
      const enabled = res.intercept_enabled !== false;
      if (!enabled) {
        sendResponse({ intercepted: false });
        return;
      }

      const referrer = msg.referrer || (sender.tab && sender.tab.url) || "";
      const cookies = await getCookiesForUrl(msg.url, referrer);
      let cleanSuggested = (msg.suggestedFilename || "").split(/[/\\]/).pop() || undefined;

      triggerOpenDlDownload({
        action: "download",
        url: msg.url,
        filename: cleanSuggested,
        referrer: referrer,
        cookies: cookies,
        user_agent: navigator.userAgent,
        headers: {
          "Referer": referrer,
          "User-Agent": navigator.userAgent,
          "Cookie": cookies,
          "Accept-Language": navigator.language || "en-US,en;q=0.9"
        }
      });

      sendResponse({ intercepted: true });
    });
    return true; // Keep message channel open for async response
  }
});

// 3. Browser Download Interception: Determining Filename (Canonical Chromium hook)
// Intercepts the fully-resolved HTTP response (after server redirects, scripts, auth, and cookies)
if (chrome.downloads && chrome.downloads.onDeterminingFilename) {
  chrome.downloads.onDeterminingFilename.addListener((item, suggest) => {
    chrome.storage.local.get(["intercept_enabled"], async (res) => {
      const enabled = res.intercept_enabled !== false;
      if (!enabled) {
        suggest();
        return;
      }

      const url = item.finalUrl || item.url;
      if (!url || url.startsWith("blob:") || url.startsWith("data:")) {
        suggest();
        return;
      }

      if (interceptedDownloadIds.has(item.id)) {
        suggest();
        return;
      }
      interceptedDownloadIds.add(item.id);

      // Cancel the browser's internal download
      try {
        chrome.downloads.cancel(item.id, () => {
          chrome.downloads.erase({ id: item.id });
        });
      } catch (_) {}

      // Tell Chrome filename determination is complete (canceled)
      suggest();

      let rawFilename = item.filename || "";
      let cleanFilename = rawFilename ? rawFilename.split(/[/\\]/).pop() : undefined;
      const referrer = item.referrer || "";
      const cookies = await getCookiesForUrl(url, referrer);

      triggerOpenDlDownload({
        action: "download",
        url: url,
        filename: cleanFilename,
        referrer: referrer,
        cookies: cookies,
        user_agent: navigator.userAgent,
        total_size: (item.fileSize && item.fileSize > 0) ? item.fileSize : undefined,
        headers: {
          "Referer": referrer,
          "User-Agent": navigator.userAgent,
          "Cookie": cookies,
          "Accept-Language": navigator.language || "en-US,en;q=0.9"
        }
      });
    });

    return true; // Asynchronous suggest callback
  });
}

// Deep cookie extraction across target URL, referrer, and parent domain
async function getCookiesForUrl(url, referrer = "") {
  const cookieMap = new Map();

  async function collectCookies(targetUrl) {
    if (!targetUrl || (!targetUrl.startsWith("http://") && !targetUrl.startsWith("https://"))) return;
    try {
      // 1. Direct URL cookies
      const cookies = await chrome.cookies.getAll({ url: targetUrl });
      for (const c of cookies) {
        cookieMap.set(c.name, c.value);
      }

      // 2. Root domain cookies (e.g. .mediafire.com for download123.mediafire.com)
      try {
        const parsed = new URL(targetUrl);
        const parts = parsed.hostname.split(".");
        if (parts.length >= 2) {
          const rootDomain = parts.slice(-2).join(".");
          const domainCookies = await chrome.cookies.getAll({ domain: rootDomain });
          for (const c of domainCookies) {
            if (!cookieMap.has(c.name)) {
              cookieMap.set(c.name, c.value);
            }
          }
        }
      } catch (_) {}
    } catch (_) {}
  }

  await collectCookies(url);
  if (referrer) {
    await collectCookies(referrer);
  }

  const cookieList = [];
  for (const [name, val] of cookieMap.entries()) {
    cookieList.push(`${name}=${val}`);
  }
  return cookieList.join("; ");
}

function triggerOpenDlDownload(payload) {
  try {
    chrome.runtime.sendNativeMessage(NATIVE_HOST, payload, (response) => {
      if (chrome.runtime.lastError) {
        console.warn("OpenDL native host error:", chrome.runtime.lastError.message);
      } else {
        console.log("OpenDL core response:", response);
      }
    });
  } catch (e) {
    console.error("Failed to send native message to OpenDL:", e);
  }
}

// 4. Browser Magnet Link Navigation Interception
if (chrome.webNavigation && chrome.webNavigation.onBeforeNavigate) {
  chrome.webNavigation.onBeforeNavigate.addListener((details) => {
    if (details.url && details.url.startsWith("magnet:")) {
      chrome.storage.local.get(["intercept_enabled"], async (res) => {
        if (res.intercept_enabled === false) return;
        let suggested = "Magnet Download";
        try {
          const match = details.url.match(/dn=([^&]+)/i);
          if (match && match[1]) suggested = decodeURIComponent(match[1].replace(/\+/g, " "));
        } catch (_) {}
        triggerOpenDlDownload({
          action: "download",
          url: details.url,
          filename: suggested,
          referrer: "",
          cookies: "",
          user_agent: navigator.userAgent
        });
      });
    }
  });
}
