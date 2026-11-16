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

export function inferNameFromUrl(url = "") {
  if (!url || typeof url !== "string") return "download.bin";
  try {
    const u = new URL(url.trim());

    // 1. Check query parameters for explicit filename hints
    const params = u.searchParams;
    const queryKeys = ["filename", "file_name", "fn", "file", "name", "title", "response-content-disposition"];
    for (const k of queryKeys) {
      const val = params.get(k);
      if (val && val.trim()) {
        let clean = decodeURIComponent(val.trim());
        if (clean.includes("filename=")) {
          const m = clean.match(/filename\*?=['"]?(?:UTF-8'')?([^'";]+)['"]?/i);
          if (m && m[1]) clean = decodeURIComponent(m[1].trim());
        }
        clean = clean.split(/[/\\]/).pop() || "";
        if (clean && clean.includes(".") && !clean.endsWith(".")) {
          return clean;
        }
      }
    }

    // 2. Check path segments
    const segments = u.pathname.split("/").map((s) => decodeURIComponent(s.trim())).filter(Boolean);
    const scriptExts = [".php", ".jsp", ".asp", ".aspx", ".cgi", ".do", ".action", ".html", ".htm"];
    const genericBases = ["download", "dl", "get", "media", "stream", "file", "view"];

    while (segments.length > 0) {
      const last = segments.pop();
      if (!last) continue;
      const lower = last.toLowerCase();
      const isScript = scriptExts.some((ext) => lower.endsWith(ext));
      const isGeneric = genericBases.includes(lower);

      if (!isScript && !isGeneric && last.includes(".")) {
        return last;
      }
      if (!isScript && !isGeneric && last.length > 0 && segments.length === 0) {
        return last;
      }
    }

    for (const k of ["title", "name", "filename", "file"]) {
      const val = params.get(k);
      if (val && val.trim()) {
        const clean = decodeURIComponent(val.trim()).split(/[/\\]/).pop();
        if (clean) return clean;
      }
    }

    return "download.bin";
  } catch {
    const clean = String(url).split(/[/\\]/).pop()?.split("?")[0];
    return clean || "download.bin";
  }
}

