import {
  store,
  getFilteredTasks,
  formatBytes,
  formatSpeedParts,
  formatEta,
  setDockCollapsed,
  notify,
  setColumnWidths
} from "./state.js";
import { getIcon } from "./icons.js";
import { pauseTask, resumeTask, openPath, showInFolder, reorderQueueTasks } from "./api.js";

const ROW_HEIGHT = 46;
const GROUP_HEADER_HEIGHT = 26;
const OVERSCAN = 6;

const domRowPool = new Map(); // key -> HTMLElement
let currentHeaderView = null;

function formatCompletedReceipt(task) {
  const parts = [];
  if (task.completedAt) {
    const d = new Date(task.completedAt);
    const now = new Date();
    const isToday = d.toDateString() === now.toDateString();
    const yest = new Date(now);
    yest.setDate(now.getDate() - 1);
    const isYesterday = d.toDateString() === yest.toDateString();
    const timeStr = d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
    if (isToday) parts.push(`Today, ${timeStr}`);
    else if (isYesterday) parts.push(`Yesterday, ${timeStr}`);
    else parts.push(`${d.toLocaleDateString([], { month: "short", day: "numeric" })}, ${timeStr}`);
  }
  if (task.size != null) parts.push(formatBytes(task.size));
  if (task.avgSpeed) {
    const sp = formatSpeedParts(task.avgSpeed);
    parts.push(`avg ${sp.value} ${sp.unit.replace("/S", "/s")}`);
  }
  return parts.join(" · ");
}

function getCompletedBucket(ts) {
  if (!ts) return "EARLIER";
  const d = new Date(ts);
  const now = new Date();
  if (d.toDateString() === now.toDateString()) return "TODAY";
  const yest = new Date(now);
  yest.setDate(now.getDate() - 1);
  if (d.toDateString() === yest.toDateString()) return "YESTERDAY";
  return "EARLIER";
}

function buildDisplayItems(tasks) {
  const items = [];
  let offset = 0;

  if (store.view === "completed") {
    const buckets = { TODAY: [], YESTERDAY: [], EARLIER: [] };
    tasks.forEach((t, i) => {
      buckets[getCompletedBucket(t.completedAt)].push({ task: t, index: i + 1 });
    });
    for (const label of ["TODAY", "YESTERDAY", "EARLIER"]) {
      const group = buckets[label];
      if (group.length === 0) continue;
      items.push({ type: "group", key: `grp-${label}`, label, top: offset, height: GROUP_HEADER_HEIGHT });
      offset += GROUP_HEADER_HEIGHT;
      for (const entry of group) {
        items.push({
          type: "row",
          key: entry.task.id,
          task: entry.task,
          index: entry.index,
          top: offset,
          height: ROW_HEIGHT
        });
        offset += ROW_HEIGHT;
      }
    }
  } else {
    tasks.forEach((task, idx) => {
      items.push({
        type: "row",
        key: task.id,
        task,
        index: idx + 1,
        queuePos: idx + 1,
        top: offset,
        height: ROW_HEIGHT
      });
      offset += ROW_HEIGHT;
    });
  }
  return { items, totalHeight: offset };
}

const DEFAULT_GRID_COLS = ["44px", "minmax(180px, 1.2fr)", "92px", "minmax(220px, 1.4fr)", "96px", "90px", "100px"];

