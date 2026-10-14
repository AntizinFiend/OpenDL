/**
 * OpenDL - Technical Segment Map Visualizer
 * Renders like a technical diagram:
 * - Byte-offset ruler with tick marks along the top
 * - Hairline lanes labeled "SEG 01/08" in uppercase mono
 * - Per-lane speed in mono
 * - Ink fill for downloaded ranges
 * - 2px signal (#FF4F1F) leading edge
 * Real data strictly from get_all_progress.segments.
 */

import { formatBytes } from "./table.js";

export class SegmentMapVisualizer {
  constructor(canvasId = "segmentMapCanvas") {
    this.canvas = document.getElementById(canvasId);
    this.ctx = this.canvas ? this.canvas.getContext("2d") : null;
    this.animationId = null;
    this.currentTaskId = null;
    this.targetSegments = [];
    this.interpolatedSegments = [];
    this.totalBytes = 0;
    this.task = null;
    this.prog = null;

    if (this.canvas) {
      window.addEventListener("resize", () => this.resizeCanvas());
      this.resizeCanvas();
    }
  }

  resizeCanvas() {
    if (!this.canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const rect = this.canvas.getBoundingClientRect();
    const w = rect.width || 600;
    const h = rect.height || 100;

    this.canvas.width = Math.round(w * dpr);
    this.canvas.height = Math.round(h * dpr);
    if (this.ctx) {
      this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    }
  }

  setTask(taskId) {
    this.currentTaskId = taskId;
    this.targetSegments = [];
    this.interpolatedSegments = [];
    if (!taskId) {
      this.stop();
      this.clear();
      return;
    }
    this.start();
  }

  start() {
    if (this.animationId) return;
    const loop = () => {
      this.render();
      this.animationId = requestAnimationFrame(loop);
    };
    this.animationId = requestAnimationFrame(loop);
  }

  stop() {
    if (this.animationId) {
      cancelAnimationFrame(this.animationId);
      this.animationId = null;
    }
  }

  clear() {
    if (!this.ctx || !this.canvas) return;
    const rect = this.canvas.getBoundingClientRect();
    this.ctx.clearRect(0, 0, rect.width, rect.height);
  }

  updateData(task, prog) {
    if (!task) return;
    const total = (prog && prog.total_bytes) ? prog.total_bytes : task.total_size;
    const rawSegments = (prog && prog.segments && prog.segments.length > 0) ? prog.segments : [];

    this.totalBytes = total;
    this.task = task;
    this.prog = prog;

    if (rawSegments.length > 0) {
      this.targetSegments = rawSegments.map(s => ({
        id: s.id,
        start: s.start,
        current: s.current,
        end: s.end,
        is_active: s.is_active,
        is_completed: s.is_completed
      }));

      if (this.interpolatedSegments.length !== this.targetSegments.length) {
        this.interpolatedSegments = this.targetSegments.map(s => ({ ...s, interpCurrent: s.current }));
      } else {
        for (let i = 0; i < this.targetSegments.length; i++) {
          const t = this.targetSegments[i];
          const curr = this.interpolatedSegments[i];
          curr.is_active = t.is_active;
          curr.is_completed = t.is_completed;
          curr.start = t.start;
          curr.end = t.end;
          curr.current = t.current;
        }
      }
    } else {
      this.targetSegments = [];
      this.interpolatedSegments = [];
    }
  }

  render() {
    if (!this.ctx || !this.canvas) return;
    const rect = this.canvas.getBoundingClientRect();
    const w = rect.width;
    const h = rect.height;

    this.ctx.clearRect(0, 0, w, h);

    const isDark = document.documentElement.getAttribute("data-theme") !== "light";
    const inkColor = isDark ? "#EDEDEF" : "#18181B";
    const textSecColor = isDark ? "#8C8C94" : "#6A6A72";
    const borderColor = isDark ? "#26262A" : "#E4E4E7";
    const trackColor = isDark ? "#2A2A2E" : "#E8E8EB";
    const signalColor = "#FF4F1F"; // 2px signal leading edge only

    if (!this.task) {
      this.ctx.fillStyle = textSecColor;
      this.ctx.font = '11px "JetBrains Mono", monospace';
      this.ctx.textAlign = "center";
      this.ctx.fillText("SELECT A TASK TO INSPECT TECHNICAL SEGMENT DIAGRAM", w / 2, h / 2 + 4);
      return;
    }

    const total = this.totalBytes;
    const segments = this.interpolatedSegments;

    // Smooth easing towards latest current byte offset
    for (const seg of segments) {
      if (seg.interpCurrent < seg.current) {
        seg.interpCurrent += (seg.current - seg.interpCurrent) * 0.18;
      } else {
        seg.interpCurrent = seg.current;
      }
    }

    const laneStartX = 72;
    const laneEndX = w - 74;
    const laneWidth = Math.max(10, laneEndX - laneStartX);

    // -------------------------------------------------------------
    // 1. Technical Byte-Offset Ruler along the Top
    // -------------------------------------------------------------
    const rulerY = 14;
    this.ctx.strokeStyle = borderColor;
    this.ctx.lineWidth = 1;
    this.ctx.beginPath();
    this.ctx.moveTo(laneStartX, rulerY);
    this.ctx.lineTo(laneEndX, rulerY);
    this.ctx.stroke();

    // Ruler tick marks: 0%, 25%, 50%, 75%, 100%
    const ticks = [0, 0.25, 0.5, 0.75, 1];
    this.ctx.font = '9px "JetBrains Mono", monospace';
    this.ctx.fillStyle = textSecColor;

    for (const t of ticks) {
      const tx = Math.round(laneStartX + t * laneWidth);

      // Major tick mark
      this.ctx.beginPath();
      this.ctx.moveTo(tx, rulerY - 4);
      this.ctx.lineTo(tx, rulerY);
      this.ctx.stroke();

      // Tick label
      let label = "0 B";
      if (total && total > 0) {
        label = formatBytes(Math.round(total * t));
      } else if (t === 1) {
        label = "END";
      }

      this.ctx.textAlign = (t === 0) ? "left" : (t === 1 ? "right" : "center");
      this.ctx.fillText(label, tx, rulerY - 6);
    }

    // Minor intermediate ticks at 12.5%, 37.5%, 62.5%, 87.5%
    const minorTicks = [0.125, 0.375, 0.625, 0.875];
    for (const mt of minorTicks) {
      const mtx = Math.round(laneStartX + mt * laneWidth);
      this.ctx.beginPath();
      this.ctx.moveTo(mtx, rulerY - 2);
      this.ctx.lineTo(mtx, rulerY);
      this.ctx.stroke();
    }

    // -------------------------------------------------------------
    // 2. Technical Connection Lanes
    // -------------------------------------------------------------
    const startY = 20;
    const totalSlots = segments.length > 0 ? Math.min(segments.length, 8) : 1;
    const availableHeight = h - startY - 2;
    const laneSlotH = Math.max(8, Math.floor(availableHeight / totalSlots));
    const laneBarH = Math.max(4, laneSlotH - 3);

    if (segments.length > 0 && total && total > 0) {
      for (let i = 0; i < totalSlots; i++) {
        const seg = segments[i];
        const y = startY + i * laneSlotH;
        const totalPad = String(segments.length).padStart(2, "0");
        const segNumPad = String(seg.id + 1).padStart(2, "0");

        // A. Lane Label: "SEG 01/04" in mono uppercase
        this.ctx.font = '10px "JetBrains Mono", monospace';
        this.ctx.fillStyle = textSecColor;
        this.ctx.textAlign = "left";
        this.ctx.fillText(`SEG ${segNumPad}/${totalPad}`, 4, y + laneBarH);

        // B. Hairline Track
        this.ctx.fillStyle = trackColor;
        this.ctx.fillRect(laneStartX, y, laneWidth, laneBarH);

        // C. Segment Assigned Boundary
        const segStartX = laneStartX + (seg.start / total) * laneWidth;
        const segEndX = laneStartX + (seg.end / total) * laneWidth;
        const segBlockW = Math.max(1, segEndX - segStartX);

        // Hairline boundary
        this.ctx.strokeStyle = borderColor;
        this.ctx.strokeRect(segStartX + 0.5, y + 0.5, Math.max(0, segBlockW - 1), laneBarH - 1);

        // D. Downloaded portion (INK FILL)
        const currX = laneStartX + (Math.max(seg.start, Math.min(seg.end, seg.interpCurrent)) / total) * laneWidth;
        const filledW = Math.max(0, currX - segStartX);

        this.ctx.fillStyle = seg.is_completed ? (isDark ? "#3A3A40" : "#D4D4D8") : inkColor;
        this.ctx.fillRect(segStartX, y, Math.min(segBlockW, filledW), laneBarH);

        // E. Leading edge: 2px SIGNAL (#FF4F1F) ONLY when active
        if (seg.is_active && !seg.is_completed) {
          this.ctx.fillStyle = signalColor;
          this.ctx.fillRect(Math.min(segEndX - 2, currX - 1), y, 2, laneBarH);
        }

        // F. Per-lane speed / status label on right in mono
        this.ctx.font = '10px "JetBrains Mono", monospace';
        this.ctx.textAlign = "right";

        if (seg.is_completed) {
          this.ctx.fillStyle = textSecColor;
          this.ctx.fillText("DONE", w - 4, y + laneBarH);
        } else if (seg.is_active) {
          const perLaneSpeed = this.prog && this.prog.speed_bytes_per_sec
            ? (this.prog.speed_bytes_per_sec / Math.max(1, segments.filter(s => s.is_active).length))
            : 0;
          this.ctx.fillStyle = inkColor;
          this.ctx.fillText(perLaneSpeed > 0 ? `${formatBytes(perLaneSpeed)}/s` : "ACTIVE", w - 4, y + laneBarH);
        } else {
          this.ctx.fillStyle = textSecColor;
          this.ctx.fillText("IDLE", w - 4, y + laneBarH);
        }
      }
    } else {
      // Single stream fallback
      const downloaded = (this.prog ? this.prog.downloaded_bytes : this.task.downloaded_bytes) || 0;
      const pct = total ? Math.min(1, downloaded / total) : (this.task.status === "completed" ? 1 : 0);
      const isComplete = this.task.status === "completed" || (this.prog && this.prog.is_complete);

      const y = startY + 6;
      this.ctx.font = '10px "JetBrains Mono", monospace';
      this.ctx.fillStyle = textSecColor;
      this.ctx.textAlign = "left";
      this.ctx.fillText("STREAM 01", 4, y + 10);

      // Track
      this.ctx.fillStyle = trackColor;
      this.ctx.fillRect(laneStartX, y, laneWidth, 12);
      this.ctx.strokeStyle = borderColor;
      this.ctx.strokeRect(laneStartX + 0.5, y + 0.5, laneWidth - 1, 11);

      // Fill
      const fillW = laneWidth * pct;
      this.ctx.fillStyle = isComplete ? (isDark ? "#3A3A40" : "#D4D4D8") : inkColor;
      this.ctx.fillRect(laneStartX, y, fillW, 12);

      // 2px Signal leading edge
      if (pct > 0 && pct < 1 && this.task.status === "downloading") {
        this.ctx.fillStyle = signalColor;
        this.ctx.fillRect(Math.min(laneEndX - 2, laneStartX + fillW - 1), y, 2, 12);
      }

      this.ctx.textAlign = "right";
      this.ctx.fillStyle = isComplete ? textSecColor : inkColor;
      this.ctx.fillText(isComplete ? "DONE" : `${formatBytes(downloaded)}`, w - 4, y + 10);
    }
  }
}
