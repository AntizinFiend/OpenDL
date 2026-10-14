// js/dock.js
import {
  store,
  setDockCollapsed,
  setDockHeight,
  setDockTab,
  setSegViewMode,
  setSoloSegment,
  appendWireLog,
  formatBytes,
  formatSpeedParts,
  decodeTitleEffect,
  playPiezo
} from "./state.js";

let hoverState = null; // { x, y, ratio, laneIdx, sectorIdx, overReconnect }
const laneVisualWidths = new Map();
const reconnectHitBoxes = []; // [{ laneIdx, x, y, w, h }]

function getSelectedTask() {
  if (store.lastSelectedId && store.selectedIds.has(store.lastSelectedId)) {
    return store.taskMap.get(store.lastSelectedId) || null;
  }
  const firstId = store.selectedIds.values().next().value;
  return firstId ? store.taskMap.get(firstId) || null : null;
}

function createDiagonalHatchPattern(ctx, strokeColor, bgColor) {
  const pCanvas = document.createElement("canvas");
  pCanvas.width = 8;
  pCanvas.height = 8;
  const pCtx = pCanvas.getContext("2d");
  pCtx.fillStyle = bgColor;
  pCtx.fillRect(0, 0, 8, 8);
  pCtx.strokeStyle = strokeColor;
  pCtx.lineWidth = 1;
  pCtx.beginPath();
  pCtx.moveTo(0, 8);
  pCtx.lineTo(8, 0);
  pCtx.stroke();
  return ctx.createPattern(pCanvas, "repeat");
}

