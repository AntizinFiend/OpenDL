const hasTauri = () => Boolean(window.__TAURI__ && window.__TAURI__.core && typeof window.__TAURI__.core.invoke === "function");

// Clearly-labeled built-in sample dataset for plain browser preview when window.__TAURI__ is absent
const SAMPLE_TASKS = [
  {
    id: "task-001",
    name: "windows-11-iot-enterprise-ltsc-2024-x64.iso",
    url: "https://mirror.etti.local/iso/windows-11-iot-enterprise-ltsc-2024-x64.iso",
    status: "downloading",
    size: 4823449600,
    downloaded: 2610485760,
    speed: 18454937,
    eta: 120,
    category: "Compressed",
    queue: "Main Queue",
    segments: [
      { start: 0, end: 602931200, downloaded: 602931200, active: false, speed: 0 },
      { start: 602931200, end: 1205862400, downloaded: 602931200, active: false, speed: 0 },
      { start: 1205862400, end: 1808793600, downloaded: 512000000, active: true, speed: 5242880 },
      { start: 1808793600, end: 2411724800, downloaded: 410000000, active: true, speed: 4718592 },
      { start: 2411724800, end: 3014656000, downloaded: 280000000, active: true, speed: 4194304 },
      { start: 3014656000, end: 3617587200, downloaded: 202623360, active: true, speed: 4299161 },
      { start: 3617587200, end: 4220518400, downloaded: 0, active: false, speed: 0 },
      { start: 4220518400, end: 4823449600, downloaded: 0, active: false, speed: 0 }
    ]
  },
  {
    id: "task-002",
    name: "cuda_12.6.2_560.94_windows_network.exe",
    url: "https://developer.download.nvidia.com/compute/cuda/12.6.2/cuda_12.6.2_windows.exe",
    status: "downloading",
    size: 1979711488,
    downloaded: 845152256,
    speed: 9646899,
    eta: 117,
    category: "Programs",
    queue: "Main Queue",
    segments: [
      { start: 0, end: 494927872, downloaded: 494927872, active: false, speed: 0 },
      { start: 494927872, end: 989855744, downloaded: 220112192, active: true, speed: 5138022 },
      { start: 989855744, end: 1484783616, downloaded: 130112192, active: true, speed: 4508877 },
      { start: 1484783616, end: 1979711488, downloaded: 0, active: false, speed: 0 }
    ]
  },
  {
    id: "task-003",
    name: "blender-4.3.0-windows-x64.zip",
    url: "https://download.blender.org/release/Blender4.3/blender-4.3.0-windows-x64.zip",
    status: "paused",
    size: 348127232,
    downloaded: 215838884,
    speed: 0,
    eta: null,
    category: "Programs",
    queue: "Main Queue",
    segments: [
      { start: 0, end: 174063616, downloaded: 174063616, active: false, speed: 0 },
      { start: 174063616, end: 348127232, downloaded: 41775268, active: false, speed: 0 }
    ]
  },
  {
    id: "task-004",
    name: "siggraph-2025-realtime-radiance-cascades-4k.mkv",
    url: "https://media.archive.org/siggraph2025/realtime-radiance-cascades-4k.mkv",
    status: "queued",
    size: 2415919104,
    downloaded: 0,
    speed: 0,
    eta: null,
    category: "Video",
    queue: "Nightly Batch",
    segments: []
  },
  {
    id: "task-005",
    name: "vulkan-sdk-1.3.296.0-installer.exe",
    url: "https://sdk.lunarg.com/sdk/download/1.3.296.0/windows/vulkan-sdk.exe",
    status: "failed",
    size: 238026752,
    downloaded: 64225280,
    speed: 0,
    eta: null,
    category: "Programs",
    queue: "Main Queue",
    error: "HTTP 503 Service Unavailable: upstream mirror reset peer connection",
    segments: []
  },
  {
    id: "task-006",
    name: "ieee-802.3df-800gbe-specification-draft.pdf",
    url: "https://standards.ieee.org/drafts/ieee-802.3df-800gbe.pdf",
    status: "finished",
    size: 48234496,
    downloaded: 48234496,
    speed: 0,
    eta: 0,
    category: "Documents",
    queue: "Main Queue",
    completedAt: Date.now() - 1000 * 60 * 42,
    avgSpeed: 6422528,
    segments: []
  },
  {
    id: "task-007",
    name: "anechoic-test-sweep-96khz-24bit.flac",
    url: "https://audio.telemetry.local/reference/anechoic-test-sweep-96khz-24bit.flac",
    status: "finished",
    size: 1104857600,
    downloaded: 1104857600,
    speed: 0,
    eta: 0,
    category: "Music",
    queue: "Main Queue",
    completedAt: Date.now() - 1000 * 60 * 60 * 27,
    avgSpeed: 14155776,
    segments: []
  }
];

