import {
  listTasks,
  getAllProgress,
  pauseTask,
  resumeTask,
  deleteTask,
  normalizeTask,
  windowMinimize,
  windowToggleMaximize,
  windowClose,
  windowStartDragging,
  getDiskFree,
  getPendingIntercepts,
  getAllDrives,
  listQueues
} from "./api.js";
import { openMoveQueueDialog } from "./dialogs.js";
import {
  store,
  subscribe,
  setTheme,
  setSidebarCollapsed,
  addQueueName,
  pushThroughputSample,
  pushTaskSpeedSample,
  formatBytes,
  formatSpeedParts,
  getFilteredTasks,
  notify,
  seedOrUpdateWireLogs,
  playPiezo
} from "./state.js";
import { hydrateIcons, getIcon } from "./icons.js";
import { renderTaskTable, initTableInteractions } from "./table.js";
import { renderDock, initDockInteractions } from "./dock.js";
import { renderGraphs, initGraphInteractions } from "./graph.js";
import {
  openAddDialog,
  openSettingsDialog,
  openQueueDialog,
  closeAllDialogs,
  showContextMenu,
  initDialogs
} from "./dialogs.js";
import { openCommandPalette, initCommandPalette } from "./palette.js";

let launchInitActive = true;

function renderSidebarCountsAndQueues() {
  const counts = {
    all: store.tasks.length,
    downloading: store.tasks.filter((t) => t.status === "downloading").length,
    completed: store.tasks.filter((t) => t.status === "finished").length,
    queued: store.tasks.filter((t) => t.status === "queued").length
  };

  for (const [key, val] of Object.entries(counts)) {
    const el = document.querySelector(`[data-count-for="${key}"]`);
    if (el) {
      el.textContent = String(val);
      el.classList.toggle("is-zero", val === 0);
    }
  }

  const categories = ["Compressed", "Programs", "Video", "Music", "Documents"];
  for (const cat of categories) {
    const cVal = store.tasks.filter((t) => t.category === cat).length;
    const el = document.querySelector(`[data-count-cat="${cat}"]`);
    if (el) {
      el.textContent = String(cVal);
      el.classList.toggle("is-zero", cVal === 0);
    }
  }

  const queueList = document.getElementById("nav-queues");
  if (queueList) {
    queueList.innerHTML = store.queues
      .map((q) => {
        const qCount = store.tasks.filter((t) => t.queue === q).length;
        const isActive = store.activeQueueFilter === q;
        return `
          <li>
            <div class="nav-item-queue-row">
              <button type="button" class="nav-item ${isActive ? "is-active" : ""}" data-queue="${q}" data-tooltip="${q}">
                <span class="nav-icon">${getIcon("listTree")}</span>
                <span class="nav-label">${q}</span>
                <span class="nav-count ${qCount === 0 ? "is-zero" : ""}">${qCount}</span>
              </button>
              <button type="button" class="btn-queue-edit" data-queue-edit="${q}" data-tooltip="Configure Queue" title="Configure Queue">
                ${getIcon("settings")}
              </button>
            </div>
          </li>
        `;
      })
      .join("");
  }

  const sidebar = document.getElementById("sidebar");
  sidebar?.classList.toggle("is-collapsed", store.sidebarCollapsed);
}

function renderStatusBar(totalDownSpeed) {
  const led = document.getElementById("status-link-led");
  const label = document.getElementById("status-link-label");
  const downEl = document.getElementById("status-down-rate");
  const activeEl = document.getElementById("status-active-count");
  const queuedEl = document.getElementById("status-queued-count");

  const activeCount = store.tasks.filter((t) => t.status === "downloading").length;
  const queuedCount = store.tasks.filter((t) => t.status === "queued").length;

  if (led) led.setAttribute("data-state", activeCount > 0 ? "downloading" : "idle");

  if (label) {
    if (launchInitActive) {
      label.textContent = `BUS SYNC // ${store.tasks.length} TASKS INDEXED`;
    } else {
      label.textContent = activeCount > 0 ? "LINK ACTIVE" : "IDLE";
    }
  }

  if (downEl) {
    const sp = formatSpeedParts(totalDownSpeed);
    downEl.textContent = totalDownSpeed > 0 ? `${sp.value} ${sp.unit}` : "0 B/s";
  }
  if (activeEl) activeEl.textContent = String(activeCount);
  if (queuedEl) queuedEl.textContent = String(queuedCount);
}

