# Steamroller Exhibit

Imposition for print: lay artwork out on press sheets with bleed and crop marks, then export a
print-ready PDF that keeps every source at its original quality. Runs as a portable Windows 11
app and in the browser.

## Run

```
cargo run -p exhibit                      # the app
cargo run -p exhibit -- a.pdf b.heic      # open with files
cargo test --workspace                    # automated validation
```

Browser build (needs `rustup target add wasm32-unknown-unknown` and
`cargo install wasm-bindgen-cli --version <the wasm-bindgen version in Cargo.lock>`):

```
powershell -File web/build.ps1                                # → dist/web, a static site
python -m http.server 8765 --directory dist/web               # try it at http://127.0.0.1:8765
```

Manual-check helpers in `crates/data/examples`:

```
cargo run -p exhibit-data --example fixtures -- <dir>                 # sample artwork
cargo run -p exhibit-data --example impose -- out.pdf [--edge|--scale|--trim] files...
cargo run -p exhibit-data --example peek -- in.pdf out.png <page> <scale> [x y w h]
```

## Layout

| Path | Layer |
|---|---|
| `crates/domain` | Models, layout math, use cases, ports. No dependencies. |
| `crates/data` | Decoders, krilla PDF writer, hayro previews, file sink. |
| `crates/session` | The commands both shells expose, and the shapes sent to the UI. |
| `app` | Windows shell: Tauri commands (`src`) and the shared UI (`ui`). |
| `web` | Browser shell: wasm-bindgen bindings (`src`), worker and manifest (`www`), `build.ps1`. |

`app/ui/platform.js` holds everything the UI does differently in Tauri and the browser.

`docs/VALIDATION.md` tracks every feature and how it's verified; `docs/DECISIONS.md` lists the
choices made along the way.