function normalizeStatus(rawStatus) {
  const s = String(rawStatus || "").toLowerCase();
  if (s === "downloading" || s === "active" || s === "running" || s === "in_progress") return "downloading";
  if (s === "queued" || s === "pending" || s === "waiting") return "queued";
  if (s === "paused" || s === "stopped" || s === "suspended") return "paused";
  if (s === "failed" || s === "error") return "failed";
  if (s === "finished" || s === "completed" || s === "complete" || s === "done") return "finished";
  return "queued";
}

function inferCategory(name = "") {
  const ext = (name.split(".").pop() || "").toLowerCase();
  if (["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso", "tgz"].includes(ext)) return "Compressed";
  if (["exe", "msi", "msix", "bat", "bin", "dll", "deb", "rpm"].includes(ext)) return "Programs";
  if (["mp4", "mkv", "webm", "avi", "mov", "ts"].includes(ext)) return "Video";
  if (["mp3", "flac", "wav", "ogg", "aac", "m4a"].includes(ext)) return "Music";
  if (["pdf", "txt", "md", "doc", "docx", "xls", "xlsx", "csv", "json"].includes(ext)) return "Documents";
  return "Documents";
}

function inferNameFromUrl(url = "") {
  try {
    const u = new URL(url);
    const base = u.pathname.split("/").filter(Boolean).pop();
    return base ? decodeURIComponent(base) : url;
  } catch {
    const parts = String(url).split("/").filter(Boolean);
    return parts.pop() || "unnamed-resource.bin";
  }
}

export function normalizeTask(raw = {}, progressOverride = null) {
  const merged = progressOverride ? { ...raw, ...progressOverride } : raw;
  const status = normalizeStatus(merged.status ?? merged.state);
  const url = merged.url ?? merged.uri ?? "";
  const name = merged.name ?? merged.filename ?? merged.file_name ?? inferNameFromUrl(url);
  const size = typeof merged.size === "number" ? merged.size : (typeof merged.total_size === "number" ? merged.total_size : (typeof merged.total_bytes === "number" ? merged.total_bytes : null));
  let downloaded = typeof merged.downloaded === "number"
    ? merged.downloaded
    : (typeof merged.downloaded_bytes === "number" ? merged.downloaded_bytes : 0);
  if (status === "finished" && size != null) {
    downloaded = size;
  }
  const speed = status === "downloading"
    ? (typeof merged.speed === "number" ? merged.speed : (typeof merged.bytes_per_sec === "number" ? merged.bytes_per_sec : 0))
    : 0;

  let eta = typeof merged.eta === "number" ? merged.eta : null;
  if (eta == null && status === "downloading" && speed > 0 && size != null && size > downloaded) {
    eta = Math.ceil((size - downloaded) / speed);
  }

  const rawSegments = Array.isArray(merged.segments) ? merged.segments : [];
  const segments = rawSegments.map((seg) => ({
    start: Number(seg.start ?? seg.start_byte ?? 0),
    end: Number(seg.end ?? seg.end_byte ?? 0),
    downloaded: Number(seg.downloaded ?? seg.downloaded_bytes ?? 0),
    active: Boolean(seg.active ?? (seg.speed > 0)),
    speed: Number(seg.speed ?? 0)
  }));

  return {
    id: String(merged.id ?? merged.task_id ?? ""),
    name,
    url,
    status,
    size,
    downloaded,
    speed,
    eta,
    category: merged.category || inferCategory(name),
    queue: merged.queue || merged.queue_id || "Main Queue",
    destination: merged.destination || null,
    error: merged.error ?? merged.error_message ?? null,
    completedAt: merged.completedAt ?? merged.completed_at ?? null,
    avgSpeed: merged.avgSpeed ?? merged.avg_speed ?? null,
    segments
  };
}