async function refreshTaskList() {
  const list = await listTasks();
  store.tasks = list;
  store.taskMap.clear();
  for (const t of list) store.taskMap.set(t.id, t);
  if (!store.lastSelectedId && list.length > 0) {
    const active = list.find((t) => t.status === "downloading") || list[0];
    store.selectedIds.add(active.id);
    store.lastSelectedId = active.id;
    store.focusedRowId = active.id;
  }
  notify("tasks");
}

async function checkPendingIntercepts() {
  try {
    const pending = await getPendingIntercepts();
    if (pending && pending.length > 0) {
      for (const item of pending) {
        if (item.url) {
          openAddDialog(item.url, item.filename || "");
        }
      }
    }
  } catch (err) {
    console.warn("Failed to check pending intercepts:", err);
  }
}

async function pollProgressTick() {
  await checkPendingIntercepts();
  const progressMap = await getAllProgress();
  let totalSpeed = 0;

  let hasMissing = false;
  for (const id of progressMap.keys()) {
    if (!store.taskMap.has(id)) {
      hasMissing = true;
      break;
    }
  }
  if (hasMissing) {
    await refreshTaskList();
  }

  store.tasks = store.tasks.map((existing) => {
    const patch = progressMap.get(existing.id);
    const updated = patch ? normalizeTask(existing, patch) : existing;

    // Smooth rolling average speed and dynamic ETA
    if (updated.status === "downloading") {
      const prevRolling = existing.rollingSpeed || existing.speed || 0;
      const curSpeed = updated.speed || 0;
      const alpha = 0.3;
      updated.rollingSpeed = prevRolling > 0 ? (alpha * curSpeed + (1 - alpha) * prevRolling) : curSpeed;

      if (updated.size && updated.downloaded < updated.size) {
        const effectiveSpeed = updated.rollingSpeed > 512 ? updated.rollingSpeed : curSpeed;
        if (effectiveSpeed > 512) {
          updated.eta = Math.max(1, Math.ceil((updated.size - updated.downloaded) / effectiveSpeed));
        }
      }
      totalSpeed += updated.speed || 0;
    }

    store.taskMap.set(updated.id, updated);
    seedOrUpdateWireLogs(updated);
    pushTaskSpeedSample(updated.id, updated.speed || 0);
    return updated;
  });

  pushThroughputSample(totalSpeed);
  renderSidebarCountsAndQueues();
  renderTaskTable();
  renderDock();
  renderGraphs(totalSpeed);
  renderStatusBar(totalSpeed);
}

function executeAction(action, arg) {
  if (action === "resume-all") {
    Promise.all(
      store.tasks
        .filter((t) => t.status === "paused" || t.status === "queued" || t.status === "failed")
        .map((t) => resumeTask(t.id))
    ).then(() => pollProgressTick());
  } else if (action === "pause-all") {
    Promise.all(
      store.tasks.filter((t) => t.status === "downloading").map((t) => pauseTask(t.id))
    ).then(() => pollProgressTick());
  } else if (action === "view") {
    store.view = arg;
    store.activeQueueFilter = null;
    store.activeCategoryFilter = null;
    syncNavSelection();
    notify("view");
  }
}

function syncNavSelection() {
  document.querySelectorAll("#nav-library .nav-item").forEach((btn) => {
    btn.classList.toggle(
      "is-active",
      btn.getAttribute("data-view") === store.view && !store.activeQueueFilter && !store.activeCategoryFilter
    );
  });
  document.querySelectorAll("#nav-categories .nav-item").forEach((btn) => {
    btn.classList.toggle("is-active", btn.getAttribute("data-category") === store.activeCategoryFilter);
  });
}

