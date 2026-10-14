const STORAGE_KEYS = {
  theme: "opendl.theme",
  sidebarCollapsed: "opendl.sidebarCollapsed",
  dockCollapsed: "opendl.dockCollapsed",
  dockHeight: "opendl.dockHeight",
  dockTab: "opendl.dockTab",
  clipboardEnabled: "opendl.clipboardEnabled",
  queues: "opendl.queues",
  piezoEnabled: "opendl.piezoEnabled",
  segViewMode: "opendl.segViewMode"
};

function readStorage(key, fallback) {
  try {
    const raw = localStorage.getItem(key);
    return raw !== null ? JSON.parse(raw) : fallback;
  } catch {
    return fallback;
  }
}

function writeStorage(key, value) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Ignore storage quota errors
  }
}

export const RING_CAPACITY = 1800; // 15 minutes at 500ms intervals

export const store = {
  tasks: [],
  taskMap: new Map(),
  taskSparklines: new Map(), // id -> number[] (last 32 samples)
  throughputRing: new Float64Array(RING_CAPACITY),
  throughputHead: 0,
  throughputCount: 0,
  throughputWindowSec: 60,

  view: "all", // 'all' | 'downloading' | 'completed' | 'queued'
  activeQueueFilter: null,
  activeCategoryFilter: null,
  searchQuery: "",

  selectedIds: new Set(),
  lastSelectedId: null,
  focusedRowId: null,

  segViewMode: readStorage(STORAGE_KEYS.segViewMode, "lanes"), // 'lanes' | 'matrix'
  soloSegmentIdx: null,
  piezoEnabled: readStorage(STORAGE_KEYS.piezoEnabled, true),
  wireLogs: new Map(), // taskId -> Array<{ ts, verb, detail, status, level }>
  prevTaskStates: new Map(), // taskId -> status string for transition detection

  queues: readStorage(STORAGE_KEYS.queues, ["Main Queue", "Nightly Batch"]),
  theme: readStorage(STORAGE_KEYS.theme, "dark"),
  sidebarCollapsed: readStorage(STORAGE_KEYS.sidebarCollapsed, false),
  dockCollapsed: readStorage(STORAGE_KEYS.dockCollapsed, false),
  dockHeight: readStorage(STORAGE_KEYS.dockHeight, 228),
  dockTab: readStorage(STORAGE_KEYS.dockTab, "segments"),
  clipboardEnabled: readStorage(STORAGE_KEYS.clipboardEnabled, true),
  dismissedClipboardUrl: ""
};

const listeners = new Set();

