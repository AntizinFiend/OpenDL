/**
 * OpenDL - Comprehensive Settings Controller
 */

import { store, updateSettings, decodeTitleEffect, setTheme } from "./state.js";
import { getAllDrives, selectFolder } from "./api.js";

let activeTab = "general";
let detectedDrives = [];

export async function initSettings() {
  const dialog = document.getElementById("dialog-settings");
  if (!dialog) return;

  // Query connected drives
  try {
    detectedDrives = await getAllDrives();
  } catch (e) {
    detectedDrives = [
      { letter: "C:\\", label: "Local Disk (C:)", total_bytes: 119047157760, free_bytes: 20080939008, drive_type: "fixed" }
    ];
  }

  // Ensure enabledDrives in settings has a valid baseline
  if (!store.settings.storage || !Array.isArray(store.settings.storage.enabledDrives)) {
    store.settings.storage = {
      ...store.settings.storage,
      enabledDrives: detectedDrives.map((d) => d.letter)
    };
  }

  setupSettingsDom();
}

export async function openSettingsDialog(initialTab = "general") {
  const backdrop = document.getElementById("dialog-settings-backdrop");
  if (!backdrop) return;

  activeTab = initialTab;

  // Refresh drives
  try {
    detectedDrives = await getAllDrives();
  } catch (_) {}

  renderSettingsView();
  backdrop.classList.remove("is-hidden");

  const titleEl = document.getElementById("dialog-settings-title");
  if (titleEl) {
    decodeTitleEffect(titleEl, "SETTINGS");
  }
}

function setupSettingsDom() {
  const container = document.getElementById("dialog-settings");
  if (!container) return;

  // Wire close buttons
  container.querySelectorAll("[data-close-modal]").forEach((btn) => {
    btn.addEventListener("click", () => {
      document.getElementById("dialog-settings-backdrop")?.classList.add("is-hidden");
    });
  });
}

