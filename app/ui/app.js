"use strict";

const { invoke } = window.platform;

const $ = (id) => document.getElementById(id);
const POINTS_PER = { in: 72, mm: 72 / 25.4 };
const CSS_PX_PER_POINT = 96 / 72;

const LENGTH_FIELDS = {
  "sheet-width": "sheetWidth",
  "sheet-height": "sheetHeight",
  margin: "margin",
  "item-width": "itemWidth",
  "item-height": "itemHeight",
  gutter: "gutter",
  "bleed-width": "bleedWidth",
  "mark-length": "markLength",
  "mark-offset": "markOffset",
};
const COUNT_FIELDS = { across: "across", down: "down" };

let state = null;
let sheet = 0;
let unit = load("unit", "in");
const thumbnails = new Map();

// ---------- small helpers ----------

function load(key, fallback) {
  try { return localStorage.getItem(key) ?? fallback; } catch { return fallback; }
}

function save(key, value) {
  try { localStorage.setItem(key, value); } catch { /* per-viewer convenience only */ }
}

function toDisplay(points) {
  const value = points / POINTS_PER[unit];
  const digits = unit === "in" ? 4 : 2;
  return String(Number(value.toFixed(digits)));
}

function toPoints(text) {
  const value = Number(String(text).trim().replace(",", "."));
  return Number.isFinite(value) ? value * POINTS_PER[unit] : NaN;
}

function toast(message, kind = "") {
  const el = document.createElement("div");
  el.className = `toast ${kind}`;
  el.textContent = message;
  $("toasts").append(el);
  setTimeout(() => el.remove(), kind === "error" ? 7000 : 4000);
}

function bitmapFromBuffer(buffer) {
  const view = new DataView(buffer);
  const width = view.getUint32(0, true);
  const height = view.getUint32(4, true);
  const pixels = new Uint8ClampedArray(buffer, 8, width * height * 4);
  return new ImageData(pixels, width, height);
}

async function call(command, args) {
  try {
    return await invoke(command, args);
  } catch (error) {
    toast(String(error), "error");
    throw error;
  }
}

async function apply(command, args) {
  try {
    state = await call(command, args);
  } finally {
    render();
  }
}

// ---------- settings ----------

function commit(partial) {
  return apply("update_settings", { settings: { ...state.settings, ...partial } });
}

for (const [id, key] of Object.entries(LENGTH_FIELDS)) {
  $(id).addEventListener("change", (e) => {
    const points = toPoints(e.target.value);
    if (!(points >= 0)) {
      e.target.value = toDisplay(state.settings[key]);
      return;
    }
    commit({ [key]: points });
  });
}

for (const [id, key] of Object.entries(COUNT_FIELDS)) {
  $(id).addEventListener("change", (e) => {
    const count = Number.parseInt(e.target.value, 10);
    if (!(count >= 1)) {
      e.target.value = state.settings[key];
      return;
    }
    commit({ [key]: count });
  });
}

for (const input of document.querySelectorAll(".settings input")) {
  input.addEventListener("focus", () => input.select());
  input.addEventListener("keydown", (e) => { if (e.key === "Enter") input.blur(); });
}

function segmented(id, onPick) {
  $(id).addEventListener("click", (e) => {
    const button = e.target.closest("button[data-value]");
    if (button) onPick(button.dataset.value);
  });
}

segmented("units", (value) => { unit = value; save("unit", value); render(); });
segmented("scaling", (value) => commit({ scaling: value }));
segmented("bleed-mode", (value) => commit({ bleedMode: value }));
segmented("mode", (value) => {
  if (value === state.settings.mode) return;
  // Each mode starts from what the other one currently produces.
  commit({ mode: value });
});

for (const [id, key] of [["mix", "mixOrientations"], ["crop-marks", "cropMarks"], ["trim-lines", "trimLines"]]) {
  $(id).addEventListener("click", () => commit({ [key]: !state.settings[key] }));
}

$("preset").addEventListener("change", (e) => {
  const preset = state.presets[Number(e.target.value)];
  if (preset) commit({ sheetWidth: preset.width, sheetHeight: preset.height });
});

// ---------- files ----------

$("add-files").addEventListener("click", importPicked);
$("drop-zone").addEventListener("click", importPicked);
$("clear-all").addEventListener("click", () => apply("clear_items"));

