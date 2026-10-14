/**
 * OpenDL - Theme Engine & Custom Theme Studio
 * Provides 7 curated built-in palettes and custom theme creation, import & export.
 */

export const BUILTIN_THEMES = [
  {
    id: "graphite-dark",
    name: "Graphite Dark",
    mode: "dark",
    isBuiltin: true,
    tokens: {
      "--page-bg": "#0C0C0D",
      "--sidebar-bg": "#111113",
      "--titlebar-bg": "#111113",
      "--surface": "#151517",
      "--surface-card": "#151517",
      "--surface-modal": "#18181B",
      "--surface-popover": "#18181B",
      "--surface-hover": "rgba(237, 237, 239, 0.04)",
      "--surface-active": "rgba(237, 237, 239, 0.08)",
      "--text": "#EDEDEF",
      "--text-primary": "#EDEDEF",
      "--text-secondary": "#8C8C94",
      "--border": "#26262A",
      "--border-subtle": "#1E1E22",
      "--track": "#2A2A2E",
      "--accent-ink": "#EDEDEF",
      "--accent-primary": "#EDEDEF",
      "--on-accent": "#0C0C0D",
      "--signal": "#FF4F1F",
      "--state-finished": "#4CC38A",
      "--state-finished-subtle": "rgba(76, 195, 138, 0.12)",
      "--state-paused": "#E0A33A",
      "--state-paused-subtle": "rgba(224, 163, 58, 0.12)",
      "--state-failed": "#FF6B5E",
      "--state-failed-subtle": "rgba(255, 107, 94, 0.12)",
      "--grid-line": "rgba(237, 237, 239, 0.04)",
      "--crosshair-color": "rgba(237, 237, 239, 0.35)",
      "--table-header-bg": "#111113"
    }
  },
  {
    id: "graphite-light",
    name: "Graphite Light",
    mode: "light",
    isBuiltin: true,
    tokens: {
      "--page-bg": "#F4F4F5",
      "--sidebar-bg": "#EDEDEF",
      "--titlebar-bg": "#EDEDEF",
      "--surface": "#FFFFFF",
      "--surface-card": "#FFFFFF",
      "--surface-modal": "#FFFFFF",
      "--surface-popover": "#FFFFFF",
      "--surface-hover": "rgba(24, 24, 27, 0.04)",
      "--surface-active": "rgba(24, 24, 27, 0.08)",
      "--text": "#18181B",
      "--text-primary": "#18181B",
      "--text-secondary": "#6A6A72",
      "--border": "#E4E4E7",
      "--border-subtle": "#EBEBEF",
      "--track": "#E8E8EB",
      "--accent-ink": "#18181B",
      "--accent-primary": "#18181B",
      "--on-accent": "#FFFFFF",
      "--signal": "#FF4F1F",
      "--state-finished": "#1A784E",
      "--state-finished-subtle": "rgba(26, 120, 78, 0.10)",
      "--state-paused": "#926014",
      "--state-paused-subtle": "rgba(146, 96, 20, 0.10)",
      "--state-failed": "#D1352B",
      "--state-failed-subtle": "rgba(209, 53, 43, 0.10)",
      "--grid-line": "rgba(24, 24, 27, 0.04)",
      "--crosshair-color": "rgba(24, 24, 27, 0.28)",
      "--table-header-bg": "#EDEDEF"
    }
  },
  {
    id: "cyber-cyan",
    name: "Cyber Cyan",
    mode: "dark",
    isBuiltin: true,
    tokens: {
      "--page-bg": "#090D12",
      "--sidebar-bg": "#0E141D",
      "--titlebar-bg": "#0E141D",
      "--surface": "#131C28",
      "--surface-card": "#131C28",
      "--surface-modal": "#16202E",
      "--surface-popover": "#16202E",
      "--surface-hover": "rgba(0, 229, 255, 0.06)",
      "--surface-active": "rgba(0, 229, 255, 0.12)",
      "--text": "#E2F1F8",
      "--text-primary": "#E2F1F8",
      "--text-secondary": "#7A92A6",
      "--border": "#1F2D3D",
      "--border-subtle": "#172330",
      "--track": "#1A2533",
      "--accent-ink": "#00E5FF",
      "--accent-primary": "#00E5FF",
      "--on-accent": "#05131A",
      "--signal": "#FF3366",
      "--state-finished": "#00E676",
      "--state-finished-subtle": "rgba(0, 230, 118, 0.12)",
      "--state-paused": "#FFD600",
      "--state-paused-subtle": "rgba(255, 214, 0, 0.12)",
      "--state-failed": "#FF1744",
      "--state-failed-subtle": "rgba(255, 23, 68, 0.12)",
      "--grid-line": "rgba(0, 229, 255, 0.05)",
      "--crosshair-color": "rgba(0, 229, 255, 0.40)",
      "--table-header-bg": "#0E141D"
    }
  },
  {
    id: "midnight-oled",
    name: "Midnight OLED",
    mode: "dark",
    isBuiltin: true,
    tokens: {
      "--page-bg": "#000000",
      "--sidebar-bg": "#070707",
      "--titlebar-bg": "#070707",
      "--surface": "#0F0F0F",
      "--surface-card": "#0F0F0F",
      "--surface-modal": "#141414",
      "--surface-popover": "#141414",
      "--surface-hover": "rgba(255, 255, 255, 0.05)",
      "--surface-active": "rgba(255, 255, 255, 0.10)",
      "--text": "#FFFFFF",
      "--text-primary": "#FFFFFF",
      "--text-secondary": "#888888",
      "--border": "#222222",
      "--border-subtle": "#191919",
      "--track": "#1C1C1C",
      "--accent-ink": "#FFFFFF",
      "--accent-primary": "#FFFFFF",
      "--on-accent": "#000000",
      "--signal": "#FF5252",
      "--state-finished": "#00E676",
      "--state-finished-subtle": "rgba(0, 230, 118, 0.12)",
      "--state-paused": "#FFB300",
      "--state-paused-subtle": "rgba(255, 179, 0, 0.12)",
      "--state-failed": "#FF5252",
      "--state-failed-subtle": "rgba(255, 82, 82, 0.12)",
      "--grid-line": "rgba(255, 255, 255, 0.04)",
      "--crosshair-color": "rgba(255, 255, 255, 0.35)",
      "--table-header-bg": "#070707"
    }
  },
  {
    id: "industrial-amber",
    name: "Industrial Amber",
    mode: "dark",
    isBuiltin: true,
    tokens: {
      "--page-bg": "#0D0C0A",
      "--sidebar-bg": "#14120E",
      "--titlebar-bg": "#14120E",
      "--surface": "#1C1914",
      "--surface-card": "#1C1914",
      "--surface-modal": "#221E18",
      "--surface-popover": "#221E18",
      "--surface-hover": "rgba(255, 176, 0, 0.06)",
      "--surface-active": "rgba(255, 176, 0, 0.12)",
      "--text": "#F3EDE2",
      "--text-primary": "#F3EDE2",
      "--text-secondary": "#9E9585",
      "--border": "#2E281F",
      "--border-subtle": "#241F18",
      "--track": "#26211A",
      "--accent-ink": "#FFB000",
      "--accent-primary": "#FFB000",
      "--on-accent": "#1A1000",
      "--signal": "#FF3B30",
      "--state-finished": "#52C41A",
      "--state-finished-subtle": "rgba(82, 196, 26, 0.12)",
      "--state-paused": "#FFB000",
      "--state-paused-subtle": "rgba(255, 176, 0, 0.12)",
      "--state-failed": "#F5222D",
      "--state-failed-subtle": "rgba(245, 34, 45, 0.12)",
      "--grid-line": "rgba(255, 176, 0, 0.04)",
      "--crosshair-color": "rgba(255, 176, 0, 0.35)",
      "--table-header-bg": "#14120E"
    }
  },
  {
    id: "nordic-frost",
    name: "Nordic Frost",
    mode: "light",
    isBuiltin: true,
    tokens: {
      "--page-bg": "#F4F6F9",
      "--sidebar-bg": "#E9ECF2",
      "--titlebar-bg": "#E9ECF2",
      "--surface": "#FFFFFF",
      "--surface-card": "#FFFFFF",
      "--surface-modal": "#FFFFFF",
      "--surface-popover": "#FFFFFF",
      "--surface-hover": "rgba(30, 41, 59, 0.04)",
      "--surface-active": "rgba(30, 41, 59, 0.08)",
      "--text": "#111827",
      "--text-primary": "#111827",
      "--text-secondary": "#5A6578",
      "--border": "#D8DFEB",
      "--border-subtle": "#E2E8F0",
      "--track": "#E2E7F0",
      "--accent-ink": "#1E293B",
      "--accent-primary": "#1E293B",
      "--on-accent": "#FFFFFF",
      "--signal": "#0284C7",
      "--state-finished": "#059669",
      "--state-finished-subtle": "rgba(5, 150, 105, 0.10)",
      "--state-paused": "#D97706",
      "--state-paused-subtle": "rgba(217, 119, 6, 0.10)",
      "--state-failed": "#DC2626",
      "--state-failed-subtle": "rgba(220, 38, 38, 0.10)",
      "--grid-line": "rgba(30, 41, 59, 0.04)",
      "--crosshair-color": "rgba(30, 41, 59, 0.30)",
      "--table-header-bg": "#E9ECF2"
    }
  },
  {
    id: "synth-violet",
    name: "Synth Violet",
    mode: "dark",
    isBuiltin: true,
    tokens: {
      "--page-bg": "#0E0C17",
      "--sidebar-bg": "#141122",
      "--titlebar-bg": "#141122",
      "--surface": "#1B172E",
      "--surface-card": "#1B172E",
      "--surface-modal": "#211C38",
      "--surface-popover": "#211C38",
      "--surface-hover": "rgba(168, 85, 247, 0.06)",
      "--surface-active": "rgba(168, 85, 247, 0.12)",
      "--text": "#F1EDFD",
      "--text-primary": "#F1EDFD",
      "--text-secondary": "#8F88AE",
      "--border": "#2D264A",
      "--border-subtle": "#251F3D",
      "--track": "#251F3D",
      "--accent-ink": "#A855F7",
      "--accent-primary": "#A855F7",
      "--on-accent": "#0F021F",
      "--signal": "#EC4899",
      "--state-finished": "#10B981",
      "--state-finished-subtle": "rgba(16, 185, 129, 0.12)",
      "--state-paused": "#F59E0B",
      "--state-paused-subtle": "rgba(245, 158, 11, 0.12)",
      "--state-failed": "#F43F5E",
      "--state-failed-subtle": "rgba(244, 63, 94, 0.12)",
      "--grid-line": "rgba(168, 85, 247, 0.05)",
      "--crosshair-color": "rgba(168, 85, 247, 0.35)",
      "--table-header-bg": "#141122"
    }
  }
];