function wireColumnResizing(tableEl, headerEl) {
  let activeCols = [...DEFAULT_GRID_COLS];

  try {
    const saved = JSON.parse(localStorage.getItem("opendl_col_widths_v2") || "[]");
    if (Array.isArray(saved) && saved.length === DEFAULT_GRID_COLS.length) {
      activeCols = saved;
      tableEl.style.setProperty("--grid-cols-all", activeCols.join(" "));
    }
  } catch (_) {}

  const resizers = headerEl.querySelectorAll(".col-resizer");
  resizers.forEach((resizer) => {
    resizer.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();

      const colIdx = parseInt(resizer.getAttribute("data-col-idx"), 10);
      const th = resizer.closest(".th-cell, .th-select-cell") || resizer.parentElement;
      const startX = e.clientX;
      const startWidth = th.getBoundingClientRect().width;

      resizer.classList.add("is-active");

      const onMouseMove = (moveEvent) => {
        const diff = moveEvent.clientX - startX;
        const newWidth = Math.max(50, Math.round(startWidth + diff));
        activeCols[colIdx] = `${newWidth}px`;
        tableEl.style.setProperty("--grid-cols-all", activeCols.join(" "));
      };

      const onMouseUp = () => {
        resizer.classList.remove("is-active");
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
        document.body.style.cursor = "";

        try {
          localStorage.setItem("opendl_col_widths_v2", JSON.stringify(activeCols));
        } catch (_) {}
      };

      document.body.style.cursor = "col-resize";
      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", onMouseUp);
    });
  });
}

function renderHeaderIfNeeded(tableEl, headerEl, tasks) {
  if (currentHeaderView !== store.view) {
    currentHeaderView = store.view;
    tableEl.setAttribute("data-view-cols", store.view);

    const cols = [
      { label: "#", cls: "th-select-cell" },
      { label: "NAME", cls: "th-cell" },
      { label: "SIZE", cls: "th-cell th-num" },
      { label: "PROGRESS", cls: "th-cell" },
      { label: "SPEED", cls: "th-cell th-num" },
      { label: "ETA", cls: "th-cell th-num" },
      { label: "CATEGORY", cls: "th-cell" }
    ];

    headerEl.innerHTML = cols
      .map((c, idx) => {
        const resizer = (idx > 0 && idx < cols.length - 1) ? `<div class="col-resizer" data-col-idx="${idx}"></div>` : "";
        if (idx === 0) {
          return `<div class="${c.cls}" role="columnheader"><input type="checkbox" class="header-checkbox" aria-label="Select all tasks" /></div>`;
        }
        return `<div class="${c.cls}" role="columnheader">${c.label}${resizer}</div>`;
      })
      .join("");

    wireColumnResizing(tableEl, headerEl);

    const cb = headerEl.querySelector(".header-checkbox");
    cb?.addEventListener("change", (e) => {
      const visible = getFilteredTasks();
      if (e.target.checked) {
        visible.forEach((t) => store.selectedIds.add(t.id));
      } else {
        store.selectedIds.clear();
      }
      notify("selection");
    });

    for (const el of domRowPool.values()) el.remove();
    domRowPool.clear();
  }

  const cb = headerEl.querySelector(".header-checkbox");
  if (cb) {
    const visibleCount = tasks.length;
    const selectedVisible = tasks.filter((t) => store.selectedIds.has(t.id)).length;
    cb.checked = visibleCount > 0 && selectedVisible === visibleCount;
    cb.indeterminate = selectedVisible > 0 && selectedVisible < visibleCount;
  }
  tableEl.classList.toggle("has-selection", store.selectedIds.size > 0);
}

