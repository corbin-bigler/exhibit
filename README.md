# Steamroller Exhibit

Imposition for print: lay artwork out on press sheets with bleed and crop marks, then export a
print-ready PDF that keeps every source at its original quality. Windows 11.

## Run

```
cargo run -p exhibit                      # the app
cargo run -p exhibit -- a.pdf b.heic      # open with files
cargo test --workspace                    # automated validation
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
| `app` | Tauri commands (`src`) and the UI (`ui`). |

`docs/VALIDATION.md` tracks every feature and how it's verified; `docs/DECISIONS.md` lists the
choices made along the way.
