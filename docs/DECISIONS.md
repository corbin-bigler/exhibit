# Decisions for review

Made while building the first implementation. Each one is easy to change; the ones most worth a
look are marked ★.

## Architecture & dependencies

1. ★ **Three crates, following ARCHITECTURE.md.** `crates/domain` (no dependencies at all —
   layout math, bleed, marks, items, use cases, ports), `crates/data` (adapters: decoding, PDF
   writing, previews, saving), `app` (Tauri commands + UI = the interface layer). The UI never
   touches `data`; commands only call use cases.
2. ★ **Dependencies added** (you'd normally approve each — listing them so you can veto):
   - `krilla` 0.8 (+`pdf` feature) — PDF writer; embeds PDF pages as Form XObjects, passes JPEGs
     through, keeps ICC.
   - `krilla-svg` + `usvg` 0.47 — SVG → vector PDF.
   - `hayro` 0.8 — pure-Rust PDF rasteriser for the preview and thumbnails.
   - `image` 0.25 — TIFF/BMP/TGA/QOI/PNM decoding, EXIF orientation, resampling.
   - `jxl-oxide` 0.12 — pure-Rust JPEG XL decoder.
   - `windows` 0.62 (Imaging/COM) — HEIC/HEIF/AVIF through Windows Imaging Component.
   - `tauri` 2, `tauri-plugin-dialog` 2, `serde`, `serde_json` — app shell.
3. ★ **HEIC/AVIF via Windows (WIC), not libheif.** libheif needs CMake + a C toolchain build; WIC
   is already on Windows 11 and keeps full resolution, 10-bit depth and the ICC profile. Catch:
   it needs the Store's *HEIF Image Extensions* + *HEVC Video Extensions* (AVIF: *AV1 Video
   Extension*). This machine has them; shop PCs might not — the import error says what to
   install. Swapping to libheif later only touches `crates/data/src/wic.rs`.
4. **No frontend build step.** Plain HTML/CSS/JS in `app/ui`, no npm. Easy to move to a framework
   later if the UI grows.
5. **Fonts bundled** (Poppins 400–700, Archivo Black; OFL licensed) so the app works offline.
6. **Custom title bar** (`decorations: false`) to match the design. Downside: Windows 11's
   snap-layout flyout on the maximise button is lost. Can be restored with a plugin or by going
   back to the native title bar.

## Output quality

7. ★ **What is untouched vs re-encoded:**
   - PDF pages → vector, embedded once per page no matter how many copies.
   - SVG → vector paths. **Text is outlined** (not live text) so the print never depends on fonts.
   - JPEG → byte-for-byte (`/DCTDecode`), ICC kept. EXIF orientation is applied as a transform,
     not by re-encoding.
   - PNG / WebP / GIF → as krilla embeds them (lossless, alpha and 16-bit kept).
   - TIFF / BMP / TGA / QOI / PNM / JXL / HEIC / AVIF → decoded once, stored losslessly (Flate)
     at original pixel size and bit depth with their ICC profile. Files can be larger than the
     source for formats that were lossy (HEIC/AVIF/JXL) — there's no lossless way to keep their
     compressed bytes in a PDF.
8. **`/Interpolate` off** on all images, so the RIP doesn't smooth pixels it shouldn't.
9. **Output is PDF 1.7**, raised automatically to 2.0 only if an embedded PDF requires it.
10. **CMYK TIFF/JXL** decode is best-effort; CMYK **JPEG** passes through untouched.

## The heavy-sheet check (your lunch idea)

11. ★ **Metric: raster megapixels per sheet, counting every placement**, threshold **500 MP**.
    60 copies of an 8K photo ≈ 2,000 MP → flagged; 4 copies of a 24 MP photo ≈ 96 MP → fine.
    The PDF itself stays small (the image is stored once), but a RIP rasterises every copy, so
    per-placement is the right count. Threshold lives in `crates/domain/src/job.rs`.