function drawMiniRowSparkline(canvas, samples) {
  if (!canvas) return;
  const ctx = canvas.getContext("2d");
  const w = canvas.width;
  const h = canvas.height;
  ctx.clearRect(0, 0, w, h);
  if (!samples || samples.length < 2) return;
  const max = Math.max(...samples, 1);
  const accent = getComputedStyle(document.documentElement).getPropertyValue("--accent-text").trim() || "#F59E0B";
  ctx.strokeStyle = accent;
  ctx.lineWidth = 1;
  ctx.beginPath();
  samples.forEach((v, i) => {
    const x = (i / (samples.length - 1)) * (w - 2) + 1;
    const y = h - 2 - (v / max) * (h - 4);
    if (i === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  });
  ctx.stroke();
}

function createRowElement(view) {
  const row = document.createElement("div");
  row.className = "task-row reg-panel";
  row.setAttribute("role", "row");

  const regMarks = `
    <span class="reg-mark reg-tl is-hidden" aria-hidden="true"></span>
    <span class="reg-mark reg-tr is-hidden" aria-hidden="true"></span>
    <span class="reg-mark reg-bl is-hidden" aria-hidden="true"></span>
    <span class="reg-mark reg-br is-hidden" aria-hidden="true"></span>
  `;

  const indexCol = `
    <div class="row-index-cell" role="gridcell">
      <span class="row-index-num">001</span>
      <input type="checkbox" class="row-checkbox" aria-label="Select task" />
    </div>
  `;

  const nameCol = `
    <div class="td-name-cell" role="gridcell">
      <div class="name-primary-line">
        <span class="finished-check is-hidden">${getIcon("check")}</span>
        <span class="task-name-text"></span>
        <div class="row-inline-actions"></div>
      </div>
      <div class="task-subline is-hidden"></div>
    </div>
  `;

  const progressCol = `
    <div class="td-progress-cell" role="gridcell">
      <div class="progress-top-line">
        <span class="state-badge" data-state="idle">
          <span class="status-led" data-state="idle"></span>
          <span class="state-word">IDLE</span>
        </span>
        <div class="progress-telemetry-meta">
          <canvas class="row-sparkline is-hidden" width="40" height="14"></canvas>
          <span class="progress-pct"></span>
          <span class="progress-bytes"></span>
          <span class="progress-eta-inline"></span>
        </div>
      </div>
      <div class="seg-bar-strip"></div>
    </div>
  `;

  row.innerHTML = `${regMarks}${indexCol}${nameCol}
    <div class="td-num cell-size" role="gridcell"></div>
    ${progressCol}
    <div class="td-num cell-speed" role="gridcell"></div>
    <div class="td-num cell-eta" role="gridcell"></div>
    <div class="th-cell cell-cat" role="gridcell"></div>`;

  return row;
}

function patchProgressCell(row, task) {
  const progCell = row.querySelector(".td-progress-cell");
  if (!progCell) return;

  if (task.status === "finished") {
    progCell.innerHTML = `<div class="progress-top-line"><span class="mono-val">100%</span><span class="micro-label">${task.size != null ? formatBytes(task.size) : ""}</span></div>`;
    progCell.dataset.renderedMode = "finished";
    return;
  }

  if (progCell.dataset.renderedMode === "finished") {
    progCell.innerHTML = `
      <div class="progress-top-line">
        <span class="state-badge" data-state="idle">
          <span class="status-led" data-state="idle"></span>
          <span class="state-word">IDLE</span>
        </span>
        <div class="progress-telemetry-meta">
          <canvas class="row-sparkline is-hidden" width="40" height="14"></canvas>
          <span class="progress-pct"></span>
          <span class="progress-bytes"></span>
          <span class="progress-eta-inline"></span>
        </div>
      </div>
      <div class="seg-bar-strip"></div>
    `;
    progCell.dataset.renderedMode = "live";
  }

  const badge = progCell.querySelector(".state-badge");
  const led = progCell.querySelector(".status-led");
  const word = progCell.querySelector(".state-word");
  const spark = progCell.querySelector(".row-sparkline");
  const pctEl = progCell.querySelector(".progress-pct");
  const bytesEl = progCell.querySelector(".progress-bytes");
  const etaInlineEl = progCell.querySelector(".progress-eta-inline");
  const strip = progCell.querySelector(".seg-bar-strip");

  const pct = task.size ? Math.min(100, Math.floor((task.downloaded / task.size) * 100)) : 0;
  badge.setAttribute("data-state", task.status);
  led.setAttribute("data-state", task.status);

  if (task.status === "downloading") {
    word.textContent = "DOWNLOADING";
    spark.classList.remove("is-hidden");
    drawMiniRowSparkline(spark, store.taskSparklines.get(task.id));
    pctEl.textContent = `${pct}%`;
    bytesEl.textContent = task.size ? `${formatBytes(task.downloaded)} / ${formatBytes(task.size)}` : formatBytes(task.downloaded);
    etaInlineEl.textContent = ""; // Clear inline ETA to prevent collision with SPEED column
  } else if (task.status === "queued") {
    word.textContent = `WAITING IN ${(task.queue || "MAIN QUEUE").toUpperCase()}`;
    spark.classList.add("is-hidden");
    pctEl.textContent = "";
    bytesEl.textContent = "";
    etaInlineEl.textContent = "";
  } else if (task.status === "paused") {
    word.textContent = `PAUSED AT ${pct}%`;
    spark.classList.add("is-hidden");
    pctEl.textContent = "";
    bytesEl.textContent = task.size ? `${formatBytes(task.downloaded)} / ${formatBytes(task.size)}` : "";
    etaInlineEl.textContent = "";
  } else if (task.status === "failed") {
    word.textContent = "FAILED";
    spark.classList.add("is-hidden");
    pctEl.textContent = `${pct}%`;
    bytesEl.textContent = "";
    etaInlineEl.textContent = "";
  }

  const segCount = task.segments && task.segments.length > 0 ? task.segments.length : 1;
  if (strip.children.length !== segCount) {
    strip.innerHTML = new Array(segCount)
      .fill(0)
      .map(() => `<div class="seg-cell hatched-surface"><div class="seg-cell-fill"></div></div>`)
      .join("");
  }

  if (task.segments && task.segments.length > 0) {
    task.segments.forEach((seg, i) => {
      const cellFill = strip.children[i]?.firstElementChild;
      if (!cellFill) return;
      const len = Math.max(1, seg.end - seg.start);
      const fillPct = task.status === "queued" ? 0 : Math.min(100, (seg.downloaded / len) * 100);
      cellFill.style.width = `${fillPct.toFixed(1)}%`;
      cellFill.classList.toggle("is-active", Boolean(seg.active && task.status === "downloading"));
      cellFill.classList.toggle("is-paused", task.status === "paused");
    });
  } else {
    const cellFill = strip.children[0]?.firstElementChild;
    if (cellFill) {
      const fillPct = task.status === "queued" ? 0 : pct;
      cellFill.style.width = `${fillPct}%`;
      cellFill.classList.toggle("is-active", task.status === "downloading");
      cellFill.classList.toggle("is-paused", task.status === "paused");
    }
  }
}

function patchRowElement(row, item) {
  const { task, index, queuePos, top } = item;
  row.style.transform = `translateY(${top}px)`;
  row.dataset.taskId = task.id;
  row.draggable = true;

  const isSelected = store.selectedIds.has(task.id);
  row.classList.toggle("is-selected", isSelected);
  row.setAttribute("aria-selected", String(isSelected));
  row.querySelectorAll(".reg-mark").forEach((m) => m.classList.toggle("is-hidden", !isSelected));

  const idxNum = row.querySelector(".row-index-num");
  if (idxNum) idxNum.textContent = String(index).padStart(3, "0");
  const cb = row.querySelector(".row-checkbox");
  if (cb && cb.checked !== isSelected) cb.checked = isSelected;

  const nameText = row.querySelector(".task-name-text");
  if (nameText && nameText.textContent !== task.name) nameText.textContent = task.name || "Unnamed task";

  const checkEl = row.querySelector(".finished-check");
  if (checkEl) checkEl.classList.toggle("is-hidden", task.status !== "finished");

  const subline = row.querySelector(".task-subline");
  if (subline) {
    if (task.status === "finished") {
      const receipt = formatCompletedReceipt(task);
      subline.textContent = receipt;
      subline.classList.toggle("is-hidden", !receipt);
      subline.classList.remove("is-error");
    } else if (task.status === "failed") {
      subline.textContent = task.error || "Download failed";
      subline.classList.remove("is-hidden");
      subline.classList.add("is-error");
    } else {
      subline.classList.add("is-hidden");
      subline.classList.remove("is-error");
    }
  }

  const actionsEl = row.querySelector(".row-inline-actions");
  if (actionsEl && actionsEl.dataset.stateActions !== task.status) {
    actionsEl.dataset.stateActions = task.status;
    if (task.status === "finished") {
      actionsEl.innerHTML = `
        <button type="button" class="btn btn-ghost btn-xs" data-row-action="open">Open</button>
        <button type="button" class="btn btn-ghost btn-xs" data-row-action="folder">Show in folder</button>
      `;
    } else if (task.status === "failed") {
      actionsEl.innerHTML = `<button type="button" class="btn btn-ghost btn-xs" data-row-action="retry">Retry</button>`;
    } else {
      actionsEl.innerHTML = "";
    }
  }

  const sizeEl = row.querySelector(".cell-size");
  if (sizeEl) sizeEl.textContent = task.size != null ? formatBytes(task.size) : "--";

  const speedEl = row.querySelector(".cell-speed");
  if (speedEl) {
    if (task.status === "downloading" && task.speed > 0) {
      const sp = formatSpeedParts(task.speed);
      speedEl.textContent = `${sp.value} ${sp.unit}`;
    } else if (task.isSeeding && (task.uploadSpeed || 0) > 0) {
      const sp = formatSpeedParts(task.uploadSpeed);
      speedEl.textContent = `up ${sp.value} ${sp.unit}`;
    } else if (task.status === "finished" && task.avgSpeed) {
      const sp = formatSpeedParts(task.avgSpeed);
      speedEl.textContent = `avg ${sp.value} ${sp.unit.replace("/S", "/s")}`;
    } else {
      speedEl.textContent = "--";
    }
  }

  const etaEl = row.querySelector(".cell-eta");
  if (etaEl) {
    if (task.status === "downloading") {
      etaEl.textContent = task.speed > 0 && task.eta != null ? formatEta(task.eta) : "--";
    } else if (task.status === "finished") {
      etaEl.textContent = task.completedAt
        ? new Date(task.completedAt).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })
        : "Done";
    } else if (task.status === "paused") {
      etaEl.textContent = "Paused";
    } else if (task.status === "queued") {
      etaEl.textContent = `#${String(queuePos || index).padStart(2, "0")}`;
    } else {
      etaEl.textContent = "--";
    }
  }

  const catEl = row.querySelector(".cell-cat");
  if (catEl) catEl.textContent = task.category || "General";

  const finEl = row.querySelector(".cell-finished-at");
  if (finEl) {
    finEl.textContent = task.completedAt
      ? new Date(task.completedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
      : "--";
  }

  const qEl = row.querySelector(".cell-queue");
  if (qEl) qEl.textContent = task.queue || "Main Queue";

  const posEl = row.querySelector(".cell-pos");
  if (posEl) posEl.textContent = `#${String(queuePos || index).padStart(2, "0")}`;

  patchProgressCell(row, task);
}

