"use strict";

// Everything that differs between the Windows app (Tauri) and the browser. app.js only talks to
// `platform`; the choice is made here at runtime because the same files ship in both.
window.platform = window.__TAURI__ ? tauriPlatform() : webPlatform();

function tauriPlatform() {
  const { invoke } = window.__TAURI__.core;
  const appWindow = window.__TAURI__.window.getCurrentWindow();
  const webview = window.__TAURI__.webview.getCurrentWebview();
  return {
    invoke,
    /** Resolves to an import result, or one with nothing added when the dialog is cancelled. */
    pickFiles: () => invoke("pick_files"),
    /** `hover(on)` while files are dragged over the window; `arrived(promise of import result)` on drop. */
    onFilesArrive({ hover, arrived }) {
      webview.onDragDropEvent((event) => {
        const { type } = event.payload;
        hover(type === "enter" || type === "over");
        if (type === "drop" && event.payload.paths.length) {
          arrived(invoke("import_files", { paths: event.payload.paths }));
        }
      });
    },
    /** Asks where to save, then writes. Resolves to `{ sheets, where }`, or null if cancelled. */
    async exportPdf(maxDpi) {
      const result = await invoke("export_pdf", { maxDpi });
      return result && { sheets: result.sheets, where: result.path };
    },
    window: {
      minimize: () => appWindow.minimize(),
      toggleMaximize: () => appWindow.toggleMaximize(),
      close: () => appWindow.close(),
    },
  };
}

function webPlatform() {
  // The Rust side runs in a worker so big imports and exports don't freeze the page.
  const worker = new Worker("worker.js", { type: "module" });
  const pending = new Map();
  let nextId = 0;

  worker.addEventListener("message", ({ data }) => {
    const request = pending.get(data.id);
    if (!request) return;
    pending.delete(data.id);
    if ("error" in data) request.reject(data.error);
    else request.resolve(data.result);
  });
  worker.addEventListener("error", (event) => {
    const message = event.message || "Exhibit stopped working. Reload the page to start again.";
    for (const request of pending.values()) request.reject(message);
    pending.clear();
  });

  function invoke(command, args = {}, transfer = []) {
    return new Promise((resolve, reject) => {
      const id = ++nextId;
      pending.set(id, { resolve, reject });
      worker.postMessage({ id, command, args }, transfer);
    });
  }

  async function importFiles(fileList) {
    const files = await Promise.all(
      [...fileList].map(async (file) => ({ name: file.name, bytes: await file.arrayBuffer() })),
    );
    return invoke("import_files", { files }, files.map((f) => f.bytes));
  }

  const accept = invoke("extensions").then((list) => list.map((e) => `.${e}`).join(","));

  return {
    invoke,

    async pickFiles() {
      const input = document.createElement("input");
      input.type = "file";
      input.multiple = true;
      input.accept = await accept;
      const picked = new Promise((resolve) => {
        input.addEventListener("change", () => resolve(input.files), { once: true });
        input.addEventListener("cancel", () => resolve(null), { once: true });
      });
      input.click();
      const files = await picked;
      return files && files.length ? importFiles(files) : invoke("import_files", { files: [] });
    },

    onFilesArrive({ hover, arrived }) {
      const carriesFiles = (e) => e.dataTransfer && [...e.dataTransfer.types].includes("Files");
      let depth = 0;
      window.addEventListener("dragenter", (e) => {
        if (!carriesFiles(e)) return;
        depth += 1;
        hover(true);
      });
      window.addEventListener("dragleave", (e) => {
        if (!carriesFiles(e)) return;
        depth = Math.max(0, depth - 1);
        if (depth === 0) hover(false);
      });
      window.addEventListener("dragover", (e) => {
        if (!carriesFiles(e)) return;
        e.preventDefault();
        e.dataTransfer.dropEffect = "copy";
      });
      window.addEventListener("drop", (e) => {
        if (!carriesFiles(e)) return;
        e.preventDefault();
        depth = 0;
        hover(false);
        if (e.dataTransfer.files.length) arrived(importFiles(e.dataTransfer.files));
      });
      // Files opened with the installed app ("Open with" → Exhibit), where the browser supports it.
      if ("launchQueue" in window) {
        window.launchQueue.setConsumer(async ({ files }) => {
          if (files.length) arrived(Promise.all(files.map((h) => h.getFile())).then(importFiles));
        });
      }
    },

    async exportPdf(maxDpi) {
      // Ask first: browsers only show the save dialog straight after a click, not after rendering.
      let handle = null;
      if ("showSaveFilePicker" in window) {
        try {
          handle = await window.showSaveFilePicker({
            suggestedName: "exhibit.pdf",
            types: [{ description: "PDF", accept: { "application/pdf": [".pdf"] } }],
          });
        } catch (error) {
          if (error.name === "AbortError") return null;
          throw error.message || String(error);
        }
      }
      const { sheets, bytes } = await invoke("export_pdf", { maxDpi });
      if (handle) {
        const writable = await handle.createWritable();
        await writable.write(bytes);
        await writable.close();
        return { sheets, where: handle.name };
      }
      const url = URL.createObjectURL(new Blob([bytes], { type: "application/pdf" }));
      const link = Object.assign(document.createElement("a"), { href: url, download: "exhibit.pdf" });
      link.click();
      setTimeout(() => URL.revokeObjectURL(url), 60_000);
      return { sheets, where: "Downloads" };
    },

    window: null,
  };
}