class ThemeManager {
  constructor() {
    this.customThemes = this.loadCustomThemes();
    this.activeThemeId = localStorage.getItem("opendl_theme_id") || "graphite-dark";
    this.styleTag = null;
  }

  loadCustomThemes() {
    try {
      const data = localStorage.getItem("opendl_custom_themes");
      return data ? JSON.parse(data) : [];
    } catch (_) {
      return [];
    }
  }

  saveCustomThemes() {
    try {
      localStorage.setItem("opendl_custom_themes", JSON.stringify(this.customThemes));
    } catch (_) {}
  }

  getAllThemes() {
    return [...BUILTIN_THEMES, ...this.customThemes];
  }

  getTheme(id) {
    return this.getAllThemes().find(t => t.id === id) || BUILTIN_THEMES[0];
  }

  init() {
    this.ensureStyleTag();
    this.applyTheme(this.activeThemeId);
  }

  ensureStyleTag() {
    if (!this.styleTag) {
      this.styleTag = document.getElementById("dynamic-theme-vars");
      if (!this.styleTag) {
        this.styleTag = document.createElement("style");
        this.styleTag.id = "dynamic-theme-vars";
        document.head.appendChild(this.styleTag);
      }
    }
  }

  applyTheme(themeId) {
    const theme = this.getTheme(themeId);
    this.activeThemeId = theme.id;
    localStorage.setItem("opendl_theme_id", theme.id);

    // Set base data-theme attribute for light/dark scheme behavior
    document.documentElement.setAttribute("data-theme", theme.mode);
    document.documentElement.setAttribute("data-theme-id", theme.id);

    // Apply tokens dynamically to :root
    this.ensureStyleTag();
    let cssRules = ":root {\n";
    for (const [k, v] of Object.entries(theme.tokens || {})) {
      cssRules += `  ${k}: ${v};\n`;
    }
    cssRules += "}\n";
    this.styleTag.textContent = cssRules;

    // Dispatch event so UI widgets (speed canvas, segment map) can redraw
    window.dispatchEvent(new CustomEvent("opendl_theme_changed", { detail: theme }));
  }