function tickSampleTasks() {
  for (const t of SAMPLE_TASKS) {
    if (t.status !== "downloading") continue;
    let totalSpeed = 0;
    if (Array.isArray(t.segments) && t.segments.length > 0) {
      for (const seg of t.segments) {
        const cap = seg.end - seg.start;
        if (seg.downloaded >= cap) {
          seg.downloaded = cap;
          seg.active = false;
          seg.speed = 0;
          continue;
        }
        if (!seg.active) {
          const activeCount = t.segments.filter((s) => s.active).length;
          if (activeCount < 4) seg.active = true;
        }
        if (seg.active) {
          const jitter = 0.92 + Math.random() * 0.16;
          const baseSegSpeed = seg.speed > 0 ? seg.speed : 3800000;
          seg.speed = Math.round(baseSegSpeed * jitter);
          const step = Math.round(seg.speed * 0.5);
          seg.downloaded = Math.min(cap, seg.downloaded + step);
          if (seg.downloaded >= cap) {
            seg.active = false;
            seg.speed = 0;
          } else {
            totalSpeed += seg.speed;
          }
        }
      }
      t.downloaded = t.segments.reduce((acc, s) => acc + s.downloaded, 0);
      t.speed = totalSpeed;
    } else if (t.size) {
      t.speed = Math.round(6200000 * (0.9 + Math.random() * 0.2));
      t.downloaded = Math.min(t.size, t.downloaded + Math.round(t.speed * 0.5));
    }

    if (t.size && t.downloaded >= t.size) {
      t.downloaded = t.size;
      t.status = "finished";
      t.speed = 0;
      t.eta = 0;
      t.completedAt = Date.now();
      t.avgSpeed = 12845056;
    } else if (t.size && t.speed > 0) {
      t.eta = Math.ceil((t.size - t.downloaded) / t.speed);
    }
  }
}

export async function listTasks() {
  if (hasTauri()) {
    const res = await window.__TAURI__.core.invoke("list_tasks");
    const list = Array.isArray(res) ? res : (res?.tasks || []);
    return list.map((item) => normalizeTask(item));
  }
  return SAMPLE_TASKS.map((item) => normalizeTask(item));
}

export async function getAllProgress() {
  if (hasTauri()) {
    const res = await window.__TAURI__.core.invoke("get_all_progress");
    const map = new Map();
    if (res && typeof res === "object") {
      if (Array.isArray(res)) {
        for (const item of res) {
          const id = String(item.id ?? item.task_id ?? "");
          if (id) map.set(id, item);
        }
      } else {
        for (const [id, item] of Object.entries(res)) {
          map.set(id, { id, ...item });
        }
      }
    }
    return map;
  }
  tickSampleTasks();
  const map = new Map();
  for (const item of SAMPLE_TASKS) {
    map.set(item.id, { ...item });
  }
  return map;
}

export async function addTask(payload) {
  const { url, name, category, queue, folder, startNow = true } = payload;
  if (hasTauri()) {
    const res = await window.__TAURI__.core.invoke("add_task", {
      url,
      filename: name || undefined,
      start_now: startNow,
      destination: folder ? `${folder}\\${name || ''}` : undefined,
      queue_id: queue || undefined
    });
    return normalizeTask(res || { id: `task-${Date.now()}`, url, name, category, queue, status: startNow ? "downloading" : "queued" });
  }
  const resolvedName = name || inferNameFromUrl(url);
  const totalBytes = 536870912;
  const quarter = totalBytes / 4;
  const newTask = {
    id: `task-${Date.now()}-${Math.floor(Math.random() * 1000)}`,
    name: resolvedName,
    url,
    status: startNow ? "downloading" : "queued",
    size: totalBytes,
    downloaded: 0,
    speed: startNow ? 8388608 : 0,
    eta: startNow ? 64 : null,
    category: category || inferCategory(resolvedName),
    queue: queue || "Main Queue",
    segments: startNow
      ? [
          { start: 0, end: quarter, downloaded: 0, active: true, speed: 4194304 },
          { start: quarter, end: quarter * 2, downloaded: 0, active: true, speed: 4194304 },
          { start: quarter * 2, end: quarter * 3, downloaded: 0, active: false, speed: 0 },
          { start: quarter * 3, end: totalBytes, downloaded: 0, active: false, speed: 0 }
        ]
      : []
  };
  SAMPLE_TASKS.unshift(newTask);
  return normalizeTask(newTask);
}

export async function pauseTask(id) {
  if (hasTauri()) {
    await window.__TAURI__.core.invoke("pause_task", { id });
    return;
  }
  const found = SAMPLE_TASKS.find((t) => t.id === id);
  if (found && (found.status === "downloading" || found.status === "queued")) {
    found.status = "paused";
    found.speed = 0;
    if (found.segments) found.segments.forEach((s) => { s.active = false; s.speed = 0; });
  }
}

export async function resumeTask(id) {
  if (hasTauri()) {
    await window.__TAURI__.core.invoke("resume_task", { id });
    return;
  }
  const found = SAMPLE_TASKS.find((t) => t.id === id);
  if (found && (found.status === "paused" || found.status === "failed" || found.status === "queued")) {
    found.status = "downloading";
    found.error = null;
    if (!found.segments || found.segments.length === 0) {
      const sz = found.size || 268435456;
      const half = Math.floor(sz / 2);
      found.segments = [
        { start: 0, end: half, downloaded: Math.min(half, found.downloaded || 0), active: true, speed: 4194304 },
        { start: half, end: sz, downloaded: Math.max(0, (found.downloaded || 0) - half), active: true, speed: 4194304 }
      ];
    } else {
      found.segments.forEach((s) => {
        if (s.downloaded < s.end - s.start) {
          s.active = true;
          s.speed = 4194304;
        }
      });
    }
  }
}

