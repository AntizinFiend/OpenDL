/**
 * OpenDL - Context Menu, Clipboard Monitoring & Drag-and-Drop Overlay
 */

import { state } from "./state.js";
import { api } from "./api.js";

export function initContextMenu(openAddModalWithUrlFn) {
  const contextMenu = document.getElementById("contextMenu");
  const clipboardToast = document.getElementById("clipboardToast");
  const toastUrlText = document.getElementById("toastUrlText");
  const btnToastDownload = document.getElementById("btnToastDownload");
  const btnToastDismiss = document.getElementById("btnToastDismiss");
  const dragOverlay = document.getElementById("dragOverlay");

  let currentTargetTaskId = null;
  let lastSeenClipboardText = "";

  // -----------------------------------------------------------------
  // 1. Right-Click Context Menu
  // -----------------------------------------------------------------
  window.addEventListener("contextmenu", (e) => {
    const row = e.target.closest(".task-row");
    if (!row) {
      hideContextMenu();
      return;
    }

    e.preventDefault();
    const taskId = row.getAttribute("data-task-id");
    currentTargetTaskId = taskId;

    // If right clicked row is not already selected, select it
    if (!state.selectedTaskIds.has(taskId)) {
      state.selectSingle(taskId);
    }

    showContextMenu(e.clientX, e.clientY);
  });

  window.addEventListener("click", (e) => {
    if (!e.target.closest("#contextMenu")) {
      hideContextMenu();
    }
  });

  const contextSubmenuQueues = document.getElementById("contextSubmenuQueues");

  function populateQueueSubmenu() {
    if (!contextSubmenuQueues) return;
    contextSubmenuQueues.innerHTML = "";
    const queues = state.queues || [];
    for (const q of queues) {
      const li = document.createElement("li");
      li.className = "menu-item";
      li.innerHTML = `<span>${q.name}</span>`;
      li.addEventListener("click", async (e) => {
        e.stopPropagation();
        hideContextMenu();
        for (const id of state.selectedTaskIds) {
          await api.moveTaskToQueue(id, q.id);
        }
        const updated = await api.listTasks();
        state.setTasks(updated);
      });
      contextSubmenuQueues.appendChild(li);
    }
  }

  function showContextMenu(x, y) {
    if (!contextMenu) return;

    populateQueueSubmenu();

    // Keep menu within viewport bounds
    const menuW = 210;
    const menuH = 240;
    const finalX = (x + menuW > window.innerWidth) ? window.innerWidth - menuW - 8 : x;
    const finalY = (y + menuH > window.innerHeight) ? window.innerHeight - menuH - 8 : y;

    contextMenu.style.left = `${finalX}px`;
    contextMenu.style.top = `${finalY}px`;
    contextMenu.classList.add("open");
  }

  function hideContextMenu() {
    if (contextMenu) {
      contextMenu.classList.remove("open");
    }
  }

  // Action listeners
  const menuResume = document.getElementById("menuItemResume");
  const menuPause = document.getElementById("menuItemPause");
  const menuOpenFile = document.getElementById("menuItemOpenFile");
  const menuOpenFolder = document.getElementById("menuItemOpenFolder");
  const menuCopyLink = document.getElementById("menuItemCopyLink");
  const menuRemove = document.getElementById("menuItemRemove");
  const menuDeleteFile = document.getElementById("menuItemDeleteFile");

  if (menuResume) {
    menuResume.addEventListener("click", async () => {
      hideContextMenu();
      for (const id of state.selectedTaskIds) {
        await api.resumeTask(id);
      }
      const updated = await api.listTasks();
      state.setTasks(updated);
    });
  }

  if (menuPause) {
    menuPause.addEventListener("click", async () => {
      hideContextMenu();
      for (const id of state.selectedTaskIds) {
        await api.pauseTask(id);
      }
      const updated = await api.listTasks();
      state.setTasks(updated);
    });
  }

  if (menuOpenFile) {
    menuOpenFile.addEventListener("click", async () => {
      hideContextMenu();
      const task = state.tasks.find(t => t.id === currentTargetTaskId);
      if (task && task.destination) {
        await api.openPath(task.destination);
      }
    });
  }

  if (menuOpenFolder) {
    menuOpenFolder.addEventListener("click", async () => {
      hideContextMenu();
      const task = state.tasks.find(t => t.id === currentTargetTaskId);
      if (task && task.destination) {
        await api.showInFolder(task.destination);
      }
    });
  }

  if (menuCopyLink) {
    menuCopyLink.addEventListener("click", () => {
      hideContextMenu();
      const task = state.tasks.find(t => t.id === currentTargetTaskId);
      if (task && task.url) {
        navigator.clipboard.writeText(task.url);
      }
    });
  }

  if (menuRemove) {
    menuRemove.addEventListener("click", async () => {
      hideContextMenu();
      if (!state.selectedTaskIds.size) return;
      if (confirm(`Remove ${state.selectedTaskIds.size} task(s) from the list?`)) {
        for (const id of state.selectedTaskIds) {
          await api.deleteTask(id, false);
        }
        state.clearSelection();
        const updated = await api.listTasks();
        state.setTasks(updated);
      }
    });
  }

  if (menuDeleteFile) {
    menuDeleteFile.addEventListener("click", async () => {
      hideContextMenu();
      if (!state.selectedTaskIds.size) return;
      if (confirm(`Permanently remove ${state.selectedTaskIds.size} task(s) AND delete downloaded files from disk?`)) {
        for (const id of state.selectedTaskIds) {
          await api.deleteTask(id, true);
        }
        state.clearSelection();
        const updated = await api.listTasks();
        state.setTasks(updated);
      }
    });
  }

  // -----------------------------------------------------------------
  // 2. Clipboard Monitoring
  // -----------------------------------------------------------------
  let toastTimeout = null;

  async function checkClipboard() {
    if (!state.clipboardMonitoring || !document.hasFocus()) return;
    try {
      if (!navigator.clipboard || !navigator.clipboard.readText) return;
      const text = await navigator.clipboard.readText();
      if (text && text !== lastSeenClipboardText && isValidHttpUrl(text.trim())) {
        lastSeenClipboardText = text;
        showClipboardToast(text.trim());
      }
    } catch (_) {
      // Browser permission might not be granted yet, silently ignore
    }
  }

  function showClipboardToast(url) {
    if (!clipboardToast || !toastUrlText) return;
    toastUrlText.textContent = url;
    clipboardToast.classList.add("show");

    if (toastTimeout) clearTimeout(toastTimeout);
    toastTimeout = setTimeout(hideClipboardToast, 9000);
  }

  function hideClipboardToast() {
    if (clipboardToast) clipboardToast.classList.remove("show");
  }

  if (btnToastDismiss) {
    btnToastDismiss.addEventListener("click", hideClipboardToast);
  }

  if (btnToastDownload) {
    btnToastDownload.addEventListener("click", () => {
      const url = toastUrlText.textContent;
      hideClipboardToast();
      openAddModalWithUrlFn(url);
    });
  }

  window.addEventListener("focus", checkClipboard);
  setInterval(checkClipboard, 3500);

  // -----------------------------------------------------------------
  // 3. Drag-and-Drop URL Overlay
  // -----------------------------------------------------------------
  let dragCounter = 0;

  window.addEventListener("dragenter", (e) => {
    e.preventDefault();
    dragCounter++;
    if (dragOverlay) dragOverlay.classList.add("active");
  });

  window.addEventListener("dragleave", (e) => {
    e.preventDefault();
    dragCounter--;
    if (dragCounter <= 0) {
      dragCounter = 0;
      if (dragOverlay) dragOverlay.classList.remove("active");
    }
  });

  window.addEventListener("dragover", (e) => {
    e.preventDefault();
  });

  window.addEventListener("drop", (e) => {
    e.preventDefault();
    dragCounter = 0;
    if (dragOverlay) dragOverlay.classList.remove("active");

    const text = e.dataTransfer.getData("text/plain") || e.dataTransfer.getData("text/uri-list");
    if (text && isValidHttpUrl(text.trim())) {
      openAddModalWithUrlFn(text.trim());
    }
  });
}

function isValidHttpUrl(string) {
  try {
    const url = new URL(string);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch (_) {
    return false;
  }
}