function formatBytesSimple(bytes) {
  if (bytes == null || bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${units[i]}`;
}

export function renderSettingsView() {
  const body = document.querySelector("#dialog-settings .modal-body");
  if (!body) return;

  const tabs = [
    { id: "general", label: "General", icon: "settings" },
    { id: "storage", label: "Drives & Storage", icon: "disc" },
    { id: "network", label: "Speed & Network", icon: "activity" },
    { id: "queues", label: "Queues & Power", icon: "layers" },
    { id: "bittorrent", label: "BitTorrent & Magnet", icon: "download" },
    { id: "appearance", label: "Appearance & Audio", icon: "sunMoon" }
  ];

  body.innerHTML = `
    <div class="settings-layout">
      <nav class="settings-nav" role="tablist">
        ${tabs
          .map(
            (t) => `
          <button type="button" class="settings-nav-btn ${t.id === activeTab ? "is-active" : ""}" data-settings-tab="${t.id}" role="tab">
            <span>${t.label}</span>
          </button>
        `
          )
          .join("")}
      </nav>
      <div class="settings-content" role="tabpanel">
        ${renderActiveTabPanel()}
      </div>
    </div>
  `;

  // Attach tab click listeners
  body.querySelectorAll("[data-settings-tab]").forEach((btn) => {
    btn.addEventListener("click", () => {
      activeTab = btn.getAttribute("data-settings-tab");
      renderSettingsView();
    });
  });

  attachSettingsFormListeners(body);
}

function renderActiveTabPanel() {
  const s = store.settings;

  if (activeTab === "general") {
    return `
      <div class="settings-section">
        <h3 class="settings-section-title">General Preferences</h3>
        
        <div class="form-field">
          <label class="micro-label">DEFAULT DOWNLOAD FOLDER</label>
          <div style="display: flex; gap: 8px;">
            <input id="set-general-dir" class="mono-input" type="text" value="${s.general?.downloadDir || ""}" style="flex: 1;" />
            <button type="button" id="btn-set-browse-dir" class="btn btn-ghost" style="padding: 0 12px;">Browse...</button>
          </div>
        </div>

        <div class="form-field">
          <label class="toggle-row" for="set-general-autostart">
            <span class="toggle-label-text">Start downloads immediately when added</span>
            <input id="set-general-autostart" type="checkbox" ${s.general?.autoStartDownloads !== false ? "checked" : ""} />
          </label>
        </div>

        <div class="form-field">
          <label class="toggle-row" for="set-general-confirmdelete">
            <span class="toggle-label-text">Confirm before deleting files from disk</span>
            <input id="set-general-confirmdelete" type="checkbox" ${s.general?.confirmDelete !== false ? "checked" : ""} />
          </label>
        </div>

        <div class="form-field">
          <label class="toggle-row" for="set-general-clipboard">
            <span class="toggle-label-text">Clipboard monitor: automatically detect copied URLs and magnets</span>
            <input id="set-general-clipboard" type="checkbox" ${store.clipboardEnabled ? "checked" : ""} />
          </label>
        </div>
      </div>
    `;
  }

  if (activeTab === "storage") {
    const enabledDrives = s.storage?.enabledDrives || detectedDrives.map((d) => d.letter);
    return `
      <div class="settings-section">
        <h3 class="settings-section-title">Drives & Free Space Display</h3>
        <p class="settings-desc">Choose which connected system drives to display in the status bar:</p>
        
        <div class="drive-list-container">
          ${detectedDrives
            .map((d) => {
              const isChecked = enabledDrives.includes(d.letter);
              return `
              <label class="drive-item-card">
                <input type="checkbox" class="drive-toggle-checkbox" data-drive-letter="${d.letter}" ${isChecked ? "checked" : ""} />
                <div class="drive-item-info">
                  <div class="drive-item-name">
                    <strong>${d.letter}</strong> <span class="drive-item-label">${d.label || "Local Disk"}</span>
                  </div>
                  <div class="drive-item-meta mono-val">
                    ${formatBytesSimple(d.free_bytes)} free of ${formatBytesSimple(d.total_bytes)}
                  </div>
                </div>
              </label>
            `;
            })
            .join("")}
        </div>

        <div class="form-field" style="margin-top: 16px;">
          <label class="micro-label">LOW DISK SPACE WARNING THRESHOLD</label>
          <select id="set-storage-threshold" class="mono-select" style="max-width: 220px;">
            <option value="2" ${s.storage?.lowSpaceThresholdGb === 2 ? "selected" : ""}>2 GB</option>
            <option value="5" ${s.storage?.lowSpaceThresholdGb === 5 ? "selected" : ""}>5 GB</option>
            <option value="10" ${s.storage?.lowSpaceThresholdGb === 10 || !s.storage?.lowSpaceThresholdGb ? "selected" : ""}>10 GB</option>
            <option value="20" ${s.storage?.lowSpaceThresholdGb === 20 ? "selected" : ""}>20 GB</option>
          </select>
        </div>

        <div class="form-field">
          <label class="toggle-row" for="set-storage-preallocate">
            <span class="toggle-label-text">Pre-allocate disk space instantly (zero-fill avoidance / sparse allocation)</span>
            <input id="set-storage-preallocate" type="checkbox" ${s.storage?.preallocateDiskSpace !== false ? "checked" : ""} />
          </label>
        </div>
      </div>
    `;
  }

  if (activeTab === "network") {
    return `
      <div class="settings-section">
        <h3 class="settings-section-title">Speed & Network Tuning</h3>

        <div class="form-row-2col">
          <div class="form-field">
            <label class="micro-label">MAX CONCURRENT DOWNLOADS</label>
            <input id="set-net-concurrent" class="mono-input" type="number" min="1" max="20" value="${s.network?.maxConcurrentDownloads || 5}" />
          </div>
          <div class="form-field">
            <label class="micro-label">MAX CONNECTIONS PER DOWNLOAD</label>
            <input id="set-net-connections" class="mono-input" type="number" min="1" max="32" value="${s.network?.maxConnectionsPerDownload || 16}" />
          </div>
        </div>

        <div class="form-row-2col">
          <div class="form-field">
            <label class="micro-label">GLOBAL DOWNLOAD LIMIT (KB/s, 0 = UNLIMITED)</label>
            <input id="set-net-limit-dl" class="mono-input" type="number" min="0" step="100" value="${s.network?.globalDownloadLimitKbps || 0}" />
          </div>
          <div class="form-field">
            <label class="micro-label">GLOBAL UPLOAD LIMIT (KB/s, 0 = UNLIMITED)</label>
            <input id="set-net-limit-ul" class="mono-input" type="number" min="0" step="100" value="${s.network?.globalUploadLimitKbps || 0}" />
          </div>
        </div>

        <div class="form-field">
          <label class="micro-label">SOCKET TIMEOUT (SECONDS)</label>
          <input id="set-net-timeout" class="mono-input" type="number" min="5" max="120" value="${s.network?.timeoutSeconds || 30}" style="max-width: 220px;" />
        </div>
      </div>
    `;
  }

  if (activeTab === "queues") {
    return `
      <div class="settings-section">
        <h3 class="settings-section-title">Multi-Queue Automation</h3>
        <p class="settings-desc">Configure automated batch queues and post-completion power states:</p>

        <div class="form-field">
          <label class="micro-label">DEFAULT QUEUE FOR NEW DOWNLOADS</label>
          <select id="set-queue-default" class="mono-select" style="max-width: 240px;">
            <option value="main">Main Queue</option>
            <option value="night">Nightly Batch</option>
          </select>
        </div>

        <div class="form-field">
          <label class="micro-label">WHEN QUEUE COMPLETES</label>
          <select id="set-queue-power" class="mono-select" style="max-width: 240px;">
            <option value="none">Do nothing</option>
            <option value="sleep">Sleep PC</option>
            <option value="hibernate">Hibernate PC</option>
            <option value="shutdown">Shut down PC</option>
          </select>
        </div>
      </div>
    `;
  }

  if (activeTab === "bittorrent") {
    return `
      <div class="settings-section">
        <h3 class="settings-section-title">BitTorrent & Magnet Configuration</h3>
        <p class="settings-desc">Native aria2 BitTorrent engine configuration:</p>

        <div class="form-field">
          <label class="toggle-row" for="set-bt-enabled">
            <span class="toggle-label-text">Enable BitTorrent & Magnet link protocol handler</span>
            <input id="set-bt-enabled" type="checkbox" ${s.bittorrent?.enabled !== false ? "checked" : ""} />
          </label>
        </div>

        <div class="form-row-2col">
          <div class="form-field">
            <label class="micro-label">MAX BITTORENT PEERS PER TORRENT</label>
            <input id="set-bt-peers" class="mono-input" type="number" min="10" max="200" value="${s.bittorrent?.maxPeers || 60}" />
          </div>
          <div class="form-field">
            <label class="micro-label">BITTORENT LISTENING PORT</label>
            <input id="set-bt-port" class="mono-input" type="number" min="1024" max="65535" value="${s.bittorrent?.listenPort || 6881}" />
          </div>
        </div>

        <div class="form-field">
          <label class="toggle-row" for="set-bt-dht">
            <span class="toggle-label-text">Enable Distributed Hash Table (DHT) and Peer Exchange (PEX)</span>
            <input id="set-bt-dht" type="checkbox" ${s.bittorrent?.enableDht !== false ? "checked" : ""} />
          </label>
        </div>

        <div class="form-row-2col">
          <div class="form-field">
            <label class="toggle-row" for="set-bt-seed">
              <span class="toggle-label-text">Seed torrents after completion</span>
              <input id="set-bt-seed" type="checkbox" ${s.bittorrent?.seedAfterComplete !== false ? "checked" : ""} />
            </label>
          </div>
          <div class="form-field">
            <label class="micro-label">DEFAULT SEED RATIO (0 = UNLIMITED)</label>
            <input id="set-bt-ratio" class="mono-input" type="number" min="0" max="100" step="0.1" value="${s.bittorrent?.seedRatio ?? 1.0}" />
          </div>
        </div>
      </div>
    `;
  }

  if (activeTab === "appearance") {
    return `
      <div class="settings-section">
        <h3 class="settings-section-title">Theme & Interface Customization</h3>

        <div class="form-field">
          <label class="micro-label">THEME PALETTE</label>
          <div id="set-theme-picker" class="theme-picker-grid" role="radiogroup">
            <label class="theme-card ${store.theme === "dark" ? "is-selected" : ""}" data-theme-val="dark">
              <span class="theme-preview dark-preview"></span>
              <span class="theme-card-name">Graphite Dark</span>
            </label>
            <label class="theme-card ${store.theme === "light" ? "is-selected" : ""}" data-theme-val="light">
              <span class="theme-preview light-preview"></span>
              <span class="theme-card-name">Solar Light</span>
            </label>
            <label class="theme-card ${store.theme === "oled" ? "is-selected" : ""}" data-theme-val="oled">
              <span class="theme-preview oled-preview"></span>
              <span class="theme-card-name">Pure OLED</span>
            </label>
            <label class="theme-card ${store.theme === "cyberpunk" ? "is-selected" : ""}" data-theme-val="cyberpunk">
              <span class="theme-preview cyberpunk-preview"></span>
              <span class="theme-card-name">High-Contrast Amber</span>
            </label>
          </div>
        </div>

        <div class="form-field">
          <label class="micro-label">TEXT DECODING SCRAMBLE ANIMATION</label>
          <select id="set-app-anim" class="mono-select" style="max-width: 240px;">
            <option value="0" ${s.appearance?.animationDuration === 0 ? "selected" : ""}>Disabled (Instant)</option>
            <option value="350" ${s.appearance?.animationDuration === 350 ? "selected" : ""}>Fast (350ms)</option>
            <option value="650" ${s.appearance?.animationDuration === 650 || !s.appearance?.animationDuration ? "selected" : ""}>Standard (650ms)</option>
            <option value="1000" ${s.appearance?.animationDuration === 1000 ? "selected" : ""}>Cinematic (1000ms)</option>
          </select>
        </div>

        <div class="form-field">
          <label class="toggle-row" for="set-app-piezo">
            <span class="toggle-label-text">Audio feedback (chime on download completion & UI clicks)</span>
            <input id="set-app-piezo" type="checkbox" ${store.piezoEnabled ? "checked" : ""} />
          </label>
        </div>
      </div>
    `;
  }

  return "";
}

function attachSettingsFormListeners(container) {
  // General Tab
  const btnBrowseDir = container.querySelector("#btn-set-browse-dir");
  const inputDir = container.querySelector("#set-general-dir");
  btnBrowseDir?.addEventListener("click", async () => {
    const chosen = await selectFolder(inputDir?.value);
    if (chosen && inputDir) {
      inputDir.value = chosen;
      updateSettings({ general: { ...store.settings.general, downloadDir: chosen } });
    }
  });
  inputDir?.addEventListener("change", () => {
    updateSettings({ general: { ...store.settings.general, downloadDir: inputDir.value } });
  });

  const chkAutostart = container.querySelector("#set-general-autostart");
  chkAutostart?.addEventListener("change", () => {
    updateSettings({ general: { ...store.settings.general, autoStartDownloads: chkAutostart.checked } });
  });

  const chkConfirm = container.querySelector("#set-general-confirmdelete");
  chkConfirm?.addEventListener("change", () => {
    updateSettings({ general: { ...store.settings.general, confirmDelete: chkConfirm.checked } });
  });

  // Storage Tab
  const driveCheckboxes = container.querySelectorAll(".drive-toggle-checkbox");
  driveCheckboxes.forEach((cb) => {
    cb.addEventListener("change", () => {
      const selected = Array.from(container.querySelectorAll(".drive-toggle-checkbox:checked")).map((el) =>
        el.getAttribute("data-drive-letter")
      );
      updateSettings({ storage: { ...store.settings.storage, enabledDrives: selected } });
      window.dispatchEvent(new CustomEvent("opendl_drives_changed"));
    });
  });

  const selThreshold = container.querySelector("#set-storage-threshold");
  selThreshold?.addEventListener("change", () => {
    updateSettings({ storage: { ...store.settings.storage, lowSpaceThresholdGb: Number(selThreshold.value) } });
  });

  // Network Tab
  const numConcurrent = container.querySelector("#set-net-concurrent");
  numConcurrent?.addEventListener("change", () => {
    updateSettings({ network: { ...store.settings.network, maxConcurrentDownloads: Number(numConcurrent.value) } });
  });
  const numConnections = container.querySelector("#set-net-connections");
  numConnections?.addEventListener("change", () => {
    updateSettings({ network: { ...store.settings.network, maxConnectionsPerDownload: Number(numConnections.value) } });
  });
  const numDlLimit = container.querySelector("#set-net-limit-dl");
  numDlLimit?.addEventListener("change", () => {
    updateSettings({ network: { ...store.settings.network, globalDownloadLimitKbps: Number(numDlLimit.value) } });
  });
  const numUlLimit = container.querySelector("#set-net-limit-ul");
  numUlLimit?.addEventListener("change", () => {
    updateSettings({ network: { ...store.settings.network, globalUploadLimitKbps: Number(numUlLimit.value) } });
  });

  // BitTorrent Tab
  const chkBtEnabled = container.querySelector("#set-bt-enabled");
  chkBtEnabled?.addEventListener("change", () => {
    updateSettings({ bittorrent: { ...store.settings.bittorrent, enabled: chkBtEnabled.checked } });
  });
  const chkBtDht = container.querySelector("#set-bt-dht");
  chkBtDht?.addEventListener("change", () => {
    updateSettings({ bittorrent: { ...store.settings.bittorrent, enableDht: chkBtDht.checked } });
  });
  const chkBtSeed = container.querySelector("#set-bt-seed");
  chkBtSeed?.addEventListener("change", () => {
    updateSettings({ bittorrent: { ...store.settings.bittorrent, seedAfterComplete: chkBtSeed.checked } });
  });
  const inputBtRatio = container.querySelector("#set-bt-ratio");
  inputBtRatio?.addEventListener("change", () => {
    updateSettings({ bittorrent: { ...store.settings.bittorrent, seedRatio: Number(inputBtRatio.value) } });
  });

  // Appearance Tab
  container.querySelectorAll("[data-theme-val]").forEach((card) => {
    card.addEventListener("click", () => {
      const themeId = card.getAttribute("data-theme-val");
      setTheme(themeId);
      updateSettings({ appearance: { ...store.settings.appearance, theme: themeId } });
      container.querySelectorAll("[data-theme-val]").forEach((c) => c.classList.remove("is-selected"));
      card.classList.add("is-selected");
    });
  });

  const selAnim = container.querySelector("#set-app-anim");
  selAnim?.addEventListener("change", () => {
    updateSettings({ appearance: { ...store.settings.appearance, animationDuration: Number(selAnim.value) } });
  });
}
