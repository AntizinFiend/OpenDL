import { store, setTheme, notify } from "./state.js";
import { openAddDialog, openSettingsDialog } from "./dialogs.js";

let activeIndex = 0;
let currentItems = [];

function buildPaletteItems(query, actionsCallback) {
  const q = query.trim().toLowerCase();
  const baseCommands = [
    { id: "cmd-add", label: "Add New Download", hint: "CTRL+N", run: () => openAddDialog() },
    { id: "cmd-resume-all", label: "Resume All Transfers", hint: "ACTION", run: () => actionsCallback("resume-all") },
    { id: "cmd-pause-all", label: "Pause All Transfers", hint: "ACTION", run: () => actionsCallback("pause-all") },
    { id: "cmd-view-all", label: "Switch View: All Transfers", hint: "VIEW", run: () => actionsCallback("view", "all") },
    { id: "cmd-view-down", label: "Switch View: Downloading", hint: "VIEW", run: () => actionsCallback("view", "downloading") },
    { id: "cmd-view-comp", label: "Switch View: Completed", hint: "VIEW", run: () => actionsCallback("view", "completed") },
    { id: "cmd-view-queued", label: "Switch View: Queued", hint: "VIEW", run: () => actionsCallback("view", "queued") },
    { id: "cmd-theme-dark", label: "Switch Theme: Dark", hint: "THEME", run: () => setTheme("dark") },
    { id: "cmd-theme-light", label: "Switch Theme: Light", hint: "THEME", run: () => setTheme("light") },
    { id: "cmd-theme-amber", label: "Switch Theme: Amber Day", hint: "THEME", run: () => setTheme("amber-day") },
    { id: "cmd-theme-blueprint", label: "Switch Theme: Blueprint", hint: "THEME", run: () => setTheme("blueprint") },
    { id: "cmd-theme-system", label: "Switch Theme: System", hint: "THEME", run: () => setTheme("system") },
    { id: "cmd-settings", label: "Open System Configuration", hint: "SETTINGS", run: () => openSettingsDialog() }
  ];

  const taskCommands = store.tasks.map((t) => ({
    id: `task-${t.id}`,
    label: `Task: ${t.name}`,
    hint: t.status.toUpperCase(),
    run: () => {
      store.view = "all";
      store.selectedIds.clear();
      store.selectedIds.add(t.id);
      store.lastSelectedId = t.id;
      notify("selection");
    }
  }));

  const combined = [...baseCommands, ...taskCommands];
  if (!q) return combined;
  return combined.filter((item) => item.label.toLowerCase().includes(q) || item.hint.toLowerCase().includes(q));
}

function renderPaletteList() {
  const listEl = document.getElementById("palette-list");
  if (!listEl) return;
  if (currentItems.length === 0) {
    listEl.innerHTML = `<li class="palette-item"><span class="micro-label">NO MATCHING COMMANDS</span></li>`;
    return;
  }
  activeIndex = Math.max(0, Math.min(activeIndex, currentItems.length - 1));
  listEl.innerHTML = currentItems
    .map((item, idx) => `
      <li
        class="palette-item ${idx === activeIndex ? "is-active" : ""}"
        role="option"
        aria-selected="${idx === activeIndex}"
        data-palette-idx="${idx}"
      >
        <span>${item.label}</span>
        <kbd class="keycap">${item.hint}</kbd>
      </li>
    `)
    .join("");
}

export function openCommandPalette(actionsCallback) {
  const backdrop = document.getElementById("dialog-palette-backdrop");
  const input = document.getElementById("palette-input");
  if (!backdrop || !input) return;

  input.value = "";
  activeIndex = 0;
  currentItems = buildPaletteItems("", actionsCallback);
  renderPaletteList();
  backdrop.classList.remove("is-hidden");
  input.focus();
}

export function initCommandPalette(actionsCallback) {
  const backdrop = document.getElementById("dialog-palette-backdrop");
  const input = document.getElementById("palette-input");
  const listEl = document.getElementById("palette-list");

  input?.addEventListener("input", () => {
    activeIndex = 0;
    currentItems = buildPaletteItems(input.value, actionsCallback);
    renderPaletteList();
  });

  input?.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      activeIndex = (activeIndex + 1) % Math.max(1, currentItems.length);
      renderPaletteList();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      activeIndex = (activeIndex - 1 + Math.max(1, currentItems.length)) % Math.max(1, currentItems.length);
      renderPaletteList();
    } else if (e.key === "Enter") {
      e.preventDefault();
      const chosen = currentItems[activeIndex];
      backdrop?.classList.add("is-hidden");
      chosen?.run();
    } else if (e.key === "Escape") {
      backdrop?.classList.add("is-hidden");
    }
  });

  listEl?.addEventListener("click", (e) => {
    const li = e.target.closest("[data-palette-idx]");
    if (!li) return;
    const idx = Number(li.getAttribute("data-palette-idx"));
    const chosen = currentItems[idx];
    backdrop?.classList.add("is-hidden");
    chosen?.run();
  });
}