async function importPicked() {
  try {
    showImport(await platform.pickFiles());
  } catch (error) {
    toast(String(error), "error");
  }
}

function showImport(result) {
  state = result.state;
  render();
  for (const failure of result.failed) toast(failure, "error");
}

platform.onFilesArrive({
  hover: (on) => $("drop-veil").classList.toggle("on", on),
  arrived: (result) => result.then(showImport, (error) => toast(String(error), "error")),
});

const ICONS = {
  handle: '<svg width="8" height="14" viewBox="0 0 8 14" fill="currentColor"><circle cx="2" cy="2" r="1.2"/><circle cx="6" cy="2" r="1.2"/><circle cx="2" cy="7" r="1.2"/><circle cx="6" cy="7" r="1.2"/><circle cx="2" cy="12" r="1.2"/><circle cx="6" cy="12" r="1.2"/></svg>',
  split: '<svg width="14" height="14" viewBox="0 0 14 14"><rect x="1" y="1.5" width="5" height="11" rx="1" fill="none" stroke="currentColor" stroke-width="1.3"/><rect x="8" y="1.5" width="5" height="11" rx="1" fill="none" stroke="currentColor" stroke-width="1.3"/></svg>',
  remove: '<svg width="12" height="12" viewBox="0 0 12 12"><path d="M2.5 2.5l7 7M9.5 2.5l-7 7" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>',
};

function renderFiles() {
  const list = $("file-list");
  list.replaceChildren(...state.items.map(fileRow));
  document.querySelector(".files").classList.toggle("empty", state.items.length === 0);
  const pages = state.items.length;
  $("item-count").textContent = pages ? `${pages} ${pages === 1 ? "item" : "items"}` : "";
  $("clear-all").hidden = pages === 0;
}

function fileRow(item) {
  const row = document.createElement("li");
  row.className = "file-row";
  row.dataset.id = item.id;

  const badge = item.vector
    ? '<span class="badge">Vector</span>'
    : item.dpi != null
      ? `<span class="badge${item.dpi < 300 ? " low" : ""}">${Math.round(item.dpi)} dpi</span>`
      : "";
  row.innerHTML = `
    <span class="handle" title="Drag to reorder">${ICONS.handle}</span>
    <span class="thumb${item.isPack ? " pack" : ""}"><img alt="" /></span>
    <div class="meta">
      <div class="name"></div>
      <div class="spec-line">${badge}<span class="spec"></span></div>
    </div>
    ${item.isPack ? `<button class="split" title="Split pages">${ICONS.split}</button>` : ""}
    <input class="count${item.copies ? " set" : ""}" inputmode="numeric" aria-label="Copies" />
    <button class="remove" aria-label="Remove">${ICONS.remove}</button>`;
  row.querySelector(".name").textContent = item.name;
  row.querySelector(".name").title = item.name;
  row.querySelector(".spec").textContent = item.spec;

  const count = row.querySelector(".count");
  count.value = item.copies ?? "";
  count.addEventListener("input", () => count.classList.toggle("set", count.value.trim() !== ""));
  count.addEventListener("keydown", (e) => { if (e.key === "Enter") count.blur(); });
  count.addEventListener("change", () => {
    const text = count.value.trim();
    const copies = text === "" ? null : Number.parseInt(text, 10);
    if (copies !== null && !(copies >= 1)) {
      count.value = item.copies ?? "";
      return;
    }
    apply("set_copies", { id: item.id, copies });
  });

  row.querySelector(".remove").addEventListener("click", () => apply("remove_item", { id: item.id }));
  row.querySelector(".split")?.addEventListener("click", () => apply("split_pack", { id: item.id }));
  row.querySelector(".handle").addEventListener("pointerdown", (e) => startReorder(e, row));
  loadThumbnail(item, row.querySelector("img"));
  return row;
}

async function loadThumbnail(item, img) {
  const key = `${item.source}:${item.page}`;
  if (!thumbnails.has(key)) {
    thumbnails.set(key, invoke("thumbnail", { source: item.source, page: item.page }).then((buffer) => {
      const bitmap = bitmapFromBuffer(buffer);
      const canvas = document.createElement("canvas");
      canvas.width = bitmap.width;
      canvas.height = bitmap.height;
      canvas.getContext("2d").putImageData(bitmap, 0, 0);
      return canvas.toDataURL();
    }));
  }
  try {
    img.src = await thumbnails.get(key);
  } catch {
    thumbnails.delete(key);
  }
}

