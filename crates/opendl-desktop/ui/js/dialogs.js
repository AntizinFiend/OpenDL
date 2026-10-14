import {
  store,
  setTheme,
  setClipboardEnabled,
  setPiezoEnabled,
  playPiezo,
  decodeTitleEffect,
  notify
} from "./state.js";
import {
  addTask,
  pauseTask,
  resumeTask,
  deleteTask,
  detectFilenameFromUrl,
  detectCategoryFromFilename,
  openPath,
  showInFolder,
  selectFolder,
  createQueue,
  updateQueue,
  abortShutdown,
  listQueues
} from "./api.js";

const THEMES = [
  { id: "system", name: "System", bg: "#121210", accent: "#F59E0B" },
  { id: "dark", name: "Dark", bg: "#0D0D0C", accent: "#F59E0B" },
  { id: "light", name: "Light", bg: "#F4F5F7", accent: "#181B20" },
  { id: "amber-day", name: "Amber Day", bg: "#F5F3EE", accent: "#F5A400" },
  { id: "blueprint", name: "Blueprint", bg: "#0B1624", accent: "#4FB8F0" }
];

export function openAddDialog(prefillUrl = "") {
  const backdrop = document.getElementById("dialog-add-backdrop");
  const urlsInput = document.getElementById("input-urls");
  const nameInput = document.getElementById("input-filename");
  const catSelect = document.getElementById("select-category");
  const queueSelect = document.getElementById("select-queue");
  const titleEl = document.getElementById("dialog-add-title");

  if (!backdrop) return;
  queueSelect.innerHTML = store.queues.map((q) => `<option value="${q}">${q}</option>`).join("");
  urlsInput.value = prefillUrl;
  if (prefillUrl) {
    const firstLine = prefillUrl.split(/\r?\n/).find((l) => l.trim());
    const fn = detectFilenameFromUrl(firstLine);
    nameInput.value = fn;
    catSelect.value = detectCategoryFromFilename(fn);
  } else {
    nameInput.value = "";
  }

  backdrop.classList.remove("is-hidden");
  decodeTitleEffect(titleEl, "NEW DOWNLOAD");
  urlsInput.focus();
}

export function openSettingsDialog() {
  const backdrop = document.getElementById("dialog-settings-backdrop");
  const grid = document.getElementById("theme-picker-grid");
  const clipToggle = document.getElementById("toggle-clipboard");
  const piezoToggle = document.getElementById("toggle-piezo");
  const titleEl = document.getElementById("dialog-settings-title");
  if (!backdrop || !grid) return;

  renderThemePicker(grid);
  if (clipToggle) clipToggle.checked = store.clipboardEnabled;
  if (piezoToggle) piezoToggle.checked = store.piezoEnabled;
  playPiezo("snap");
  backdrop.classList.remove("is-hidden");
  decodeTitleEffect(titleEl, "SYSTEM CONFIGURATION");
}

export function openQueueDialog(queueRecord = null) {
  const backdrop = document.getElementById("dialog-queue-backdrop");
  const editId = document.getElementById("queue-edit-id");
  const nameInput = document.getElementById("queue-name");
  const maxConns = document.getElementById("queue-max-concurrent");
  const speedLimit = document.getElementById("queue-speed-limit");
  const onFinish = document.getElementById("queue-on-finish");
  const titleEl = document.getElementById("dialog-queue-title");

  if (!backdrop) return;

  if (queueRecord) {
    if (editId) editId.value = queueRecord.id || "";
    if (nameInput) nameInput.value = queueRecord.name || "";
    if (maxConns) maxConns.value = queueRecord.max_concurrent || 2;
    if (speedLimit) speedLimit.value = queueRecord.speed_limit_kbps || 0;
    if (onFinish) onFinish.value = queueRecord.on_complete_action || "none";
    decodeTitleEffect(titleEl, "EDIT QUEUE CONFIGURATION");
  } else {
    if (editId) editId.value = "";
    if (nameInput) nameInput.value = `Queue 0${store.queues.length + 1}`;
    if (maxConns) maxConns.value = 2;
    if (speedLimit) speedLimit.value = 0;
    if (onFinish) onFinish.value = "none";
    decodeTitleEffect(titleEl, "NEW QUEUE CONFIGURATION");
  }

  playPiezo("snap");
  backdrop.classList.remove("is-hidden");
  nameInput?.focus();
}

function renderThemePicker(grid) {
  grid.innerHTML = THEMES.map((t) => `
    <button
      type="button"
      class="theme-card ${store.theme === t.id ? "is-active" : ""}"
      role="radio"
      aria-checked="${store.theme === t.id}"
      data-theme-pick="${t.id}"
    >
      <div class="theme-swatch-row">
        <span class="swatch-bg" style="background:${t.bg}"></span>
        <span class="swatch-accent" style="background:${t.accent}"></span>
      </div>
      <span class="micro-label">${t.name}</span>
    </button>
  `).join("");

  grid.querySelectorAll("[data-theme-pick]").forEach((btn) => {
    btn.addEventListener("click", () => {
      const id = btn.getAttribute("data-theme-pick");
      setTheme(id);
      renderThemePicker(grid);
    });
  });
}

