/**
 * OpenDL - 60s Total Throughput Canvas Speed Graph
 * Crisp 1px hairline stroke in ink color, zero gradients, transparent background.
 */

import { state } from "./state.js";

export function initSpeedGraph(canvasId = "speedGraphCanvas") {
  const canvas = document.getElementById(canvasId);
  if (!canvas) return;

  const ctx = canvas.getContext("2d");

  function render() {
    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    const w = rect.width;
    const h = rect.height;

    if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
      canvas.width = Math.round(w * dpr);
      canvas.height = Math.round(h * dpr);
    }

    ctx.save();
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, w, h);

    const history = state.speedHistory;
    const count = history.length;
    if (count < 2) {
      ctx.restore();
      return;
    }

    // Determine max speed for Y-scaling (at least 1 MB/s floor)
    let maxSpeed = 1024 * 1024;
    for (let i = 0; i < count; i++) {
      if (history[i] > maxSpeed) {
        maxSpeed = history[i];
      }
    }

    // Coordinates
    const points = [];
    const stepX = w / (count - 1);
    for (let i = 0; i < count; i++) {
      const x = i * stepX;
      const normalized = Math.min(1, Math.max(0, history[i] / maxSpeed));
      const y = h - (normalized * (h - 6)) - 3;
      points.push({ x, y });
    }

    // Determine ink stroke color based on theme
    const isDark = document.documentElement.getAttribute("data-theme") !== "light";
    const inkColor = isDark ? "#EDEDEF" : "#18181B";

    // Draw crisp line (no gradients)
    ctx.beginPath();
    ctx.moveTo(points[0].x, points[0].y);
    for (let i = 1; i < points.length; i++) {
      const xc = (points[i - 1].x + points[i].x) / 2;
      const yc = (points[i - 1].y + points[i].y) / 2;
      ctx.quadraticCurveTo(points[i - 1].x, points[i - 1].y, xc, yc);
    }
    ctx.lineTo(points[points.length - 1].x, points[points.length - 1].y);
    ctx.strokeStyle = inkColor;
    ctx.lineWidth = 1.2;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.stroke();

    ctx.restore();
  }

  // Subscribe to state updates
  state.subscribe((event) => {
    if (event === "progress_updated" || event === "theme_changed") {
      render();
    }
  });

  window.addEventListener("resize", render);
  render();
}