// Pointer-driven reordering: WebView2 disables HTML5 drag and drop while file drops are enabled.
function startReorder(event, row) {
  event.preventDefault();
  const list = $("file-list");
  const rows = [...list.children];
  let target = null;
  row.classList.add("dragging");
  row.setPointerCapture(event.pointerId);

  const move = (e) => {
    for (const r of rows) r.classList.remove("drop-before", "drop-after");
    const over = rows.find((r) => {
      const box = r.getBoundingClientRect();
      return e.clientY >= box.top && e.clientY < box.bottom;
    });
    if (!over || over === row) { target = null; return; }
    const box = over.getBoundingClientRect();
    const before = e.clientY < box.top + box.height / 2;
    over.classList.add(before ? "drop-before" : "drop-after");
    target = { over, before };
  };
  const end = () => {
    row.removeEventListener("pointermove", move);
    row.classList.remove("dragging");
    for (const r of rows) r.classList.remove("drop-before", "drop-after");
    if (!target) return;
    const order = rows.filter((r) => r !== row);
    order.splice(order.indexOf(target.over) + (target.before ? 0 : 1), 0, row);
    apply("reorder_items", { order: order.map((r) => Number(r.dataset.id)) });
  };
  row.addEventListener("pointermove", move);
  row.addEventListener("pointerup", end, { once: true });
  row.addEventListener("pointercancel", end, { once: true });
}

// ---------- preview ----------

let previewRequest = 0;
let previewTimer = null;

function layoutSheet() {
  const stage = $("stage");
  const style = getComputedStyle(stage);
  const availW = stage.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
  const availH = stage.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom);
  const { sheetWidth, sheetHeight } = state.settings;
  const scale = Math.max(0.01, Math.min(availW / sheetWidth, availH / sheetHeight));
  const el = $("sheet");
  el.style.width = `${Math.floor(sheetWidth * scale)}px`;
  el.style.height = `${Math.floor(sheetHeight * scale)}px`;
  $("zoom-label").textContent = `Fit · ${Math.round((scale / CSS_PX_PER_POINT) * 100)}%`;
  return scale;
}

function renderOverlay() {
  const overlay = $("slot-overlay");
  const { sheetWidth, sheetHeight } = state.settings;
  overlay.setAttribute("viewBox", `0 0 ${sheetWidth} ${sheetHeight}`);
  if (state.items.length) {
    overlay.replaceChildren();
    return;
  }
  // With nothing added, show where items will go.
  overlay.innerHTML = state.plan.slots
    .map((s) => `<rect x="${s.x}" y="${s.y}" width="${s.width}" height="${s.height}" fill="#f7f7f8" stroke="#d2d2d6" stroke-width="${sheetWidth / 600}" stroke-dasharray="${sheetWidth / 120} ${sheetWidth / 180}"/>`)
    .join("");
}

function schedulePreview() {
  clearTimeout(previewTimer);
  previewTimer = setTimeout(drawPreview, 90);
}

async function drawPreview() {
  const request = ++previewRequest;
  const scale = layoutSheet();
  const pixelsPerPoint = scale * window.devicePixelRatio;
  let buffer;
  try {
    buffer = await invoke("preview", { sheet, pixelsPerPoint });
  } catch (error) {
    if (request === previewRequest) toast(String(error), "error");
    return;
  }
  if (request !== previewRequest) return;
  const bitmap = bitmapFromBuffer(buffer);
  const canvas = $("sheet-canvas");
  canvas.width = bitmap.width;
  canvas.height = bitmap.height;
  canvas.getContext("2d").putImageData(bitmap, 0, 0);
}

new ResizeObserver(() => { if (state) { layoutSheet(); schedulePreview(); } }).observe($("stage"));

$("prev-sheet").addEventListener("click", () => { sheet -= 1; renderSheetControls(); schedulePreview(); });
$("next-sheet").addEventListener("click", () => { sheet += 1; renderSheetControls(); schedulePreview(); });

