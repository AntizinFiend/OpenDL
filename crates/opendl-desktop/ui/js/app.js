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
  getDiskFree
} from "./api.js";
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
            <button type="button" class="nav-item ${isActive ? "is-active" : ""}" data-queue="${q}" data-tooltip="${q}">
              <span class="nav-icon">${getIcon("listTree")}</span>
              <span class="nav-label">${q}</span>
              <span class="nav-count ${qCount === 0 ? "is-zero" : ""}">${qCount}</span>
            </button>
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

async function pollProgressTick() {
  const progressMap = await getAllProgress();
  let totalSpeed = 0;

  store.tasks = store.tasks.map((existing) => {
    const patch = progressMap.get(existing.id);
    const updated = patch ? normalizeTask(existing, patch) : existing;
    store.taskMap.set(updated.id, updated);
    seedOrUpdateWireLogs(updated);
    pushTaskSpeedSample(updated.id, updated.speed || 0);
    if (updated.status === "downloading") {
      totalSpeed += updated.speed || 0;
    }
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
  const freeBytes = await getDiskFree();
  if (typeof freeBytes === "number" && freeBytes > 0) {
    diskVal.textContent = formatBytes(freeBytes);
    return;
  }
  if (navigator.storage && typeof navigator.storage.estimate === "function") {
    try {
      const est = await navigator.storage.estimate();
      if (est && typeof est.quota === "number" && typeof est.usage === "number") {
        const free = Math.max(0, est.quota - est.usage);
        diskVal.textContent = formatBytes(free);
      }
    } catch {
      diskVal.textContent = "UNAVAILABLE";
    }
  }
}

async function initApp() {
  setTheme(store.theme);
  hydrateIcons();

  document.addEventListener("visibilitychange", () => {
    document.body.classList.toggle("is-document-hidden", document.hidden);
  });

  // Window controls
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

  document.getElementById("nav-queues")?.addEventListener("click", (e) => {
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

// Extension Interception Listener
if (typeof window !== "undefined" && window.__TAURI__?.event?.listen) {
  window.__TAURI__.event.listen("browser_download_request", (event) => {
    if (event.payload && event.payload.url) {
      openAddDialog(event.payload.url);
    }
  });
}