async function updateStorageEstimate() {
  const diskVal = document.getElementById("status-disk-val");
  if (!diskVal) return;

  try {
    const drives = await getAllDrives();
    if (Array.isArray(drives) && drives.length > 0) {
      const enabled = store.settings?.storage?.enabledDrives || drives.map((d) => d.letter);
      const filtered = drives.filter((d) => enabled.includes(d.letter));
      const targetDrives = filtered.length > 0 ? filtered : drives;

      const parts = targetDrives.map((d) => {
        const letter = d.letter.replace(/\\$/, "");
        return `${letter}: ${formatBytes(d.free_bytes)}`;
      });

      diskVal.textContent = parts.join(" · ");
      return;
    }
  } catch (_) {}

  const freeBytes = await getDiskFree();
  if (typeof freeBytes === "number" && freeBytes > 0) {
    diskVal.textContent = formatBytes(freeBytes);
    return;
  }
  diskVal.textContent = "UNAVAILABLE";
}

async function initApp() {
  setTheme(store.theme);
  hydrateIcons();

  document.addEventListener("visibilitychange", () => {
    document.body.classList.toggle("is-document-hidden", document.hidden);
  });

  // Window dragging & controls
  const titlebar = document.getElementById("titlebar");
  titlebar?.addEventListener("mousedown", (e) => {
    if (e.button !== 0) return;
    if (e.target.closest("button, input, select, textarea, .keycap, a")) return;
    if (e.detail === 2) {
      windowToggleMaximize();
      return;
    }
    windowStartDragging();
  });

  document.getElementById("win-minimize")?.addEventListener("click", () => windowMinimize());
  document.getElementById("win-maximize")?.addEventListener("click", () => windowToggleMaximize());
  document.getElementById("win-close")?.addEventListener("click", () => windowClose());

  // Topbar actions
  const themeOrder = ["dark", "light", "amber-day", "blueprint", "system"];
  document.getElementById("btn-theme-cycle")?.addEventListener("click", () => {
    const idx = themeOrder.indexOf(store.theme);
    setTheme(themeOrder[(idx + 1) % themeOrder.length]);
  });
  document.getElementById("btn-settings")?.addEventListener("click", () => openSettingsDialog());
  document.getElementById("btn-palette")?.addEventListener("click", () => openCommandPalette(executeAction));

  const searchInput = document.getElementById("global-search");
  searchInput?.addEventListener("input", () => {
    store.searchQuery = searchInput.value;
    renderTaskTable();
  });

  // Sidebar navigation
  document.getElementById("nav-library")?.addEventListener("click", (e) => {
    const btn = e.target.closest("[data-view]");
    if (!btn) return;
    store.view = btn.getAttribute("data-view");
    store.activeQueueFilter = null;
    store.activeCategoryFilter = null;
    syncNavSelection();
    notify("view");
  });

  document.getElementById("nav-categories")?.addEventListener("click", (e) => {
    const btn = e.target.closest("[data-category]");
    if (!btn) return;
    const cat = btn.getAttribute("data-category");
    store.activeCategoryFilter = store.activeCategoryFilter === cat ? null : cat;
    store.activeQueueFilter = null;
    syncNavSelection();
    notify("view");
  });

  document.getElementById("nav-queues")?.addEventListener("click", async (e) => {
    const editBtn = e.target.closest("[data-queue-edit]");
    if (editBtn) {
      e.stopPropagation();
      const qName = editBtn.getAttribute("data-queue-edit");
      try {
        const queues = await listQueues();
        const match = queues.find((q) => q.name === qName || q.id === qName) || { name: qName, id: qName };
        openQueueDialog(match);
      } catch (_) {
        openQueueDialog({ name: qName, id: qName });
      }
      return;
    }

    const btn = e.target.closest("[data-queue]");
    if (!btn) return;
    const q = btn.getAttribute("data-queue");
    store.activeQueueFilter = store.activeQueueFilter === q ? null : q;
    store.activeCategoryFilter = null;
    syncNavSelection();
    notify("view");
  });

  document.getElementById("btn-add-queue")?.addEventListener("click", () => {
    openQueueDialog();
  });

  document.getElementById("btn-sidebar-toggle")?.addEventListener("click", () => {
    setSidebarCollapsed(!store.sidebarCollapsed);
  });

  // Toolbar buttons
  document.getElementById("btn-add-download")?.addEventListener("click", () => openAddDialog());
  document.getElementById("btn-empty-add")?.addEventListener("click", () => openAddDialog());
  document.getElementById("btn-resume-all")?.addEventListener("click", () => executeAction("resume-all"));
  document.getElementById("btn-pause-all")?.addEventListener("click", () => executeAction("pause-all"));
  document.getElementById("btn-delete-selected")?.addEventListener("click", async () => {
    if (store.selectedIds.size === 0) return;
    const ids = Array.from(store.selectedIds);
    for (const id of ids) {
      await deleteTask(id, false);
      store.selectedIds.delete(id);
    }
    store.tasks = store.tasks.filter((t) => !ids.includes(t.id));
    for (const id of ids) store.taskMap.delete(id);
    await refreshTaskList();
  });

  // Global keyboard shortcuts
  window.addEventListener("keydown", async (e) => {
    const inField = ["INPUT", "TEXTAREA", "SELECT"].includes(document.activeElement?.tagName);

    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "n") {
      e.preventDefault();
      openAddDialog();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
      e.preventDefault();
      searchInput?.focus();
      searchInput?.select();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
      e.preventDefault();
      openCommandPalette(executeAction);
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a" && !inField) {
      e.preventDefault();
      getFilteredTasks().forEach((t) => store.selectedIds.add(t.id));
      notify("selection");
    } else if (e.key === "Delete" && !inField && store.selectedIds.size > 0) {
      e.preventDefault();
      const ids = Array.from(store.selectedIds);
      for (const id of ids) {
        await deleteTask(id, e.shiftKey);
        store.selectedIds.delete(id);
      }
      store.tasks = store.tasks.filter((t) => !ids.includes(t.id));
      for (const id of ids) store.taskMap.delete(id);
      await refreshTaskList();
    } else if (e.key === "Escape") {
      closeAllDialogs();
    }
  });

  initTableInteractions(showContextMenu);
  initDockInteractions();
  initGraphInteractions();
  initDialogs(refreshTaskList);
  initCommandPalette(executeAction);

  subscribe((state, reason) => {
    if (reason === "selection" || reason === "view") {
      playPiezo("tick");
    }
    renderSidebarCountsAndQueues();
    renderTaskTable();
    renderDock();
  });

  await refreshTaskList();
  await pollProgressTick();
  updateStorageEstimate();

  // Wire move to queue button
  document.getElementById("btn-move-queue")?.addEventListener("click", () => {
    const ids = Array.from(store.selectedIds);
    if (ids.length > 0) {
      openMoveQueueDialog(ids);
    }
  });

  window.addEventListener("opendl_drives_changed", () => {
    updateStorageEstimate();
  });

  // Local 1-second countdown tick for running downloads so ETA counts down smoothly
  setInterval(() => {
    let changed = false;
    for (const t of store.tasks) {
      if (t.status === "downloading" && typeof t.eta === "number" && t.eta > 1) {
        t.eta -= 1;
        changed = true;
      }
    }
    if (changed) {
      renderTaskTable();
    }
  }, 1000);

  // Process any downloads intercepted before window finish loaded
  try {
    const pending = await getPendingIntercepts();
    if (pending && pending.length > 0) {
      const first = pending[0];
      if (first.url) {
        openAddDialog(first.url, first.filename || "");
      }
    }
  } catch (err) {
    console.warn("Failed to check pending intercepts:", err);
  }

  setTimeout(() => {
    launchInitActive = false;
    renderStatusBar(store.tasks.reduce((acc, t) => acc + (t.speed || 0), 0));
  }, 500);

  setInterval(() => {
    if (!document.hidden) {
      pollProgressTick();
    }
  }, 500);
}

initApp();

// Extension Interception Listeners
if (typeof window !== "undefined") {
  window.__onBrowserDownloadRequest = (payload) => {
    if (payload && payload.url) {
      openAddDialog(payload.url, payload.filename || "");
    }
  };
}

window.__onBrowserDownloadRequest = (payload) => {
  if (payload && payload.url) {
    openAddDialog(payload.url, payload.filename || "", payload);
  }
};

if (typeof window !== "undefined" && window.__TAURI__?.event?.listen) {
  window.__TAURI__.event.listen("browser_download_request", (event) => {
    if (event.payload && event.payload.url) {
      openAddDialog(event.payload.url, event.payload.filename || "", event.payload);
    }
  });
}