function renderSheetControls() {
  const total = state.plan.sheets;
  sheet = Math.max(0, Math.min(sheet, total - 1));
  $("sheet-label").textContent = total ? `Sheet ${sheet + 1} of ${total}` : "No sheets";
  $("prev-sheet").disabled = sheet <= 0;
  $("next-sheet").disabled = sheet >= total - 1;
  $("sheet-total").textContent = total;
  $("sheet-word").textContent = total === 1 ? "sheet" : "sheets";
  $("export").disabled = total === 0;
}

// ---------- export ----------

$("export").addEventListener("click", async () => {
  let maxDpi = null;
  const heavy = await call("check_export");
  if (heavy.length) {
    const choice = await askAboutHeavySheets(heavy);
    if (choice === "cancel") return;
    if (choice === "cap") {
      maxDpi = Number.parseInt($("cap-dpi").value, 10);
      if (!(maxDpi >= 72)) { toast("Downsample resolution must be at least 72 dpi", "error"); return; }
    }
  }
  const button = $("export");
  button.disabled = true;
  try {
    const result = await platform.exportPdf(maxDpi);
    if (result) toast(`Saved ${result.sheets} ${result.sheets === 1 ? "sheet" : "sheets"} · ${result.where}`);
  } catch (error) {
    toast(String(error), "error");
  } finally {
    renderSheetControls();
  }
});

function askAboutHeavySheets(heavy) {
  const dialog = $("heavy-dialog");
  $("heavy-list").innerHTML = heavy
    .slice(0, 6)
    .map((h) => `<li><span>Sheet ${h.sheet + 1}</span><span>${Math.round(h.megapixels).toLocaleString()} MP · ${Math.round(h.maxDpi).toLocaleString()} dpi</span></li>`)
    .join("") + (heavy.length > 6 ? `<li><span>+${heavy.length - 6} more</span><span></span></li>` : "");
  dialog.returnValue = "cancel";
  dialog.showModal();
  return new Promise((resolve) => dialog.addEventListener("close", () => resolve(dialog.returnValue), { once: true }));
}

// ---------- window ----------

if (platform.window) {
  $("wc-min").addEventListener("click", () => platform.window.minimize());
  $("wc-max").addEventListener("click", () => platform.window.toggleMaximize());
  $("wc-close").addEventListener("click", () => platform.window.close());
} else {
  document.querySelector(".window-controls").hidden = true;
}

// ---------- render ----------

function setOn(groupId, value) {
  for (const button of $(groupId).querySelectorAll("button[data-value]")) {
    button.classList.toggle("on", button.dataset.value === value);
  }
}

function setSwitch(id, on) {
  $(id).setAttribute("aria-checked", String(on));
}

function renderSettings() {
  const s = state.settings;
  for (const [id, key] of Object.entries(LENGTH_FIELDS)) {
    if (document.activeElement !== $(id)) $(id).value = toDisplay(s[key]);
  }
  for (const [id, key] of Object.entries(COUNT_FIELDS)) {
    if (document.activeElement !== $(id)) $(id).value = s[key];
  }
  setOn("units", unit);
  setOn("mode", s.mode);
  setOn("scaling", s.scaling);
  setOn("bleed-mode", s.bleedMode);
  setSwitch("mix", s.mixOrientations);
  setSwitch("crop-marks", s.cropMarks);
  setSwitch("trim-lines", s.trimLines);
  $("size-row").hidden = s.mode !== "size";
  $("grid-row").hidden = s.mode !== "grid";
  $("mix-row").hidden = s.mode !== "size";
  $("mark-length").disabled = $("mark-offset").disabled = !s.cropMarks;

  const select = $("preset");
  const match = state.presets.findIndex(
    (p) => Math.abs(p.width - s.sheetWidth) < 0.05 && Math.abs(p.height - s.sheetHeight) < 0.05,
  );
  select.innerHTML =
    state.presets
      .map((p, i) => `<option value="${i}">${p.name} · ${toDisplay(p.width)} × ${toDisplay(p.height)}</option>`)
      .join("") + `<option value="custom">Custom</option>`;
  select.value = match >= 0 ? String(match) : "custom";
}

function render() {
  if (!state) return;
  renderSettings();
  renderFiles();
  renderSheetControls();
  renderOverlay();
  schedulePreview();
}

call("get_state").then((initial) => { state = initial; render(); });
