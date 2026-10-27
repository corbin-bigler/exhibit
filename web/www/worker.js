// Runs the Rust side (pkg/exhibit_web.js) off the page's thread. Messages are
// { id, command, args } in and { id, result } or { id, error } out, matching Tauri's commands.
import init, { Exhibit } from "./pkg/exhibit_web.js";

let exhibit;
const ready = init().then(() => { exhibit = new Exhibit(); });

const commands = {
  extensions: () => exhibit.extensions(),
  get_state: () => exhibit.state(),
  import_files: ({ files }) => exhibit.import(files.map((f) => f.name), files.map((f) => new Uint8Array(f.bytes))),
  remove_item: ({ id }) => exhibit.removeItem(id),
  clear_items: () => exhibit.clearItems(),
  reorder_items: ({ order }) => exhibit.reorderItems(order),
  split_pack: ({ id }) => exhibit.splitPack(id),
  set_copies: ({ id, copies }) => exhibit.setCopies(id, copies ?? undefined),
  update_settings: ({ settings }) => exhibit.updateSettings(settings),
  preview: ({ sheet, pixelsPerPoint }) => exhibit.preview(sheet, pixelsPerPoint).buffer,
  thumbnail: ({ source, page }) => exhibit.thumbnail(source, page).buffer,
  check_export: () => exhibit.checkExport(),
  export_pdf: ({ maxDpi }) => exhibit.exportPdf(maxDpi ?? undefined),
};

self.addEventListener("message", async ({ data: { id, command, args } }) => {
  try {
    await ready;
    const run = commands[command];
    if (!run) throw new Error(`unknown command ${command}`);
    const result = run(args ?? {});
    const transfer = result instanceof ArrayBuffer ? [result] : result?.bytes ? [result.bytes.buffer] : [];
    self.postMessage({ id, result }, transfer);
  } catch (error) {
    self.postMessage({ id, error: error?.message ?? String(error) });
  }
});