12. ★ **Options offered: Downsample to N dpi (default 300) · Export anyway · Cancel.** I did
    *not* add "rasterise the whole sheet": downsampling fixes the load while keeping vectors and
    text sharp and colours managed. Easy to add if you want it.
13. **Downsampling only touches images above the cap** (Lanczos3, bit depth + ICC kept).
    Images below it, and all vectors, stay original.
14. The **preview is always rasterised** from ≤1600 px proxies, so a 60-up of an 8K image costs
    the same as a 60-up of a small one.
14a. ★ **The preview has its own pixel compositor** (`crates/data/src/composite.rs`). Each art is
    rasterised once at slot size; every sheet pixel then looks up its art pixel directly, reflected
    (Mirror) or clamped (Edge) using the same domain geometry as the export. Drawing the preview
    through clip paths left hairlines at every trim and gutter: anti-aliasing at a clip edge leaks
    whatever is underneath, and hayro can't isolate groups to avoid it.
14b. **Export seam handling:** bleed is drawn for every item first, then all art on top; bleed
    copies tuck 1 pt under the art (bleed shifts 0.7 mm, cut away anyway) and, between items, run
    across the whole gutter under the neighbour. RIPs don't anti-alias, so print never had lines;
    this keeps on-screen viewers like Acrobat close to clean too. A faint line may still appear in
    a viewer where artwork's own edge sits on the trim — that's the viewer, not the file.

## Layout, bleed & marks

15. Default job: **Tabloid Extra 12×18, ¼" margin, By size 5×7, ⅛" gutter, Fill, ⅛" Mirror
    bleed, crop marks ¼" long with 1/16" offset, trim lines off.** Settings reset each launch
    (no saved jobs, per your direction); only the in/mm choice is remembered.
16. **Art auto-rotates** to match the slot orientation (landscape photo into a portrait slot).
    Square art never rotates. There's no per-item rotation override yet.
17. **Mixed orientations** use a two-band search (rows of one orientation, then rows/columns of
    the other) — finds the 3× 4×6 on Letter case; it's not a general bin-packer.
18. **Inner bleed is capped at half the gap** to the nearest neighbour; outer bleed never leaves
    the sheet.
19. **Main art is clipped to the trim**; bleed is drawn only by the bleed mode. So with Fill, the
    cropped-off part of a photo is *not* used as bleed — Mirror/Edge are. (Using real image
    overflow as bleed is possible as a fourth mode if you'd like it.)
20. **Edge bleed** stretches one source pixel for raster art, a 0.5 pt strip for vectors.
21. **Crop marks** are 0.25 pt, pure black (K only), drawn only for filled slots, at every
    distinct cut line, all outside the items' bounding box (incl. mixed layouts, per your call).
    **Trim lines** are the same weight, drawn on top of the art.
22. **Copies flow across sheets**: an override of 6 on a 4-up uses 1½ sheets and the next item
    continues on the same sheet. Auto (empty) always means one full sheet per page.

## UI

23. **Reordering uses pointer events, not HTML5 drag & drop** — WebView2 disables HTML5 DnD
    while Explorer file drops are enabled.
24. **Switching layout mode carries the current result over**: By grid starts at the by-size
    grid count; By size starts at the grid's derived item size.
25. Import/export results show as brief toasts (error toasts for failed files). The export toast
    shows the sheet count and path.
26. **App icon** is a placeholder cropped from the wordmark (roller, cab, driver). Worth a proper
    vector icon from whoever owns the brand.
27. **Files passed on the command line open at launch**, so dragging files onto the exe or
    "Open with → Exhibit" works.

## Things I couldn't settle without you

- `ARCHITECTURE.md` and `CLAUDE.md` were missing from the main checkout when I went to commit
  them, so they're not on this branch. I still have their contents if you want them restored.
- No real HEIC/AVIF/CMYK/JXL sample files were tested — see `docs/VALIDATION.md` for what's
  still 🟡.
- Installer (`cargo tauri build` → NSIS) isn't set up yet; it needs `tauri-cli` installed.