function drawLanesView(ctx, w, h, task, palette) {
  const { colText, colSec, colBorder, colTrack, colAccent, hatchPattern, fontMono } = palette;
  reconnectHitBoxes.length = 0;

  const rulerHeight = 22;
  const leftPad = 82;
  const rightPad = 128;
  const barW = Math.max(60, w - leftPad - rightPad);

  ctx.strokeStyle = colBorder;
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(leftPad, rulerHeight);
  ctx.lineTo(leftPad + barW, rulerHeight);
  ctx.stroke();

  const totalSize = task?.size || 0;
  const majorTicks = 4;
  ctx.fillStyle = colSec;
  ctx.font = `11px ${fontMono}`;
  for (let i = 0; i <= majorTicks; i++) {
    const ratio = i / majorTicks;
    const x = Math.round(leftPad + ratio * barW) + 0.5;
    ctx.beginPath();
    ctx.moveTo(x, rulerHeight - 6);
    ctx.lineTo(x, rulerHeight);
    ctx.stroke();
    const label = totalSize > 0 ? formatBytes(Math.round(totalSize * ratio)) : `${i * 25}%`;
    ctx.textAlign = i === 0 ? "left" : (i === majorTicks ? "right" : "center");
    ctx.fillText(label, x, 12);

    if (i < majorTicks) {
      for (let m = 1; m < 4; m++) {
        const mx = Math.round(leftPad + ((i + m / 4) / majorTicks) * barW) + 0.5;
        ctx.beginPath();
        ctx.moveTo(mx, rulerHeight - 3);
        ctx.lineTo(mx, rulerHeight);
        ctx.stroke();
      }
    }
  }

  if (!task || (task.status !== "downloading" && (!task.segments || task.segments.length === 0) && task.status !== "finished" && task.status !== "paused")) {
    const laneY = rulerHeight + 14;
    const laneH = h - laneY - 12;
    ctx.fillStyle = hatchPattern;
    ctx.fillRect(leftPad, laneY, barW, laneH);
    ctx.strokeStyle = colBorder;
    ctx.strokeRect(leftPad + 0.5, laneY + 0.5, barW - 1, laneH - 1);
    ctx.fillStyle = colSec;
    ctx.textAlign = "center";
    ctx.fillText("NO ACTIVE CONNECTIONS", leftPad + barW / 2, laneY + laneH / 2 + 4);
    return;
  }

  const lanes = task.segments && task.segments.length > 0
    ? task.segments
    : [{
        start: 0,
        end: task.size || 100,
        downloaded: task.status === "finished" ? (task.size || 100) : (task.downloaded || 0),
        active: task.status === "downloading",
        speed: task.speed || 0
      }];

  const availH = h - rulerHeight - 8;
  const laneStep = Math.max(13, Math.floor(availH / lanes.length));
  const laneH = Math.max(8, laneStep - 4);

  lanes.forEach((seg, idx) => {
    const y = rulerHeight + 6 + idx * laneStep;
    const isDimmed = store.soloSegmentIdx !== null && store.soloSegmentIdx !== idx;
    const isHovered = hoverState && hoverState.laneIdx === idx;

    ctx.save();
    if (isDimmed) ctx.globalAlpha = 0.22;

    const label = `${store.soloSegmentIdx === idx ? ">" : " "}SEG ${String(idx + 1).padStart(2, "0")}/${String(lanes.length).padStart(2, "0")}`;
    ctx.fillStyle = store.soloSegmentIdx === idx || isHovered ? colAccent : colSec;
    ctx.textAlign = "left";
    ctx.fillText(label, 4, y + laneH - 1);

    ctx.fillStyle = hatchPattern;
    ctx.fillRect(leftPad, y, barW, laneH);
    ctx.strokeStyle = isHovered ? colAccent : colBorder;
    ctx.strokeRect(leftPad + 0.5, y + 0.5, barW - 1, laneH - 1);

    const span = Math.max(1, seg.end - seg.start);
    const targetRatio = task.status === "finished" ? 1 : Math.min(1, Math.max(0, seg.downloaded / span));
    const key = `${task.id}:${idx}`;
    const prev = laneVisualWidths.get(key) ?? targetRatio;
    const curRatio = prev + (targetRatio - prev) * 0.38;
    laneVisualWidths.set(key, curRatio);

    const fillW = Math.round(barW * curRatio);
    if (fillW > 0) {
      ctx.fillStyle = colAccent;
      ctx.fillRect(leftPad + 1, y + 1, Math.max(0, fillW - 2), laneH - 2);
      if (seg.active && task.status === "downloading") {
        ctx.fillStyle = colText;
        ctx.fillRect(leftPad + fillW - 2, y + 1, 2, laneH - 2);
      }
    }

    const sp = seg.active && seg.speed > 0 ? formatSpeedParts(seg.speed) : null;
    ctx.fillStyle = sp ? colText : colSec;
    ctx.textAlign = "right";
    ctx.fillText(sp ? `${sp.value} ${sp.unit}` : (targetRatio >= 1 ? "EOF" : "IDLE"), w - 44, y + laneH - 1);

    // Inline [RCN] reconnect micro-trigger on each lane
    const btnX = w - 36;
    const btnW = 30;
    reconnectHitBoxes.push({ laneIdx: idx, x: btnX, y: y - 1, w: btnW, h: laneH + 2 });
    ctx.strokeStyle = isHovered ? colAccent : colBorder;
    ctx.strokeRect(btnX + 0.5, y - 0.5, btnW - 1, laneH + 1);
    ctx.fillStyle = isHovered ? colAccent : colSec;
    ctx.textAlign = "center";
    ctx.fillText("RCN", btnX + btnW / 2, y + laneH - 1);

    ctx.restore();
  });

  if (hoverState && hoverState.ratio >= 0 && hoverState.ratio <= 1) {
    const cx = Math.round(leftPad + hoverState.ratio * barW) + 0.5;
    ctx.strokeStyle = colAccent;
    ctx.beginPath();
    ctx.moveTo(cx, 0);
    ctx.lineTo(cx, h);
    ctx.stroke();
  }
}

