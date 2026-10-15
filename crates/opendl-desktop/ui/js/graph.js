// js/graph.js
import {
  store,
  getThroughputSeries,
  formatSpeedParts,
  setDockCollapsed,
  setDockTab,
  decodeTitleEffect,
  playPiezo
} from "./state.js";

let oscHoverX = null;
let previousCeiling = 0;
let rangeShiftTimer = null;

function updateRangeShiftBadge(newCeiling) {
  const badge = document.getElementById("tp-range-shift-badge");
  if (!badge) return;
  const curSp = formatSpeedParts(newCeiling);

  if (previousCeiling > 0 && Math.abs(newCeiling - previousCeiling) / previousCeiling > 0.15) {
    const oldSp = formatSpeedParts(previousCeiling);
    badge.textContent = `Scale: ${oldSp.value} ${oldSp.unit} -> ${curSp.value} ${curSp.unit}`;
    badge.classList.add("is-shifting");
    clearTimeout(rangeShiftTimer);
    rangeShiftTimer = setTimeout(() => {
      badge.classList.remove("is-shifting");
      badge.textContent = `Scale: ${curSp.value} ${curSp.unit}`;
    }, 900);
  } else if (!badge.classList.contains("is-shifting")) {
    badge.textContent = `Scale: ${curSp.value} ${curSp.unit}`;
  }
  previousCeiling = newCeiling;
}

