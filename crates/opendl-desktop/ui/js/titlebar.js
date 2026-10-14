/**
 * OpenDL - Titlebar & Window Controls
 */

import { api } from "./api.js";
import { state } from "./state.js";

export function initTitlebar() {
  const btnMin = document.getElementById("btnWinMin");
  const btnMax = document.getElementById("btnWinMax");
  const btnClose = document.getElementById("btnWinClose");
  const btnTheme = document.getElementById("btnThemeToggle");
  const searchInput = document.getElementById("globalSearchInput");
  const titlebarDrag = document.querySelector(".titlebar-drag");

  if (btnMin) {
    btnMin.addEventListener("click", () => api.minimizeWindow());
  }

  if (btnMax) {
    btnMax.addEventListener("click", async () => {
      const isMax = await api.toggleMaximizeWindow();
      updateMaximizeIcon(isMax);
    });
  }

  if (btnClose) {
    btnClose.addEventListener("click", () => api.closeWindow());
  }

  if (btnTheme) {
    btnTheme.addEventListener("click", () => {
      const current = state.theme;
      const next = current === "dark" ? "light" : (current === "light" ? "system" : "dark");
      state.setTheme(next);
    });
  }

  if (searchInput) {
    searchInput.addEventListener("input", (e) => {
      state.setSearch(e.target.value);
    });

    searchInput.addEventListener("keydown", (e) => {
      if (e.key === "Escape") {
        searchInput.value = "";
        state.setSearch("");
        searchInput.blur();
      }
    });
  }

  // Double click drag area to toggle maximize
  if (titlebarDrag) {
    titlebarDrag.addEventListener("dblclick", async () => {
      const isMax = await api.toggleMaximizeWindow();
      updateMaximizeIcon(isMax);
    });
  }

  // Update theme toggle icon based on state
  state.subscribe((event, payload) => {
    if (event === "theme_changed") {
      updateThemeIcon(payload);
    }
  });

  // Apply initial theme
  document.documentElement.setAttribute("data-theme", state.theme);
  updateThemeIcon(state.theme);
}

function updateMaximizeIcon(isMax) {
  const iconSpan = document.querySelector("#btnWinMax .icon");
  if (!iconSpan) return;
  if (isMax) {
    // Restore icon (two overlapping squares)
    iconSpan.innerHTML = `
      <svg viewBox="0 0 10 10">
        <path d="M2.5 1.5h6v6h-6z" fill="none" stroke="currentColor" stroke-width="1"/>
        <path d="M1.5 3.5v5h5" fill="none" stroke="currentColor" stroke-width="1"/>
      </svg>
    `;
  } else {
    // Maximize icon (single square)
    iconSpan.innerHTML = `
      <svg viewBox="0 0 10 10">
        <rect x="1.5" y="1.5" width="7" height="7" fill="none" stroke="currentColor" stroke-width="1"/>
      </svg>
    `;
  }
}

function updateThemeIcon(theme) {
  const btn = document.getElementById("btnThemeToggle");
  const iconSpan = document.querySelector("#btnThemeToggle .icon");
  if (!iconSpan) return;

  if (theme === "dark") {
    if (btn) btn.title = "Theme: Dark (Click for Light)";
    iconSpan.innerHTML = `
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"></path>
      </svg>
    `;
  } else if (theme === "light") {
    if (btn) btn.title = "Theme: Light (Click for System)";
    iconSpan.innerHTML = `
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="12" cy="12" r="5"></circle>
        <line x1="12" y1="1" x2="12" y2="3"></line>
        <line x1="12" y1="21" x2="12" y2="23"></line>
        <line x1="4.22" y1="4.22" x2="5.64" y2="5.64"></line>
        <line x1="18.36" y1="18.36" x2="19.78" y2="19.78"></line>
        <line x1="1" y1="12" x2="3" y2="12"></line>
        <line x1="21" y1="12" x2="23" y2="12"></line>
        <line x1="4.22" y1="19.78" x2="5.64" y2="18.36"></line>
        <line x1="18.36" y1="5.64" x2="19.78" y2="4.22"></line>
      </svg>
    `;
  } else {
    // System
    if (btn) btn.title = "Theme: System (Follows OS, click for Dark)";
    iconSpan.innerHTML = `
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <rect x="2" y="3" width="20" height="14" rx="2" ry="2"></rect>
        <line x1="8" y1="21" x2="16" y2="21"></line>
        <line x1="12" y1="17" x2="12" y2="21"></line>
      </svg>
    `;
  }
}