export function renderTaskTable() {
  const tableEl = document.getElementById("task-table");
  const headerEl = document.getElementById("table-header");
  const viewportEl = document.getElementById("table-viewport");
  const spacerEl = document.getElementById("table-spacer");
  const rowsHost = document.getElementById("table-rows");
  const emptyEl = document.getElementById("empty-state");

  if (!tableEl || !headerEl || !viewportEl || !spacerEl || !rowsHost) return;

  const tasks = getFilteredTasks();
  renderHeaderIfNeeded(tableEl, headerEl, tasks);

  if (tasks.length === 0) {
    spacerEl.style.height = "0px";
    rowsHost.innerHTML = "";
    domRowPool.clear();
    emptyEl.classList.remove("is-hidden");
    return;
  }

  emptyEl.classList.add("is-hidden");
  const { items, totalHeight } = buildDisplayItems(tasks);
  spacerEl.style.height = `${totalHeight}px`;

  const scrollTop = viewportEl.scrollTop;
  const viewHeight = viewportEl.clientHeight || 500;
  const minY = Math.max(0, scrollTop - OVERSCAN * ROW_HEIGHT);
  const maxY = scrollTop + viewHeight + OVERSCAN * ROW_HEIGHT;

  const activeKeys = new Set();

  for (const item of items) {
    if (item.top + item.height < minY || item.top > maxY) continue;
    activeKeys.add(item.key);
    let el = domRowPool.get(item.key);

    if (item.type === "group") {
      if (!el) {
        el = document.createElement("div");
        el.className = "group-header-row";
        domRowPool.set(item.key, el);
        rowsHost.appendChild(el);
      }
      el.style.transform = `translateY(${item.top}px)`;
      el.textContent = item.label;
    } else {
      if (!el) {
        el = createRowElement(store.view);
        domRowPool.set(item.key, el);
        rowsHost.appendChild(el);
      }
      patchRowElement(el, item);
    }
  }

  for (const [key, el] of domRowPool.entries()) {
    if (!activeKeys.has(key)) {
      el.remove();
      domRowPool.delete(key);
    }
  }
}