export function closeAllDialogs() {
  document.querySelectorAll(".modal-backdrop").forEach((el) => el.classList.add("is-hidden"));
  hideContextMenu();
}

export function showContextMenu(x, y, taskId) {
  const menu = document.getElementById("context-menu");
  if (!menu) return;
  const task = store.taskMap.get(taskId);
  if (!task) return;

  menu.innerHTML = `
    <li class="context-menu-item" role="menuitem" data-ctx="pause">
      <span>Pause</span><span class="micro-label">SPACE</span>
    </li>
    <li class="context-menu-item" role="menuitem" data-ctx="resume">
      <span>Resume</span><span class="micro-label">SPACE</span>
    </li>
    <li class="context-menu-item" role="menuitem" data-ctx="retry">
      <span>Retry</span><span class="micro-label">CTRL+R</span>
    </li>
    <li class="context-menu-sep" role="separator"></li>
    <li class="context-menu-item" role="menuitem" data-ctx="open">
      <span>Open file</span><span class="micro-label">ENTER</span>
    </li>
    <li class="context-menu-item" role="menuitem" data-ctx="folder">
      <span>Open folder</span><span class="micro-label">CTRL+O</span>
    </li>
    <li class="context-menu-item" role="menuitem" data-ctx="copy">
      <span>Copy link</span><span class="micro-label">CTRL+C</span>
    </li>
    <li class="context-menu-sep" role="separator"></li>
    <li class="context-menu-item" role="menuitem" data-ctx="remove">
      <span>Remove</span><span class="micro-label">DEL</span>
    </li>
    <li class="context-menu-item" role="menuitem" data-ctx="delete-disk">
      <span>Remove and delete file</span><span class="micro-label">SHIFT+DEL</span>
    </li>
  `;

  menu.classList.remove("is-hidden");
  const clampedX = Math.min(x, window.innerWidth - 240);
  const clampedY = Math.min(y, window.innerHeight - 280);
  menu.style.left = `${clampedX}px`;
  menu.style.top = `${clampedY}px`;

  menu.onclick = async (e) => {
    const item = e.target.closest("[data-ctx]");
    if (!item) return;
    const act = item.getAttribute("data-ctx");
    hideContextMenu();
    const ids = store.selectedIds.size > 0 ? Array.from(store.selectedIds) : [taskId];

    if (act === "pause") {
      for (const id of ids) await pauseTask(id);
    } else if (act === "resume" || act === "retry") {
      for (const id of ids) await resumeTask(id);
    } else if (act === "open") {
      if (task.destination) await openPath(task.destination);
    } else if (act === "folder") {
      if (task.destination) await showInFolder(task.destination);
    } else if (act === "copy" && task.url) {
      try { await navigator.clipboard.writeText(task.url); } catch {}
    } else if (act === "remove") {
      for (const id of ids) {
        await deleteTask(id, false);
        store.selectedIds.delete(id);
      }
    } else if (act === "delete-disk") {
      for (const id of ids) {
        await deleteTask(id, true);
        store.selectedIds.delete(id);
      }
    }
    notify("tasks");
  };
}

export function hideContextMenu() {
  document.getElementById("context-menu")?.classList.add("is-hidden");
}