function drawSectorMatrixView(ctx, w, h, task, palette) {
  const { colText, colSec, colBorder, colAccent, hatchPattern, fontMono } = palette;
  const padX = 10;
  const padY = 8;
  const cellS = 7;
  const gap = 2;
  const step = cellS + gap;

  const cols = Math.max(16, Math.floor((w - padX * 2) / step));
  const rows = Math.max(4, Math.floor((h - padY * 2 - 14) / step));
  const totalCells = cols * rows;

  ctx.font = `11px ${fontMono}`;

  if (!task) {
    ctx.fillStyle = hatchPattern;
    ctx.fillRect(padX, padY, w - padX * 2, h - padY * 2);
    ctx.strokeStyle = colBorder;
    ctx.strokeRect(padX + 0.5, padY + 0.5, w - padX * 2 - 1, h - padY * 2 - 1);
    ctx.fillStyle = colSec;
    ctx.textAlign = "center";
    ctx.fillText("NO TASK SELECTED // SECTOR MATRIX STANDBY", w / 2, h / 2 + 4);
    return;
  }

  const totalBytes = Math.max(1, task.size || 100);
  const segs = task.segments && task.segments.length > 0
    ? task.segments
    : [{
        start: 0,
        end: totalBytes,
        downloaded: task.status === "finished" ? totalBytes : (task.downloaded || 0),
        active: task.status === "downloading"
      }];

  let hoveredSectorInfo = null;

  for (let i = 0; i < totalCells; i++) {
    const c = i % cols;
    const r = Math.floor(i / cols);
    const x = padX + c * step;
    const y = padY + r * step;

    const sectorStartByte = (i / totalCells) * totalBytes;
    const sectorEndByte = ((i + 1) / totalCells) * totalBytes;

    let ownerIdx = 0;
    for (let sIdx = 0; sIdx < segs.length; sIdx++) {
      if (sectorStartByte >= segs[sIdx].start && sectorStartByte < segs[sIdx].end) {
        ownerIdx = sIdx;
        break;
      }
    }
    const seg = segs[ownerIdx] || segs[0];
    const segWriteEdge = seg.start + (task.status === "finished" ? (seg.end - seg.start) : seg.downloaded);

    const isDimmed = store.soloSegmentIdx !== null && store.soloSegmentIdx !== ownerIdx;
    ctx.save();
    if (isDimmed) ctx.globalAlpha = 0.2;

    if (sectorEndByte <= segWriteEdge || task.status === "finished") {
      ctx.fillStyle = colAccent;
      ctx.fillRect(x, y, cellS, cellS);
    } else if (sectorStartByte <= segWriteEdge && sectorEndByte > segWriteEdge && seg.active && task.status === "downloading") {
      // Active write head cell
      ctx.fillStyle = colText;
      ctx.fillRect(x, y, cellS, cellS);
    } else {
      ctx.fillStyle = hatchPattern;
      ctx.fillRect(x, y, cellS, cellS);
      ctx.strokeStyle = colBorder;
      ctx.strokeRect(x + 0.5, y + 0.5, cellS - 1, cellS - 1);
    }

    if (hoverState && hoverState.x >= x && hoverState.x <= x + step && hoverState.y >= y && hoverState.y <= y + step) {
      ctx.strokeStyle = colText;
      ctx.strokeRect(x - 0.5, y - 0.5, cellS + 1, cellS + 1);
      hoveredSectorInfo = `SECTOR #${String(i).padStart(4, "0")} // SEG ${String(ownerIdx + 1).padStart(2, "0")} // OFFSET ${formatBytes(Math.round(sectorStartByte))}`;
    }

    ctx.restore();
  }

  ctx.fillStyle = colSec;
  ctx.textAlign = "left";
  ctx.fillText(
    hoveredSectorInfo || `WAFER ALLOCATION: ${totalCells} SECTORS (${formatBytes(Math.round(totalBytes / totalCells))} / SECTOR)`,
    padX,
    h - 3
  );
}

function drawSegmentsCanvas(task) {
  const canvas = document.getElementById("segments-canvas");
  if (!canvas) return;
  const rect = canvas.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  const w = Math.max(200, Math.floor(rect.width));
  const h = Math.max(80, Math.floor(rect.height));
  if (canvas.width !== w * dpr || canvas.height !== h * dpr) {
    canvas.width = w * dpr;
    canvas.height = h * dpr;
  }
  const ctx = canvas.getContext("2d");
  ctx.save();
  ctx.scale(dpr, dpr);
  ctx.clearRect(0, 0, w, h);

  const styles = getComputedStyle(document.documentElement);
  const colText = styles.getPropertyValue("--text").trim() || "#EDEAE4";
  const colSec = styles.getPropertyValue("--text-secondary").trim() || "#8C877E";
  const colBorder = styles.getPropertyValue("--border").trim() || "#282724";
  const colTrack = styles.getPropertyValue("--track").trim() || "#201F1C";
  const colAccent = styles.getPropertyValue("--accent").trim() || "#F59E0B";
  const colHatch = styles.getPropertyValue("--hatch-stroke").trim() || "rgba(255,255,255,0.08)";
  const fontMono = styles.getPropertyValue("--font-mono").trim() || "monospace";
  const hatchPattern = createDiagonalHatchPattern(ctx, colHatch, colTrack);

  const palette = { colText, colSec, colBorder, colTrack, colAccent, hatchPattern, fontMono };

  if (store.segViewMode === "matrix") {
    drawSectorMatrixView(ctx, w, h, task, palette);
  } else {
    drawLanesView(ctx, w, h, task, palette);
  }

  ctx.restore();
}