export function initTableInteractions(onContextMenu) {
  const viewportEl = document.getElementById("table-viewport");
  const rowsHost = document.getElementById("table-rows");
  const tableEl = document.getElementById("task-table");

  let draggedTaskId = null;

  rowsHost?.addEventListener("dragstart", (e) => {
    const rowEl = e.target.closest(".task-row");
    if (!rowEl) return;
    draggedTaskId = rowEl.dataset.taskId;
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", draggedTaskId);
    rowEl.classList.add("is-dragging");
  });

  rowsHost?.addEventListener("dragend", () => {
    draggedTaskId = null;
    rowsHost.querySelectorAll(".task-row").forEach((r) => {
      r.classList.remove("is-dragging", "is-drag-over-top", "is-drag-over-bottom");
    });
  });

  rowsHost?.addEventListener("dragover", (e) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    const rowEl = e.target.closest(".task-row");
    if (!rowEl || rowEl.dataset.taskId === draggedTaskId) return;

    const rect = rowEl.getBoundingClientRect();
    const midY = rect.top + rect.height / 2;
    if (e.clientY < midY) {
      rowEl.classList.add("is-drag-over-top");
      rowEl.classList.remove("is-drag-over-bottom");
    } else {
      rowEl.classList.add("is-drag-over-bottom");
      rowEl.classList.remove("is-drag-over-top");
    }
  });

  rowsHost?.addEventListener("dragleave", (e) => {
    const rowEl = e.target.closest(".task-row");
    if (!rowEl) return;
    rowEl.classList.remove("is-drag-over-top", "is-drag-over-bottom");
  });

  rowsHost?.addEventListener("drop", async (e) => {
    e.preventDefault();
    const rowEl = e.target.closest(".task-row");
    if (!rowEl) return;
    const targetTaskId = rowEl.dataset.taskId;
    const sourceTaskId = draggedTaskId || e.dataTransfer.getData("text/plain");

    rowsHost.querySelectorAll(".task-row").forEach((r) => {
      r.classList.remove("is-dragging", "is-drag-over-top", "is-drag-over-bottom");
    });

    if (!sourceTaskId || !targetTaskId || sourceTaskId === targetTaskId) return;

    const tasks = getFilteredTasks();
    const fromIdx = tasks.findIndex((t) => t.id === sourceTaskId);
    const toIdx = tasks.findIndex((t) => t.id === targetTaskId);
    if (fromIdx === -1 || toIdx === -1) return;

    const rect = rowEl.getBoundingClientRect();
    const midY = rect.top + rect.height / 2;
    const placeAfter = e.clientY >= midY;

    const [moved] = tasks.splice(fromIdx, 1);
    const insertIdx = placeAfter ? (fromIdx < toIdx ? toIdx : toIdx + 1) : (fromIdx < toIdx ? toIdx - 1 : toIdx);
    tasks.splice(Math.max(0, Math.min(tasks.length, insertIdx)), 0, moved);

    const orderedIds = tasks.map((t) => t.id);
    store.tasks.sort((a, b) => {
      const idxA = orderedIds.indexOf(a.id);
      const idxB = orderedIds.indexOf(b.id);
      if (idxA !== -1 && idxB !== -1) return idxA - idxB;
      return 0;
    });

    const queueName = moved.queue || "Main Queue";
    const queueTasks = store.tasks.filter((t) => t.queue === queueName);
    const queueTaskIds = queueTasks.map((t) => t.id);

    try {
      await reorderQueueTasks(queueName, queueTaskIds);
    } catch (err) {
      console.warn("reorderQueueTasks error:", err);
    }

    notify("tasks");
  });

  viewportEl?.addEventListener("scroll", () => {
    renderTaskTable();
  }, { passive: true });

  rowsHost?.addEventListener("click", async (e) => {
    const actionBtn = e.target.closest("[data-row-action]");
    const rowEl = e.target.closest(".task-row");
    if (!rowEl) return;
    const taskId = rowEl.dataset.taskId;
    if (!taskId) return;

    if (actionBtn) {
      e.stopPropagation();
      const act = actionBtn.getAttribute("data-row-action");
      if (act === "retry") await resumeTask(taskId);
      else if (act === "open") {
        const t = store.taskMap.get(taskId);
        if (t?.destination) await openPath(t.destination);
      } else if (act === "folder") {
        const t = store.taskMap.get(taskId);
        if (t?.destination) await showInFolder(t.destination);
      }
      return;
    }

    const cb = e.target.closest(".row-checkbox");
    const visible = getFilteredTasks();

    if (e.shiftKey && store.lastSelectedId) {
      const idxA = visible.findIndex((t) => t.id === store.lastSelectedId);
      const idxB = visible.findIndex((t) => t.id === taskId);
      if (idxA !== -1 && idxB !== -1) {
        const [start, end] = idxA < idxB ? [idxA, idxB] : [idxB, idxA];
        if (!e.ctrlKey && !e.metaKey) store.selectedIds.clear();
        for (let i = start; i <= end; i++) store.selectedIds.add(visible[i].id);
      }
    } else if (cb || e.ctrlKey || e.metaKey) {
      if (store.selectedIds.has(taskId)) store.selectedIds.delete(taskId);
      else store.selectedIds.add(taskId);
      store.lastSelectedId = taskId;
    } else {
      store.selectedIds.clear();
      store.selectedIds.add(taskId);
      store.lastSelectedId = taskId;
      if (store.dockCollapsed) setDockCollapsed(false);
    }

    store.focusedRowId = taskId;
    notify("selection");
  });

  rowsHost?.addEventListener("contextmenu", (e) => {
    const rowEl = e.target.closest(".task-row");
    if (!rowEl) return;
    e.preventDefault();
    const taskId = rowEl.dataset.taskId;
    if (!store.selectedIds.has(taskId)) {
      store.selectedIds.clear();
      store.selectedIds.add(taskId);
      store.lastSelectedId = taskId;
      notify("selection");
    }
    onContextMenu?.(e.clientX, e.clientY, taskId);
  });

  rowsHost?.addEventListener("dblclick", async (e) => {
    const rowEl = e.target.closest(".task-row");
    if (!rowEl) return;
    const taskId = rowEl.dataset.taskId;
    const task = store.taskMap.get(taskId);
    if (!task) return;
    if (task.status === "finished" && task.destination) {
      await openPath(task.destination);
    } else if (task.status === "downloading") {
      await pauseTask(taskId);
    } else if (task.status === "paused" || task.status === "failed" || task.status === "queued") {
      await resumeTask(taskId);
    }
  });

  tableEl?.addEventListener("keydown", async (e) => {
    const visible = getFilteredTasks();
    if (visible.length === 0) return;

    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const curIdx = visible.findIndex((t) => t.id === (store.focusedRowId || store.lastSelectedId));
      const nextIdx = e.key === "ArrowDown"
        ? Math.min(visible.length - 1, curIdx + 1)
        : Math.max(0, curIdx <= 0 ? 0 : curIdx - 1);
      const target = visible[nextIdx];
      if (target) {
        if (!e.shiftKey) store.selectedIds.clear();
        store.selectedIds.add(target.id);
        store.lastSelectedId = target.id;
        store.focusedRowId = target.id;
        notify("selection");
      }
    } else if (e.key === " ") {
      e.preventDefault();
      for (const id of store.selectedIds) {
        const t = store.taskMap.get(id);
        if (!t) continue;
        if (t.status === "downloading") await pauseTask(id);
        else if (t.status === "paused" || t.status === "failed" || t.status === "queued") await resumeTask(id);
      }
    }
  });
}