export function initDialogs(onTasksAdded) {
  document.querySelectorAll("[data-close-modal]").forEach((btn) => {
    btn.addEventListener("click", () => {
      const id = btn.getAttribute("data-close-modal");
      document.getElementById(id)?.classList.add("is-hidden");
    });
  });

  document.querySelectorAll(".modal-backdrop").forEach((backdrop) => {
    backdrop.addEventListener("click", (e) => {
      if (e.target === backdrop) backdrop.classList.add("is-hidden");
    });
  });

  const urlsInput = document.getElementById("input-urls");
  const nameInput = document.getElementById("input-filename");
  const catSelect = document.getElementById("select-category");

  urlsInput?.addEventListener("input", () => {
    const lines = urlsInput.value.split(/\r?\n/).map((s) => s.trim()).filter(Boolean);
    if (lines.length === 1) {
      const detected = detectFilenameFromUrl(lines[0]);
      nameInput.value = detected;
      catSelect.value = detectCategoryFromFilename(detected);
    } else if (lines.length > 1) {
      nameInput.value = `Batch (${lines.length} URIs)`;
    }
  });

  const form = document.getElementById("form-add-download");
  form?.addEventListener("submit", async (e) => {
    e.preventDefault();
    const submitter = e.submitter;
    const startNow = submitter?.value !== "queue";
    const lines = urlsInput.value.split(/\r?\n/).map((s) => s.trim()).filter(Boolean);
    const category = catSelect.value;
    const queue = document.getElementById("select-queue")?.value || "Main Queue";
    const folder = document.getElementById("input-folder")?.value || "";

    for (let i = 0; i < lines.length; i++) {
      const customName = lines.length === 1 ? nameInput.value.trim() : "";
      await addTask({
        url: lines[i],
        name: customName,
        category,
        queue,
        folder,
        startNow
      });
    }
    document.getElementById("dialog-add-backdrop")?.classList.add("is-hidden");
    await onTasksAdded?.();
  });

  const clipToggle = document.getElementById("toggle-clipboard");
  clipToggle?.addEventListener("change", (e) => {
    setClipboardEnabled(e.target.checked);
  });

  const piezoToggle = document.getElementById("toggle-piezo");
  piezoToggle?.addEventListener("change", (e) => {
    setPiezoEnabled(e.target.checked);
  });

  const browseBtn = document.getElementById("btn-browse-folder");
  browseBtn?.addEventListener("click", async () => {
    const cur = document.getElementById("input-folder")?.value;
    const selected = await selectFolder(cur);
    if (selected) {
      const folderInput = document.getElementById("input-folder");
      if (folderInput) folderInput.value = selected;
    }
  });

  // Drag & drop URLs across full window
  const dropOverlay = document.getElementById("drop-overlay");
  let dragDepth = 0;

  window.addEventListener("dragenter", (e) => {
    e.preventDefault();
    dragDepth++;
    dropOverlay?.classList.remove("is-hidden");
  });

  window.addEventListener("dragover", (e) => {
    e.preventDefault();
  });

  window.addEventListener("dragleave", () => {
    dragDepth = Math.max(0, dragDepth - 1);
    if (dragDepth === 0) dropOverlay?.classList.add("is-hidden");
  });

  window.addEventListener("drop", (e) => {
    e.preventDefault();
    dragDepth = 0;
    dropOverlay?.classList.add("is-hidden");
    const uri = e.dataTransfer?.getData("text/uri-list") || e.dataTransfer?.getData("text/plain") || "";
    if (uri.trim()) openAddDialog(uri.trim());
  });

  // Clipboard link monitor
  const banner = document.getElementById("clipboard-banner");
  const preview = document.getElementById("clipboard-url-preview");
  let currentClipUrl = "";

  window.addEventListener("focus", async () => {
    if (!store.clipboardEnabled) return;
    try {
      const text = (await navigator.clipboard.readText())?.trim();
      if (text && /^https?:\/\/\S+/i.test(text) && text !== store.dismissedClipboardUrl) {
        const alreadyExists = store.tasks.some((t) => t.url === text);
        if (!alreadyExists) {
          currentClipUrl = text;
          if (preview) preview.textContent = text;
          banner?.classList.remove("is-hidden");
        }
      }
    } catch {
      // Clipboard permission not granted in non-focused state
    }
  });

  document.getElementById("btn-clipboard-download")?.addEventListener("click", () => {
    banner?.classList.add("is-hidden");
    store.dismissedClipboardUrl = currentClipUrl;
    openAddDialog(currentClipUrl);
  });

  document.getElementById("btn-clipboard-dismiss")?.addEventListener("click", () => {
    banner?.classList.add("is-hidden");
    store.dismissedClipboardUrl = currentClipUrl;
  });

  document.getElementById("btn-save-queue")?.addEventListener("click", async () => {
    const editId = document.getElementById("queue-edit-id")?.value;
    const name = document.getElementById("queue-name")?.value.trim();
    const maxConns = parseInt(document.getElementById("queue-max-concurrent")?.value || "2", 10);
    const speedLimit = parseInt(document.getElementById("queue-speed-limit")?.value || "0", 10);
    const onFinish = document.getElementById("queue-on-finish")?.value || "none";

    if (!name) return;

    if (editId) {
      await updateQueue({
        id: editId,
        name,
        max_concurrent: maxConns,
        speed_limit_kbps: speedLimit,
        is_active: true,
        schedule_start: null,
        schedule_stop: null,
        created_at: Date.now(),
        on_complete_action: onFinish,
      });
    } else {
      const q = await createQueue(name, maxConns);
      if (q) {
        q.speed_limit_kbps = speedLimit;
        q.on_complete_action = onFinish;
        await updateQueue(q);
      }
    }

    playPiezo("thud");
    document.getElementById("dialog-queue-backdrop")?.classList.add("is-hidden");
    await onTasksAdded?.();
    notify("view");
  });

  document.getElementById("btn-abort-shutdown")?.addEventListener("click", async () => {
    await abortShutdown();
    document.getElementById("shutdown-countdown-banner")?.classList.add("is-hidden");
    playPiezo("snap");
  });

  window.addEventListener("click", () => hideContextMenu());
}