function renderMiniSparkline(series) {
  const canvas = document.getElementById("mini-throughput-canvas");
  if (!canvas) return;
  const ctx = canvas.getContext("2d");
  const w = canvas.width;
  const h = canvas.height;
  ctx.clearRect(0, 0, w, h);

  const styles = getComputedStyle(document.documentElement);
  const accent = styles.getPropertyValue("--accent-text").trim() || "#F59E0B";
  const fill = styles.getPropertyValue("--accent-fill-graph").trim() || "rgba(245,158,11,0.14)";
  const border = styles.getPropertyValue("--border").trim() || "#282724";

  ctx.strokeStyle = border;
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(0, Math.round(h / 2) + 0.5);
  ctx.lineTo(w, Math.round(h / 2) + 0.5);
  ctx.stroke();

  if (!series || series.length < 2) return;
  const tail = series.slice(-60);
  const max = Math.max(...tail, 1024);

  ctx.beginPath();
  tail.forEach((v, idx) => {
    const x = (idx / (tail.length - 1)) * w;
    const y = h - 3 - (v / max) * (h - 8);
    if (idx === 0) ctx.moveTo(x, y);
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
}

function renderDockThroughputCanvas(series, windowSec, currentSpeedBytes) {
  const canvas = document.getElementById("throughput-canvas");
  const deltaEl = document.getElementById("dock-tp-delta");
  if (!canvas) return;
  const rect = canvas.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  const w = Math.max(240, Math.floor(rect.width));
  const h = Math.max(90, Math.floor(rect.height));
  if (canvas.width !== w * dpr || canvas.height !== h * dpr) {
    canvas.width = w * dpr;
    canvas.height = h * dpr;
  }

  const ctx = canvas.getContext("2d");
  ctx.save();
  ctx.scale(dpr, dpr);
  ctx.clearRect(0, 0, w, h);

  const styles = getComputedStyle(document.documentElement);
  const accent = styles.getPropertyValue("--accent-text").trim() || "#F59E0B";
  const fill = styles.getPropertyValue("--accent-fill-graph").trim() || "rgba(245,158,11,0.14)";
  const colText = styles.getPropertyValue("--text").trim() || "#EDEAE4";
  const colSec = styles.getPropertyValue("--text-secondary").trim() || "#8C877E";
  const colBorder = styles.getPropertyValue("--border").trim() || "#282724";
  const colSurface = styles.getPropertyValue("--surface").trim() || "#161614";
  const fontMono = styles.getPropertyValue("--font-mono").trim() || "monospace";

  const padLeft = 64;
  const padRight = 16;
  const padTop = 14;
  const padBottom = 22;
  const plotW = Math.max(80, w - padLeft - padRight);
  const plotH = Math.max(40, h - padTop - padBottom);

  const peak = series.length > 0 ? Math.max(...series) : 0;
  const ceiling = Math.max(peak * 1.15, 1024 * 1024);
  updateRangeShiftBadge(ceiling);

  ctx.font = `11px ${fontMono}`;
  ctx.strokeStyle = colBorder;
  ctx.fillStyle = colSec;
  ctx.lineWidth = 1;

  const hRows = 3;
  for (let r = 0; r <= hRows; r++) {
    const ratio = r / hRows;
    const y = Math.round(padTop + ratio * plotH) + 0.5;
    ctx.beginPath();
    ctx.moveTo(padLeft, y);
    ctx.lineTo(padLeft + plotW, y);
    ctx.stroke();

    const val = ceiling * (1 - ratio);
    const sp = formatSpeedParts(val);
    ctx.textAlign = "right";
    ctx.fillText(`${sp.value} ${sp.unit}`, padLeft - 6, y + 3);
  }

  const timeTicks = 4;
  for (let t = 0; t <= timeTicks; t++) {
    const ratio = t / timeTicks;
    const x = Math.round(padLeft + ratio * plotW) + 0.5;
    ctx.beginPath();
    ctx.moveTo(x, padTop);
    ctx.lineTo(x, padTop + plotH);
    ctx.stroke();

    const secAgo = Math.round(windowSec * (1 - ratio));
    const label = secAgo === 0 ? "NOW" : `-${secAgo}s`;
    ctx.textAlign = t === 0 ? "left" : (t === timeTicks ? "right" : "center");
    ctx.fillText(label, x, h - 6);
  }

  if (series.length >= 2) {
    let peakIdx = 0;
    let peakVal = -1;
    let sumVal = 0;

    ctx.beginPath();
    series.forEach((v, idx) => {
      sumVal += v;
      if (v > peakVal) {
        peakVal = v;
        peakIdx = idx;
      }
      const x = padLeft + (idx / (series.length - 1)) * plotW;
      const y = padTop + plotH - (v / ceiling) * plotH;
      if (idx === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.strokeStyle = accent;
    ctx.lineWidth = 1;
    ctx.stroke();

    ctx.lineTo(padLeft + plotW, padTop + plotH);
    ctx.lineTo(padLeft, padTop + plotH);
    ctx.closePath();
    ctx.fillStyle = fill;
    ctx.fill();

    // Average velocity trigger line
    const avgVal = sumVal / series.length;
    if (avgVal > 0) {
      const avgY = Math.round(padTop + plotH - (avgVal / ceiling) * plotH) + 0.5;
      ctx.save();
      ctx.setLineDash([3, 3]);
      ctx.strokeStyle = colSec;
      ctx.beginPath();
      ctx.moveTo(padLeft, avgY);
      ctx.lineTo(padLeft + plotW, avgY);
      ctx.stroke();
      ctx.restore();
    }

    if (peakVal > 0) {
      const px = padLeft + (peakIdx / (series.length - 1)) * plotW;
      const py = padTop + plotH - (peakVal / ceiling) * plotH;
      ctx.fillStyle = accent;
      ctx.fillRect(px - 2, py - 2, 4, 4);
      const pSp = formatSpeedParts(peakVal);
      ctx.textAlign = px > padLeft + plotW * 0.75 ? "right" : "left";
      ctx.fillText(`PEAK ${pSp.value} ${pSp.unit}`, px + (px > padLeft + plotW * 0.75 ? -6 : 6), Math.max(padTop + 10, py - 4));
    }

    // Interactive Oscilloscope Crosshairs
    if (oscHoverX !== null && oscHoverX >= padLeft && oscHoverX <= padLeft + plotW) {
      const ratio = (oscHoverX - padLeft) / plotW;
      const sampleIdx = Math.min(series.length - 1, Math.max(0, Math.round(ratio * (series.length - 1))));
      const sampleVal = series[sampleIdx] || 0;
      const snapX = Math.round(padLeft + (sampleIdx / (series.length - 1)) * plotW) + 0.5;
      const snapY = Math.round(padTop + plotH - (sampleVal / ceiling) * plotH) + 0.5;

      ctx.strokeStyle = accent;
      ctx.beginPath();
      ctx.moveTo(snapX, padTop);
      ctx.lineTo(snapX, padTop + plotH);
      ctx.moveTo(padLeft, snapY);
      ctx.lineTo(padLeft + plotW, snapY);
      ctx.stroke();

      ctx.fillStyle = colText;
      ctx.fillRect(snapX - 2.5, snapY - 2.5, 5, 5);

      const secAgo = ((1 - sampleIdx / (series.length - 1)) * windowSec).toFixed(1);
      const sSp = formatSpeedParts(sampleVal);
      let deltaStr = "0.0%";
      if (currentSpeedBytes > 0) {
        const diffPct = ((sampleVal - currentSpeedBytes) / currentSpeedBytes) * 100;
        deltaStr = `${diffPct >= 0 ? "+" : ""}${diffPct.toFixed(1)}% VS NOW`;
      }
      if (deltaEl) deltaEl.textContent = `${sSp.value} ${sSp.unit} (${deltaStr})`;

      const hudText = `-${secAgo}s // ${sSp.value} ${sSp.unit} // ${deltaStr}`;
      const textW = ctx.measureText(hudText).width + 12;
      const boxX = Math.min(padLeft + plotW - textW, Math.max(padLeft + 4, snapX + 8));
      const boxY = Math.max(padTop + 4, snapY - 22);

      ctx.fillStyle = colSurface;
      ctx.fillRect(boxX, boxY, textW, 18);
      ctx.strokeStyle = accent;
      ctx.strokeRect(boxX + 0.5, boxY + 0.5, textW - 1, 17);
      ctx.fillStyle = colText;
      ctx.textAlign = "left";
      ctx.fillText(hudText, boxX + 6, boxY + 13);
    } else if (deltaEl) {
      deltaEl.textContent = "--";
    }
  }

  ctx.restore();
}

export function renderGraphs(currentSpeedBytes = 0) {
  const sp = formatSpeedParts(currentSpeedBytes);
  const activeAny = store.tasks.some((t) => t.status === "downloading");

  const widgetVal = document.getElementById("widget-speed-val");
  const widgetUnit = document.getElementById("widget-speed-unit");
  const widgetLed = document.getElementById("widget-led");
  if (widgetVal) widgetVal.textContent = sp.value;
  if (widgetUnit) widgetUnit.textContent = sp.unit;
  if (widgetLed) widgetLed.setAttribute("data-state", activeAny ? "downloading" : "idle");

  const series = getThroughputSeries(store.throughputWindowSec);
  renderMiniSparkline(series);

  const dockCur = document.getElementById("dock-tp-current");
  const dockUnit = document.getElementById("dock-tp-unit");
  const dockPeak = document.getElementById("dock-tp-peak");
  if (dockCur) dockCur.textContent = sp.value;
  if (dockUnit) dockUnit.textContent = sp.unit;
  if (dockPeak) {
    const peakVal = series.length > 0 ? Math.max(...series) : 0;
    const pSp = formatSpeedParts(peakVal);
    dockPeak.textContent = `${pSp.value} ${pSp.unit}`;
  }

  if (!store.dockCollapsed && store.dockTab === "throughput") {
    renderDockThroughputCanvas(series, store.throughputWindowSec, currentSpeedBytes);
  }
}

export function initGraphInteractions() {
  const widget = document.getElementById("throughput-widget");
  const titleEl = document.getElementById("dock-title-decode");
  const tpCanvas = document.getElementById("throughput-canvas");

  widget?.addEventListener("click", () => {
    setDockCollapsed(false);
    setDockTab("throughput");
    playPiezo("snap");
    decodeTitleEffect(titleEl, "BUS THROUGHPUT");
  });

  document.querySelectorAll(".tp-range-btn").forEach((btn) => {
    btn.addEventListener("click", () => {
      document.querySelectorAll(".tp-range-btn").forEach((b) => b.classList.remove("is-active"));
      btn.classList.add("is-active");
      store.throughputWindowSec = Number(btn.getAttribute("data-window") || 60);
      playPiezo("tick");
      renderGraphs(store.tasks.reduce((sum, t) => sum + (t.speed || 0), 0));
    });
  });

  tpCanvas?.addEventListener("mousemove", (e) => {
    const rect = tpCanvas.getBoundingClientRect();
    oscHoverX = e.clientX - rect.left;
    renderGraphs(store.tasks.reduce((sum, t) => sum + (t.speed || 0), 0));
  });

  tpCanvas?.addEventListener("mouseleave", () => {
    oscHoverX = null;
    renderGraphs(store.tasks.reduce((sum, t) => sum + (t.speed || 0), 0));
  });
}