export function subscribe(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function notify(reason = "update") {
  for (const fn of listeners) fn(store, reason);
}

export function setTheme(themeId) {
  store.theme = themeId;
  writeStorage(STORAGE_KEYS.theme, themeId);
  document.documentElement.setAttribute("data-theme", themeId);
  notify("theme");
}

export function setSidebarCollapsed(collapsed) {
  store.sidebarCollapsed = Boolean(collapsed);
  writeStorage(STORAGE_KEYS.sidebarCollapsed, store.sidebarCollapsed);
  notify("layout");
}

export function setDockCollapsed(collapsed) {
  store.dockCollapsed = Boolean(collapsed);
  writeStorage(STORAGE_KEYS.dockCollapsed, store.dockCollapsed);
  notify("dock");
}

export function setDockHeight(px) {
  const maxH = Math.floor(window.innerHeight * 0.6);
  const clamped = Math.max(160, Math.min(maxH, Math.round(px)));
  store.dockHeight = clamped;
  writeStorage(STORAGE_KEYS.dockHeight, clamped);
  notify("dock");
}

export function setDockTab(tab) {
  store.dockTab = tab;
  writeStorage(STORAGE_KEYS.dockTab, tab);
  notify("dock");
}

export function setClipboardEnabled(enabled) {
  store.clipboardEnabled = Boolean(enabled);
  writeStorage(STORAGE_KEYS.clipboardEnabled, store.clipboardEnabled);
  notify("settings");
}

export function addQueueName(name) {
  const trimmed = String(name || "").trim();
  if (!trimmed || store.queues.includes(trimmed)) return false;
  store.queues.push(trimmed);
  writeStorage(STORAGE_KEYS.queues, store.queues);
  notify("queues");
  return true;
}

export function pushThroughputSample(bytesPerSec) {
  store.throughputRing[store.throughputHead] = bytesPerSec;
  store.throughputHead = (store.throughputHead + 1) % RING_CAPACITY;
  if (store.throughputCount < RING_CAPACITY) store.throughputCount++;
}

export function getThroughputSeries(seconds = 60) {
  const maxSamples = Math.min(store.throughputCount, Math.floor(seconds * 2));
  const out = new Array(maxSamples);
  for (let i = 0; i < maxSamples; i++) {
    const idx = (store.throughputHead - maxSamples + i + RING_CAPACITY) % RING_CAPACITY;
    out[i] = store.throughputRing[idx];
  }
  return out;
}

export function pushTaskSpeedSample(id, speed) {
  let arr = store.taskSparklines.get(id);
  if (!arr) {
    arr = [];
    store.taskSparklines.set(id, arr);
  }
  arr.push(speed || 0);
  if (arr.length > 32) arr.shift();
}

export function getFilteredTasks() {
  const q = store.searchQuery.trim().toLowerCase();
  return store.tasks.filter((task) => {
    if (store.view === "downloading" && task.status !== "downloading") return false;
    if (store.view === "completed" && task.status !== "finished") return false;
    if (store.view === "queued" && task.status !== "queued") return false;
    if (store.activeQueueFilter && task.queue !== store.activeQueueFilter) return false;
    if (store.activeCategoryFilter && task.category !== store.activeCategoryFilter) return false;
    if (q) {
      const hay = `${task.name || ""} ${task.url || ""} ${task.category || ""} ${task.queue || ""}`.toLowerCase();
      if (!hay.includes(q)) return false;
    }
    return true;
  });
}

export function formatBytes(bytes) {
  if (bytes == null || Number.isNaN(bytes) || bytes < 0) return "--";
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const idx = Math.min(units.length - 1, Math.floor(Math.log(bytes) / Math.log(1024)));
  const val = bytes / Math.pow(1024, idx);
  return `${val >= 100 || idx === 0 ? val.toFixed(0) : val.toFixed(2)} ${units[idx]}`;
}

export function formatSpeedParts(bytesPerSec) {
  if (!bytesPerSec || bytesPerSec <= 0) return { value: "0.0", unit: "KB/S" };
  const units = ["B/S", "KB/S", "MB/S", "GB/S"];
  const idx = Math.min(units.length - 1, Math.floor(Math.log(bytesPerSec) / Math.log(1024)));
  const val = bytesPerSec / Math.pow(1024, idx);
  return {
    value: val >= 100 ? val.toFixed(0) : val.toFixed(1),
    unit: units[idx]
  };
}

export function formatEta(seconds) {
  if (seconds == null || !Number.isFinite(seconds) || seconds < 0) return "--";
  if (seconds === 0) return "0s";
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${String(s).padStart(2, "0")}s`;
  return `${s}s`;
}

export function decodeTitleEffect(el, finalString) {
  if (!el) return;
  const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches || document.hidden;
  if (reduced) {
    el.textContent = finalString;
    return;
  }
  const glyphs = "0123456789ABCDEF//:_";
  const start = performance.now();
  const duration = 180;
  function step(now) {
    const t = Math.min(1, (now - start) / duration);
    const lockCount = Math.floor(t * finalString.length);
    let out = finalString.slice(0, lockCount);
    for (let i = lockCount; i < finalString.length; i++) {
      out += finalString[i] === " " ? " " : glyphs[(i + Math.floor(now)) % glyphs.length];
    }
    el.textContent = out;
    if (t < 1) requestAnimationFrame(step);
  }
  requestAnimationFrame(step);
}

export function setSegViewMode(mode) {
  store.segViewMode = mode === "matrix" ? "matrix" : "lanes";
  writeStorage(STORAGE_KEYS.segViewMode, store.segViewMode);
  playPiezo("snap");
  notify("dock");
}

export function setSoloSegment(idx) {
  store.soloSegmentIdx = store.soloSegmentIdx === idx ? null : idx;
  playPiezo("tick");
  notify("dock");
}

export function setPiezoEnabled(enabled) {
  store.piezoEnabled = Boolean(enabled);
  writeStorage(STORAGE_KEYS.piezoEnabled, store.piezoEnabled);
  if (store.piezoEnabled) playPiezo("snap");
  notify("settings");
}

let audioCtx = null;

export function playPiezo(kind = "tick") {
  if (!store.piezoEnabled || document.hidden) return;
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  try {
    if (!audioCtx) {
      const Ctx = window.AudioContext || window.webkitAudioContext;
      if (!Ctx) return;
      audioCtx = new Ctx();
    }
    if (audioCtx.state === "suspended") audioCtx.resume();

    const now = audioCtx.currentTime;
    const osc = audioCtx.createOscillator();
    const gain = audioCtx.createGain();

    if (kind === "complete") {
      osc.type = "triangle";
      osc.frequency.setValueAtTime(1480, now);
      osc.frequency.setValueAtTime(1975, now + 0.018);
      gain.gain.setValueAtTime(0.035, now);
      gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.045);
      osc.connect(gain);
      gain.connect(audioCtx.destination);
      osc.start(now);
      osc.stop(now + 0.046);
    } else if (kind === "snap") {
      osc.type = "square";
      osc.frequency.setValueAtTime(1120, now);
      gain.gain.setValueAtTime(0.022, now);
      gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.006);
      osc.connect(gain);
      gain.connect(audioCtx.destination);
      osc.start(now);
      osc.stop(now + 0.007);
    } else {
      osc.type = "square";
      osc.frequency.setValueAtTime(840, now);
      gain.gain.setValueAtTime(0.016, now);
      gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.0035);
      osc.connect(gain);
      gain.connect(audioCtx.destination);
      osc.start(now);
      osc.stop(now + 0.004);
    }
  } catch {
    // Ignore audio context restrictions
  }
}

export function appendWireLog(taskId, verb, detail, status, level = "info") {
  if (!taskId) return;
  let list = store.wireLogs.get(taskId);
  if (!list) {
    list = [];
    store.wireLogs.set(taskId, list);
  }
  const now = new Date();
  const ts = `${String(now.getMinutes()).padStart(2, "0")}:${String(now.getSeconds()).padStart(2, "0")}.${String(now.getMilliseconds()).padStart(3, "0")}`;
  list.push({ ts, verb, detail, status, level });
  if (list.length > 80) list.shift();
}

export function seedOrUpdateWireLogs(task) {
  if (!task || !task.id) return;
  if (!store.wireLogs.has(task.id)) {
    let path = task.name || "/";
    try { path = new URL(task.url).pathname; } catch {}
    appendWireLog(task.id, "SYN", `CONNECT ${task.url || task.name}`, "TLSv1.3 OK", "ok");
    appendWireLog(task.id, "GET", `${path}`, task.status === "failed" ? "HTTP/2 503" : "HTTP/2 206 PARTIAL", task.status === "failed" ? "err" : "ok");
    if (task.segments && task.segments.length > 0) {
      task.segments.forEach((s, idx) => {
        appendWireLog(
          task.id,
          `SEG#${String(idx + 1).padStart(2, "0")}`,
          `RANGE bytes=${s.start}-${s.end}`,
          s.active ? "STREAMING" : (s.downloaded >= s.end - s.start ? "EOF 100%" : "STANDBY"),
          s.active ? "ok" : "info"
        );
      });
    }
  }

  const prev = store.prevTaskStates.get(task.id);
  if (prev && prev !== task.status) {
    if (task.status === "finished") {
      appendWireLog(task.id, "FIN", `CHECKSUM & EOF VERIFIED (${formatBytes(task.size)})`, "200 COMPLETE", "ok");
      playPiezo("complete");
    } else if (task.status === "paused") {
      appendWireLog(task.id, "RST", "OPERATOR SUSPENDED SOCKET READS", "PAUSED", "info");
    } else if (task.status === "downloading") {
      appendWireLog(task.id, "ACK", "RESUMING MULTIPLEX BYTE RANGES", "206 ACTIVE", "ok");
    } else if (task.status === "failed") {
      appendWireLog(task.id, "ERR", task.error || "Upstream socket reset", "503 FAILED", "err");
    }
  }
  store.prevTaskStates.set(task.id, task.status);
}