function drawTaskSparkline(task) {
  const canvas = document.getElementById("task-sparkline-canvas");
  if (!canvas) return;
  const rect = canvas.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  const w = Math.max(120, Math.floor(rect.width));
  const h = Math.max(20, Math.floor(rect.height));
  if (canvas.width !== w * dpr || canvas.height !== h * dpr) {
    canvas.width = w * dpr;
    canvas.height = h * dpr;
  }
  const ctx = canvas.getContext("2d");
  ctx.save();
  ctx.scale(dpr, dpr);
  ctx.clearRect(0, 0, w, h);

  const samples = task ? store.taskSparklines.get(task.id) : null;
  if (!samples || samples.length < 2) {
    ctx.restore();
    return;
  }

  const styles = getComputedStyle(document.documentElement);
  const accent = styles.getPropertyValue("--accent-text").trim() || "#F59E0B";
  const fill = styles.getPropertyValue("--accent-fill-graph").trim() || "rgba(245,158,11,0.14)";
  const max = Math.max(...samples, 1);

  ctx.beginPath();
  samples.forEach((v, i) => {
    const x = (i / (samples.length - 1)) * w;
    const y = h - 2 - (v / max) * (h - 5);
    if (i === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  });
  ctx.strokeStyle = accent;
  ctx.lineWidth = 1;
  ctx.stroke();

  ctx.lineTo(w, h);
  ctx.lineTo(0, h);
  ctx.closePath();
  ctx.fillStyle = fill;
  ctx.fill();
  ctx.restore();
}

function renderWireInspector(task) {
  const hostEl = document.getElementById("wire-host");
  const schemeEl = document.getElementById("wire-scheme");
  const rangesEl = document.getElementById("wire-ranges");
  const muxEl = document.getElementById("wire-multiplex");
  const viewport = document.getElementById("wire-log-viewport");
  if (!viewport) return;

  if (!task) {
    if (hostEl) hostEl.textContent = "--";
    if (schemeEl) schemeEl.textContent = "--";
    if (muxEl) muxEl.textContent = "0 STREAMS";
    viewport.innerHTML = `<div class="micro-label">NO CHANNEL SELECTED // AWAITING PACKET CAPTURE</div>`;
    return;
  }

  let parsedHost = "LOCAL.MIRROR";
  let parsedScheme = "HTTPS // TLS 1.3";
  try {
    const u = new URL(task.url);
    parsedHost = u.host.toUpperCase();
    parsedScheme = `${u.protocol.replace(":", "").toUpperCase()} // PORT ${u.port || (u.protocol === "https:" ? "443" : "80")}`;
  } catch {}

  const activeStreams = task.segments?.filter((s) => s.active).length || (task.status === "downloading" ? 1 : 0);
  const totalStreams = task.segments?.length || 1;
  if (hostEl) hostEl.textContent = parsedHost;
  if (schemeEl) schemeEl.textContent = parsedScheme;
  if (rangesEl) rangesEl.textContent = totalStreams > 1 ? "BYTES (HTTP 206 MULTIPART)" : "BYTES (SINGLE STREAM)";
  if (muxEl) muxEl.textContent = `${activeStreams} ACTIVE / ${totalStreams} ALLOCATED`;

  const logs = store.wireLogs.get(task.id) || [];
  viewport.innerHTML = logs
    .map((entry) => {
      const statusCls = entry.level === "ok" ? "is-ok" : (entry.level === "err" ? "is-err" : "");
      return `
        <div class="wire-log-row">
          <span class="wire-ts">${entry.ts}</span>
          <span class="wire-verb">[${entry.verb}]</span>
          <span class="wire-detail">${entry.detail}</span>
          <span class="spec-leader" aria-hidden="true"></span>
          <span class="wire-status ${statusCls}">${entry.status}</span>
        </div>
      `;
    })
    .join("");
}

export function renderDock() {
  const dockEl = document.getElementById("dock-panel");
  if (!dockEl) return;

  dockEl.classList.toggle("is-collapsed", store.dockCollapsed);
  if (!store.dockCollapsed) {
    dockEl.style.height = `${store.dockHeight}px`;
  }

  const tabs = ["segments", "throughput", "wire"];
  for (const t of tabs) {
    const btn = document.getElementById(`tab-btn-${t}`);
    const panel = document.getElementById(`tab-panel-${t}`);
    const active = store.dockTab === t;
    btn?.classList.toggle("is-active", active);
    btn?.setAttribute("aria-selected", String(active));
    panel?.classList.toggle("is-hidden", !active);
  }

  document.querySelectorAll("[data-seg-mode]").forEach((btn) => {
    btn.classList.toggle("is-active", btn.getAttribute("data-seg-mode") === store.segViewMode);
  });

  const soloClearBtn = document.getElementById("btn-clear-solo");
  if (soloClearBtn) {
    soloClearBtn.classList.toggle("is-hidden", store.soloSegmentIdx === null);
    if (store.soloSegmentIdx !== null) {
      soloClearBtn.textContent = `SOLO SEG 0${store.soloSegmentIdx + 1} // CLEAR`;
    }
  }

  if (store.dockCollapsed) return;

  const task = getSelectedTask();
  const ledEl = document.getElementById("seg-task-led");
  const wordEl = document.getElementById("seg-task-state-word");
  const nameEl = document.getElementById("seg-task-name");
  const specConn = document.getElementById("spec-connections");
  const specDown = document.getElementById("spec-downloaded");
  const specVel = document.getElementById("spec-velocity");
  const specQueue = document.getElementById("spec-queue");

  if (!task) {
    ledEl?.setAttribute("data-state", "idle");
    if (wordEl) wordEl.textContent = "IDLE";
    if (nameEl) nameEl.textContent = "Select a task to inspect byte allocation";
    if (specConn) specConn.textContent = "0 / 0";
    if (specDown) specDown.textContent = "--";
    if (specVel) specVel.textContent = "0.0 KB/S";
    if (specQueue) specQueue.textContent = "--";
  } else {
    ledEl?.setAttribute("data-state", task.status);
    if (wordEl) wordEl.textContent = task.status.toUpperCase();
    if (nameEl) nameEl.textContent = task.name || "Unnamed resource";
    const totalSegs = task.segments?.length || 0;
    const activeSegs = task.segments?.filter((s) => s.active).length || 0;
    if (specConn) specConn.textContent = totalSegs > 0 ? `${activeSegs} ACTIVE / ${totalSegs} TOTAL` : (task.status === "downloading" ? "1 STREAM" : "0 ACTIVE");
    if (specDown) {
      const pct = task.size ? Math.min(100, Math.floor((task.downloaded / task.size) * 100)) : 0;
      specDown.textContent = task.size ? `${formatBytes(task.downloaded)} / ${formatBytes(task.size)} (${pct}%)` : formatBytes(task.downloaded);
    }
    if (specVel) {
      const sp = formatSpeedParts(task.speed);
      specVel.textContent = `${sp.value} ${sp.unit}`;
    }
    if (specQueue) specQueue.textContent = task.queue || "Main Queue";
  }

  if (store.dockTab === "segments") {
    drawSegmentsCanvas(task);
    drawTaskSparkline(task);
  } else if (store.dockTab === "wire") {
    renderWireInspector(task);
  }
}

export function initDockInteractions() {
  const headerEl = document.getElementById("dock-header");
  const closeBtn = document.getElementById("btn-dock-close");
  const resizeHandle = document.getElementById("dock-resize-handle");
  const segCanvas = document.getElementById("segments-canvas");
  const hoverReadout = document.getElementById("segments-hover-readout");
  const titleEl = document.getElementById("dock-title-decode");

  const tabTitles = {
    segments: "SEGMENT TELEMETRY",
    throughput: "BUS THROUGHPUT",
    wire: "PROTOCOL // WIRE INSPECTOR"
  };

  document.querySelectorAll(".dock-tab").forEach((btn) => {
    btn.addEventListener("click", (e) => {
      e.stopPropagation();
      const tab = btn.getAttribute("data-tab");
      if (store.dockCollapsed) setDockCollapsed(false);
      setDockTab(tab);
      playPiezo("snap");
      decodeTitleEffect(titleEl, tabTitles[tab] || "TELEMETRY");
    });
  });

  document.querySelectorAll("[data-seg-mode]").forEach((btn) => {
    btn.addEventListener("click", () => {
      setSegViewMode(btn.getAttribute("data-seg-mode"));
    });
  });

  document.getElementById("btn-clear-solo")?.addEventListener("click", () => {
    setSoloSegment(null);
  });

  headerEl?.addEventListener("dblclick", () => {
    const next = !store.dockCollapsed;
    setDockCollapsed(next);
    playPiezo("snap");
    if (!next) decodeTitleEffect(titleEl, tabTitles[store.dockTab] || "TELEMETRY");
  });

  closeBtn?.addEventListener("click", (e) => {
    e.stopPropagation();
    setDockCollapsed(!store.dockCollapsed);
    playPiezo("tick");
  });

  resizeHandle?.addEventListener("mousedown", (e) => {
    e.preventDefault();
    const startY = e.clientY;
    const startH = store.dockHeight;
    function onMove(ev) {
      const delta = startY - ev.clientY;
      if (store.dockCollapsed && delta > 10) setDockCollapsed(false);
      setDockHeight(startH + delta);
    }
    function onUp() {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    }
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  });

  segCanvas?.addEventListener("mousemove", (e) => {
    const rect = segCanvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    const task = getSelectedTask();

    if (store.segViewMode === "matrix") {
      hoverState = { x, y, ratio: -1, laneIdx: null };
      hoverReadout?.classList.add("is-hidden");
      renderDock();
      return;
    }

    const leftPad = 82;
    const rightPad = 128;
    const barW = Math.max(60, rect.width - leftPad - rightPad);
    const ratio = (x - leftPad) / barW;

    const lanesCount = task?.segments?.length || 1;
    const rulerHeight = 22;
    const availH = rect.height - rulerHeight - 8;
    const laneStep = Math.max(13, Math.floor(availH / lanesCount));
    const laneIdx = y > rulerHeight + 4 ? Math.floor((y - rulerHeight - 6) / laneStep) : null;
    const validLaneIdx = laneIdx !== null && laneIdx >= 0 && laneIdx < lanesCount ? laneIdx : null;

    hoverState = { x, y, ratio, laneIdx: validLaneIdx };

    if (hoverReadout && ratio >= 0 && ratio <= 1) {
      hoverReadout.classList.remove("is-hidden");
      hoverReadout.style.left = `${Math.min(rect.width - 110, Math.max(110, x))}px`;
      const bytePos = task?.size ? formatBytes(Math.round(task.size * ratio)) : `${Math.round(ratio * 100)}%`;
      if (validLaneIdx !== null && task?.segments?.[validLaneIdx]) {
        const s = task.segments[validLaneIdx];
        hoverReadout.textContent = `SEG 0${validLaneIdx + 1} [${formatBytes(s.start)} — ${formatBytes(s.end)}] // CURSOR ${bytePos}`;
      } else {
        hoverReadout.textContent = `OFFSET ${bytePos}`;
      }
    } else {
      hoverReadout?.classList.add("is-hidden");
    }
    renderDock();
  });

  segCanvas?.addEventListener("click", (e) => {
    if (store.segViewMode !== "lanes") return;
    const rect = segCanvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    const task = getSelectedTask();
    if (!task) return;

    // Check if operator clicked [RCN] reconnect box
    for (const box of reconnectHitBoxes) {
      if (x >= box.x && x <= box.x + box.w && y >= box.y && y <= box.y + box.h) {
        const seg = task.segments?.[box.laneIdx];
        if (seg) {
          seg.active = true;
          appendWireLog(
            task.id,
            `SEG#${String(box.laneIdx + 1).padStart(2, "0")}`,
            `RECONNECT SOCKET // RANGE bytes=${seg.start + seg.downloaded}-${seg.end}`,
            "206 RESET OK",
            "ok"
          );
          playPiezo("snap");
          renderDock();
        }
        return;
      }
    }

    if (hoverState && hoverState.laneIdx !== null) {
      setSoloSegment(hoverState.laneIdx);
    }
  });

  segCanvas?.addEventListener("mouseleave", () => {
    hoverState = null;
    hoverReadout?.classList.add("is-hidden");
    renderDock();
  });
}