  createCustomTheme({ name, mode = "dark", pageBg, sidebarBg, surface, text, textSec, border, accent, signal }) {
    const id = "custom-" + Date.now();
    const isDark = mode === "dark";

    const newTheme = {
      id,
      name: name || "Custom Theme",
      mode,
      isBuiltin: false,
      tokens: {
        "--page-bg": pageBg || (isDark ? "#101012" : "#F6F6F8"),
        "--sidebar-bg": sidebarBg || (isDark ? "#151518" : "#EDEDF1"),
        "--titlebar-bg": sidebarBg || (isDark ? "#151518" : "#EDEDF1"),
        "--surface": surface || (isDark ? "#1A1A1E" : "#FFFFFF"),
        "--surface-card": surface || (isDark ? "#1A1A1E" : "#FFFFFF"),
        "--surface-modal": surface || (isDark ? "#1E1E24" : "#FFFFFF"),
        "--surface-popover": surface || (isDark ? "#1E1E24" : "#FFFFFF"),
        "--surface-hover": isDark ? "rgba(255, 255, 255, 0.05)" : "rgba(0, 0, 0, 0.04)",
        "--surface-active": isDark ? "rgba(255, 255, 255, 0.10)" : "rgba(0, 0, 0, 0.08)",
        "--text": text || (isDark ? "#F0F0F4" : "#1A1A1E"),
        "--text-primary": text || (isDark ? "#F0F0F4" : "#1A1A1E"),
        "--text-secondary": textSec || (isDark ? "#8E8E98" : "#6C6C76"),
        "--border": border || (isDark ? "#2A2A32" : "#E2E2E8"),
        "--border-subtle": border || (isDark ? "#222228" : "#ECECF0"),
        "--track": isDark ? "#282830" : "#E4E4EA",
        "--accent-ink": accent || (isDark ? "#F0F0F4" : "#1A1A1E"),
        "--accent-primary": accent || (isDark ? "#F0F0F4" : "#1A1A1E"),
        "--on-accent": isDark ? "#0A0A0C" : "#FFFFFF",
        "--signal": signal || "#FF4F1F",
        "--state-finished": isDark ? "#4CC38A" : "#1A784E",
        "--state-finished-subtle": isDark ? "rgba(76, 195, 138, 0.12)" : "rgba(26, 120, 78, 0.10)",
        "--state-paused": isDark ? "#E0A33A" : "#926014",
        "--state-paused-subtle": isDark ? "rgba(224, 163, 58, 0.12)" : "rgba(146, 96, 20, 0.10)",
        "--state-failed": isDark ? "#FF6B5E" : "#D1352B",
        "--state-failed-subtle": isDark ? "rgba(255, 107, 94, 0.12)" : "rgba(209, 53, 43, 0.10)",
        "--grid-line": isDark ? "rgba(255, 255, 255, 0.04)" : "rgba(0, 0, 0, 0.04)",
        "--crosshair-color": isDark ? "rgba(255, 255, 255, 0.35)" : "rgba(0, 0, 0, 0.30)",
        "--table-header-bg": sidebarBg || (isDark ? "#151518" : "#EDEDF1")
      }
    };

    this.customThemes.push(newTheme);
    this.saveCustomThemes();
    this.applyTheme(newTheme.id);
    return newTheme;
  }

  deleteCustomTheme(id) {
    this.customThemes = this.customThemes.filter(t => t.id !== id);
    this.saveCustomThemes();
    if (this.activeThemeId === id) {
      this.applyTheme("graphite-dark");
    }
  }

  exportTheme(id) {
    const theme = this.getTheme(id);
    return JSON.stringify({
      opendlThemeSchema: 1,
      name: theme.name,
      mode: theme.mode,
      tokens: theme.tokens
    }, null, 2);
  }

  importTheme(jsonString) {
    try {
      const data = JSON.parse(jsonString);
      if (!data.name || !data.tokens) {
        throw new Error("Invalid theme format: missing name or tokens.");
      }

      const id = "custom-" + Date.now();
      const theme = {
        id,
        name: data.name.trim(),
        mode: data.mode === "light" ? "light" : "dark",
        isBuiltin: false,
        tokens: data.tokens
      };

      this.customThemes.push(theme);
      this.saveCustomThemes();
      this.applyTheme(theme.id);
      return theme;
    } catch (err) {
      throw new Error("Could not parse theme: " + err.message);
    }
  }
}

export const themes = new ThemeManager();