export function normalizeTask(raw = {}, progressOverride = null) {
  const p = progressOverride || {};
  let status = normalizeStatus(p.status ?? p.state ?? raw.status ?? raw.state);
  if (p.is_complete || raw.is_complete) {
    status = "finished";
  }

  const url = raw.url ?? raw.uri ?? p.url ?? "";
  const name = raw.name ?? raw.filename ?? raw.file_name ?? p.name ?? inferNameFromUrl(url);

  // Total size resolution: prefer progressOverride, then raw
  let size = null;
  if (typeof p.total_bytes === "number" && p.total_bytes > 0) size = p.total_bytes;
  else if (typeof p.total_size === "number" && p.total_size > 0) size = p.total_size;
  else if (typeof p.size === "number" && p.size > 0) size = p.size;
  else if (typeof raw.total_size === "number" && raw.total_size > 0) size = raw.total_size;
  else if (typeof raw.total_bytes === "number" && raw.total_bytes > 0) size = raw.total_bytes;
  else if (typeof raw.size === "number" && raw.size > 0) size = raw.size;

  // Downloaded bytes resolution: prefer progressOverride, then raw
  let downloaded = 0;
  if (typeof p.downloaded_bytes === "number") downloaded = p.downloaded_bytes;
  else if (typeof p.downloaded === "number") downloaded = p.downloaded;
  else if (typeof raw.downloaded_bytes === "number") downloaded = raw.downloaded_bytes;
  else if (typeof raw.downloaded === "number") downloaded = raw.downloaded;

  if (status === "finished" && size != null) {
    downloaded = size;
  }

  // Speed resolution: prefer progressOverride, then raw
  let speed = 0;
  if (status === "downloading") {
    if (typeof p.speed_bytes_per_sec === "number") speed = p.speed_bytes_per_sec;
    else if (typeof p.speed === "number") speed = p.speed;
    else if (typeof p.bytes_per_sec === "number") speed = p.bytes_per_sec;
    else if (typeof raw.speed_bytes_per_sec === "number") speed = raw.speed_bytes_per_sec;
    else if (typeof raw.speed === "number") speed = raw.speed;
    else if (typeof raw.bytes_per_sec === "number") speed = raw.bytes_per_sec;
  }

  let eta = typeof p.eta === "number" ? p.eta : (typeof raw.eta === "number" ? raw.eta : null);
  if (eta == null && status === "downloading" && speed > 0 && size != null && size > downloaded) {
    eta = Math.ceil((size - downloaded) / speed);
  }

  const rawSegments = Array.isArray(p.segments) && p.segments.length > 0
    ? p.segments
    : (Array.isArray(raw.segments) ? raw.segments : []);

  const segments = rawSegments.map((seg) => {
    const start = Number(seg.start ?? seg.start_byte ?? 0);
    const end = Number(seg.end ?? seg.end_byte ?? 0);
    let segDownloaded = 0;
    if (typeof seg.downloaded_bytes === "number") {
      segDownloaded = seg.downloaded_bytes;
    } else if (typeof seg.downloaded === "number") {
      segDownloaded = seg.downloaded;
    } else if (typeof seg.current === "number") {
      segDownloaded = Math.max(0, seg.current - start);
    }
    const isAct = Boolean(seg.is_active ?? seg.active ?? (seg.speed > 0));
    return {
      start,
      end,
      downloaded: segDownloaded,
      active: isAct,
      speed: Number(seg.speed ?? 0)
    };
  });

  return {
    id: String(raw.id ?? raw.task_id ?? p.id ?? ""),
    name,
    url,
    status,
    size,
    downloaded,
    speed,
    eta,
    category: raw.category || inferCategory(name),
    queue: (raw.queue === "main" || raw.queue_id === "main") ? "Main Queue" : ((raw.queue === "night" || raw.queue_id === "night") ? "Night Queue" : (raw.queue || raw.queue_id || "Main Queue")),
    destination: raw.destination || null,
    error: p.error ?? p.error_message ?? raw.error ?? raw.error_message ?? null,
    completedAt: raw.completedAt ?? raw.completed_at ?? p.completedAt ?? null,
    avgSpeed: raw.avgSpeed ?? raw.avg_speed ?? p.avgSpeed ?? null,
    segments,
    uploadSpeed: p.upload_speed_bytes_per_sec ?? raw.uploadSpeed ?? 0,
    uploadedBytes: p.uploaded_bytes ?? raw.uploadedBytes ?? 0,
    numSeeders: p.num_seeders ?? raw.numSeeders ?? 0,
    numPeers: p.num_peers ?? p.active_connections ?? raw.numPeers ?? 0,
    isSeeding: Boolean(p.is_seeding ?? raw.isSeeding),
    infoHash: p.info_hash ?? raw.infoHash ?? null,
    pieceLength: p.piece_length ?? raw.pieceLength ?? null,
    numPieces: p.num_pieces ?? raw.numPieces ?? null,
    torrentName: p.torrent_name ?? raw.torrentName ?? null,
    torrentComment: p.torrent_comment ?? raw.torrentComment ?? null,
    torrentCreationDate: p.torrent_creation_date ?? raw.torrentCreationDate ?? null
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
  const { url, name, category, queue, folder, startNow = true, selectFiles, seedRatio, referrer, cookies, userAgent, headers } = payload;
  const cleanName = (name || "").split(/[/\\]/).pop() || "";
  const cleanFolder = (folder || "").trim();
  const destination = cleanFolder
    ? (cleanName ? `${cleanFolder.replace(/[/\\]+$/, '')}\\${cleanName}` : cleanFolder)
    : undefined;

  if (hasTauri()) {
    const res = await window.__TAURI__.core.invoke("add_task", {
      url,
      filename: cleanName || undefined,
      start_now: startNow,
      startNow: startNow,
      destination,
      queue_id: queue || undefined,
      queueId: queue || undefined,
      select_files: selectFiles || undefined,
      selectFiles: selectFiles || undefined,
      seed_ratio: typeof seedRatio === "number" ? seedRatio : undefined,
      seedRatio: typeof seedRatio === "number" ? seedRatio : undefined,
      referrer: referrer || undefined,
      cookies: cookies || undefined,
      user_agent: userAgent || undefined,
      userAgent: userAgent || undefined,
      headers: headers || undefined
    });
    return normalizeTask(res || { id: `task-${Date.now()}`, url, name: cleanName, category, queue, status: startNow ? "downloading" : "queued" });
  }

  const resolvedName = cleanName || inferNameFromUrl(url);
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
    try {
      await window.__TAURI__.core.invoke("pause_task", { id });
    } catch (e) {
      console.warn("pause_task error:", e);
    }
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
    try {
      await window.__TAURI__.core.invoke("resume_task", { id });
    } catch (e) {
      console.warn("resume_task error:", e);
    }
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
    try {
      await window.__TAURI__.core.invoke("delete_task", { id, delete_file: deleteFile, deleteFile });
    } catch (e) {
      console.warn("delete_task error:", e);
    }
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

export async function windowStartDragging() {
  if (hasTauri()) {
    try {
      await window.__TAURI__.core.invoke("start_dragging");
      return;
    } catch (e) {
      console.warn("start_dragging error:", e);
    }
  }
  const win = window.__TAURI__?.window?.getCurrentWindow?.();
  if (win && typeof win.startDragging === "function") {
    await win.startDragging();
  }
}

export async function probeDownload(url) {
  if (hasTauri() && url && (url.startsWith("http://") || url.startsWith("https://"))) {
    try {
      return await window.__TAURI__.core.invoke("probe_download", { url });
    } catch (e) {
      console.warn("probe_download error:", e);
    }
  }
  return null;
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
export async function moveTaskToQueue(taskId, queueId) {
  if (hasTauri()) {
    try {
      return await window.__TAURI__.core.invoke("move_task_to_queue", {
        taskId: String(taskId),
        queueId: String(queueId)
      });
    } catch (e) {
      console.warn("move_task_to_queue error:", e);
    }
  }
  return false;
}

export async function getAllDrives() {
  if (hasTauri()) {
    try {
      return await window.__TAURI__.core.invoke("get_all_drives");
    } catch (e) {
      console.warn("get_all_drives error:", e);
    }
  }
  return [
    { letter: "C:\\", label: "Local Disk (C:)", total_bytes: 119047157760, free_bytes: 20080939008, drive_type: "fixed" },
    { letter: "D:\\", label: "Local Disk (D:)", total_bytes: 256051916800, free_bytes: 97731022848, drive_type: "fixed" }
  ];
}

export async function selectTorrentFile() {
  if (hasTauri()) {
    try {
      return await window.__TAURI__.core.invoke("select_torrent_file");
    } catch (e) {
      console.warn("select_torrent_file error:", e);
    }
  }
  return null;
}

export async function inspectTorrentFile(path) {
  if (hasTauri()) {
    try {
      return await window.__TAURI__.core.invoke("inspect_torrent_file", { path });
    } catch (e) {
      console.warn("inspect_torrent_file error:", e);
    }
  }
  return null;
}

export async function reorderQueueTasks(queueId, taskIds) {
  if (hasTauri()) {
    try {
      return await window.__TAURI__.core.invoke("reorder_queue_tasks", {
        queue_id: queueId,
        queueId: queueId,
        task_ids: taskIds,
        taskIds: taskIds
      });
    } catch (e) {
      console.warn("reorder_queue_tasks error:", e);
    }
  }
  return true;
}
