const toggle = document.getElementById("toggleIntercept");
const statusBadge = document.getElementById("hostStatus");
const pingBtn = document.getElementById("testPing");
const extIdSpan = document.getElementById("extId");
const errorDetails = document.getElementById("errorDetails");

if (extIdSpan) {
  extIdSpan.textContent = chrome.runtime.id;
}

chrome.storage.local.get(["intercept_enabled"], (res) => {
  toggle.checked = res.intercept_enabled !== false;
});

toggle.addEventListener("change", () => {
  chrome.storage.local.set({ intercept_enabled: toggle.checked });
});

function checkConnection() {
  statusBadge.textContent = "Checking...";
  statusBadge.className = "status-badge";
  if (errorDetails) errorDetails.textContent = "";

  try {
    chrome.runtime.sendNativeMessage("org.opendl.native", { action: "ping" }, (res) => {
      if (chrome.runtime.lastError) {
        const err = chrome.runtime.lastError.message || "Unknown error";
        console.warn("OpenDL native messaging error:", err);
        statusBadge.textContent = "Offline";
        statusBadge.className = "status-badge";
        if (errorDetails) errorDetails.textContent = err;
      } else if (res && res.status === "ok") {
        statusBadge.textContent = "Connected";
        statusBadge.className = "status-badge online";
        if (errorDetails) errorDetails.textContent = "";
      } else {
        console.warn("OpenDL unexpected response:", res);
        statusBadge.textContent = "Offline";
        statusBadge.className = "status-badge";
        if (errorDetails) errorDetails.textContent = "Unexpected response from host";
      }
    });
  } catch (e) {
    statusBadge.textContent = "Offline";
    statusBadge.className = "status-badge";
    if (errorDetails) errorDetails.textContent = e.message;
  }
}

pingBtn.addEventListener("click", checkConnection);
checkConnection();
