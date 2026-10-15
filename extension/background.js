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
  const cookies = await getCookiesForUrl(url);

  let suggestedFilename = undefined;
  if (info.srcUrl && tab && tab.title) {
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
    user_agent: navigator.userAgent
  });
});

// 2. In-Page Content Script Click Messages
chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  if (msg && msg.type === "PAGE_LINK_CLICKED") {
    chrome.storage.local.get(["intercept_enabled"], async (res) => {
      const enabled = res.intercept_enabled !== false;
      if (!enabled) {
        sendResponse({ intercepted: false });
        return;
      }

      const cookies = await getCookiesForUrl(msg.url);
      triggerOpenDlDownload({
        action: "download",
        url: msg.url,
        filename: msg.suggestedFilename || undefined,
        referrer: msg.referrer || (sender.tab && sender.tab.url) || "",
        cookies: cookies,
        user_agent: navigator.userAgent
      });

      sendResponse({ intercepted: true });
    });
    return true; // Keep message channel open for async response
  }
});

// 3. Browser Download Interception: Determining Filename (Canonical Chromium hook)
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

      const cookies = await getCookiesForUrl(url);
      triggerOpenDlDownload({
        action: "download",
        url: url,
        filename: item.filename || undefined,
        referrer: item.referrer || "",
        cookies: cookies,
        user_agent: navigator.userAgent,
        total_size: (item.fileSize && item.fileSize > 0) ? item.fileSize : undefined
      });
    });

    return true; // Asynchronous suggest callback
  });
}

async function getCookiesForUrl(url) {
  try {
    const cookies = await chrome.cookies.getAll({ url: url });
    return cookies.map(c => `${c.name}=${c.value}`).join("; ");
  } catch (e) {
    return "";
  }
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