export async function deleteTask(id, deleteFile = false) {
  if (hasTauri()) {
    await window.__TAURI__.core.invoke("delete_task", { id, deleteFile });
    return;
  }
  const idx = SAMPLE_TASKS.findIndex((t) => t.id === id);
  if (idx !== -1) SAMPLE_TASKS.splice(idx, 1);
}

export async function windowMinimize() {
  if (hasTauri()) {
    try {
      await window.__TAURI__.core.invoke("minimize_window");
      return;
    } catch {}
  }
  const win = window.__TAURI__?.window?.getCurrentWindow?.();
  if (win) await win.minimize();
}

export async function windowToggleMaximize() {
  if (hasTauri()) {
    try {
      await window.__TAURI__.core.invoke("toggle_maximize_window");
      return;
    } catch {}
  }
  const win = window.__TAURI__?.window?.getCurrentWindow?.();
  if (win) await win.toggleMaximize();
}

export async function windowClose() {
  if (hasTauri()) {
    try {
      await window.__TAURI__.core.invoke("close_window");
      return;
    } catch {}
  }
  const win = window.__TAURI__?.window?.getCurrentWindow?.();
  if (win) await win.close();
}

export function detectFilenameFromUrl(url) {
  return inferNameFromUrl(url);
}

export function detectCategoryFromFilename(name) {
  return inferCategory(name);
}

export async function openPath(path) {
  if (hasTauri()) {
    try {
      await window.__TAURI__.core.invoke("open_path", { path });
      return;
    } catch (e) {
      console.warn("open_path error:", e);
    }
  }
}

export async function showInFolder(path) {
  if (hasTauri()) {
    try {
      await window.__TAURI__.core.invoke("show_in_folder", { path });
      return;
    } catch (e) {
      console.warn("show_in_folder error:", e);
    }
  }
}

export async function selectFolder(defaultPath) {
  if (hasTauri()) {
    try {
      return await window.__TAURI__.core.invoke("select_folder", { defaultPath: defaultPath || undefined });
    } catch (e) {
      console.warn("select_folder error:", e);
    }
  }
  return null;
}

export async function getDiskFree(path) {
  if (hasTauri()) {
    try {
      return await window.__TAURI__.core.invoke("get_disk_free", { path: path || undefined });
    } catch (e) {
      console.warn("get_disk_free error:", e);
    }
  }
  return null;
}

export async function listQueues() {
  if (hasTauri()) {
    try {
      return await window.__TAURI__.core.invoke("list_queues");
    } catch (e) {
      console.warn("list_queues error:", e);
    }
  }
  return [];
}

export async function createQueue(name, maxConcurrent = 2) {
  if (hasTauri()) {
    try {
      return await window.__TAURI__.core.invoke("create_queue", { name, maxConcurrent });
    } catch (e) {
      console.warn("create_queue error:", e);
    }
  }
  return null;
}

export async function updateQueue(queue) {
  if (hasTauri()) {
    try {
      return await window.__TAURI__.core.invoke("update_queue", { queue });
    } catch (e) {
      console.warn("update_queue error:", e);
    }
  }
  return false;
}

export async function startQueue(id) {
  if (hasTauri()) {
    try {
      return await window.__TAURI__.core.invoke("start_queue", { id });
    } catch (e) {
      console.warn("start_queue error:", e);
    }
  }
  return false;
}

export async function pauseQueue(id) {
  if (hasTauri()) {
    try {
      return await window.__TAURI__.core.invoke("pause_queue", { id });
    } catch (e) {
      console.warn("pause_queue error:", e);
    }
  }
  return false;
}

export async function abortShutdown() {
  if (hasTauri()) {
    try {
      return await window.__TAURI__.core.invoke("abort_shutdown");
    } catch (e) {
      console.warn("abort_shutdown error:", e);
    }
  }
  return false;
}

export async function triggerPowerAction(action) {
  if (hasTauri()) {
    try {
      return await window.__TAURI__.core.invoke("trigger_power_action", { action });
    } catch (e) {
      console.warn("trigger_power_action error:", e);
    }
  }
  return false;
}

export async function getPendingIntercepts() {
  if (hasTauri()) {
    try {
      return await window.__TAURI__.core.invoke("get_pending_intercepts");
    } catch (e) {
      console.warn("get_pending_intercepts error:", e);
    }
  }
  return [];
}