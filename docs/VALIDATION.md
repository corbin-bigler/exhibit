# Validation Matrix

Every feature Exhibit must have, how it is verified, and its current status.

**Method:** `unit` = automated test in the domain crate · `integration` = automated test that
builds a real PDF and inspects it · `manual` = checked by a person in the running app (or in
Acrobat/press RIP for print checks).

**Status:** ✅ passing · 🟡 implemented, not yet verified · ⬜ not implemented · ❌ failing

## 1. Import

| ID | Feature | Pass criteria | Method | Status |
|---|---|---|---|---|
| IMP-01 | JPEG (`.jpg .jpeg .jfif .pjpeg .pjp`) | Imports; pixel size + ICC read | integration | ⬜ |
| IMP-02 | PNG (incl. 16-bit, alpha, iCCP) | Imports; bit depth and ICC preserved | integration | ⬜ |
| IMP-03 | WebP (lossy + lossless) | Imports | integration | ⬜ |
| IMP-04 | GIF (first frame) | Imports | integration | ⬜ |
| IMP-05 | BMP | Imports | integration | ⬜ |
| IMP-06 | TIFF (8/16-bit RGB, gray; ICC) | Imports; ICC preserved | integration | ⬜ |
| IMP-07 | TGA, QOI, PNM | Imports | integration | ⬜ |
| IMP-08 | JPEG XL | Imports; ICC preserved | integration | ⬜ |
| IMP-09 | HEIC / HEIF | Imports at full resolution; ICC preserved | manual (needs Windows HEIF+HEVC extensions) | ⬜ |
| IMP-10 | AVIF | Imports at full resolution; ICC preserved | manual (needs Windows AV1 extension) | ⬜ |
| IMP-11 | SVG | Imports as vector | integration | ⬜ |
| IMP-12 | PDF (multi-page) | Imports as one pack with N pages | integration | ⬜ |
| IMP-13 | Unsupported / corrupt file | Clear error naming the file; other files still import | integration | ⬜ |
| IMP-14 | Add via button (multi-select dialog) | Files appear in list, order preserved | manual | ⬜ |
| IMP-15 | Add via drag & drop onto window | Files appear in list | manual | ⬜ |

## 2. Output quality (the core promise)

| ID | Feature | Pass criteria | Method | Status |
|---|---|---|---|---|
| Q-01 | PDF pages stay vector | Output contains the source page as a Form XObject; no rasterized image of it | integration | ⬜ |
| Q-02 | Repeated PDF page reuses one XObject | 4 copies → 1 XObject referenced 4× | integration | ⬜ |
| Q-03 | JPEG bytes untouched | Output image stream is `/DCTDecode` and byte-identical to source JPEG data | integration | ⬜ |
| Q-04 | ICC profiles kept | Image with ICC → output `/ICCBased` colorspace with the same profile bytes | integration | ⬜ |
| Q-05 | No resampling of raster images | Output image pixel size == source pixel size | integration | ⬜ |
| Q-06 | 16-bit sources stay 16-bit | 16-bit PNG/TIFF → `/BitsPerComponent 16` | integration | ⬜ |
| Q-07 | SVG stays vector | Output contains vector paths, no raster image for the SVG | integration | ⬜ |
| Q-08 | CMYK JPEG kept CMYK | Output `/DeviceCMYK` or CMYK ICC, no conversion | integration | ⬜ |
| Q-09 | Transparency kept | PNG alpha → `/SMask` present | integration | ⬜ |
| Q-10 | Visual color match | Side-by-side in Acrobat: output matches source | manual | ⬜ |

## 3. Sheet & units

| ID | Feature | Pass criteria | Method | Status |
|---|---|---|---|---|
| S-01 | Presets (Letter, Legal, Tabloid, 12×18, 13×19, A4, A3, SRA3) | Selecting sets width/height | manual | ⬜ |
| S-02 | Custom width/height/margin | Output page size exact to 0.01 pt | unit + integration | ⬜ |
| S-03 | Inches ↔ mm switch | Values convert, layout unchanged; inches default | unit + manual | ⬜ |
| S-04 | Invalid input (0, negative, text) | Rejected, last valid value restored | manual | ⬜ |

## 4. Layout

| ID | Feature | Pass criteria | Method | Status |
|---|---|---|---|---|
| L-01 | By size (default mode) | Max count of uniform-orientation slots, grid centred in margins | unit | ⬜ |
| L-02 | Auto-orientation of uniform grid | Picks portrait vs landscape slots, whichever fits more | unit | ⬜ |
| L-03 | Mix orientations toggle | 4×6 on Letter (⅛" margin/gutter) → 3 up; off → 2 up | unit | ⬜ |
| L-04 | By grid | Across × Down slots, item size derived from sheet | unit | ⬜ |
| L-05 | Switching to By grid seeds Across × Down with current fit | Values equal by-size fit | manual | ⬜ |
| L-06 | Gutter | Exact spacing between trim edges | unit | ⬜ |
| L-07 | Fill scaling | Art covers slot, centred, cropped | unit + manual | ⬜ |
| L-08 | Fit scaling | Art fully visible, centred, letterboxed | unit + manual | ⬜ |
| L-09 | Art auto-rotates to slot orientation | Landscape art in portrait slot is rotated 90° | unit | ⬜ |
| L-10 | Nothing fits | Shows 0 up, export disabled, no crash | unit + manual | ⬜ |

## 5. Bleed

| ID | Feature | Pass criteria | Method | Status |
|---|---|---|---|---|
| B-01 | Bleed width | Bleed box = trim + width on each outer side | unit | ⬜ |
| B-02 | Inner bleed capped at half the gap to the neighbour | No overlap between neighbours | unit | ⬜ |
| B-03 | Mirror | Bleed shows art reflected about each trim edge (+ corners) | manual | ⬜ |
| B-04 | Edge | Bleed shows outermost edge stretched | manual | ⬜ |
| B-05 | Scale | Art scaled to cover trim + bleed | manual | ⬜ |
| B-06 | Bleed works for vector sources | Mirror on a PDF page stays vector | integration | ⬜ |

## 6. Marks

| ID | Feature | Pass criteria | Method | Status |
|---|---|---|---|---|
| M-01 | Crop marks on/off | Off → no marks | integration | ⬜ |
| M-02 | Marks at every cut line, outside the items' bounding box | Positions exact; never inside the item zone (incl. mixed layouts) | unit | ⬜ |
| M-03 | Length / offset | Mark starts `offset` beyond the bleed edge, `length` long | unit | ⬜ |
| M-04 | Trim lines on/off | Thin line on every trim edge, printed | integration + manual | ⬜ |

## 7. Items list

| ID | Feature | Pass criteria | Method | Status |
|---|---|---|---|---|
| I-01 | Count auto (default) | Empty field = slots-per-sheet copies; sheet icon shown | unit + manual | ⬜ |
| I-02 | Count override | Typed number used; clearing reverts to auto | unit + manual | ⬜ |
| I-03 | Remove item (×) | Item gone, sheets recomputed | unit + manual | ⬜ |
| I-04 | Clear all | List empty, empty state shown | unit + manual | ⬜ |
| I-05 | Reorder by drag | New order reflected on sheets | unit + manual | ⬜ |
| I-06 | PDF pack split | Pack → N page items in place, each keeps pack's count | unit + manual | ⬜ |
| I-07 | Split pages reorderable anywhere | Image can sit between pages | manual | ⬜ |
| I-08 | Quality badge | Vector badge for PDF/SVG; effective DPI for raster; amber < 300 | unit + manual | ⬜ |
| I-09 | Specs line | Format · pixel size (raster) or page (PDF); no helper text | manual | ⬜ |

## 8. Preview & export

| ID | Feature | Pass criteria | Method | Status |
|---|---|---|---|---|
| P-01 | Preview matches output | Preview is rendered from the same drawing code as the export | manual | ⬜ |
| P-02 | Sheet stepper | Steps 1..N, disabled at ends | manual | ⬜ |
| P-03 | Fit zoom fills the preview area | Sheet scales with window | manual | ⬜ |
| P-04 | Sheet total in footer | Equals exported page count | integration | ⬜ |
| P-05 | Export → Windows Save As → PDF written | File opens in Acrobat without errors | manual | ⬜ |
| P-06 | Export disabled when there are no sheets | Button disabled | manual | ⬜ |

## 9. Platform & brand

| ID | Feature | Pass criteria | Method | Status |
|---|---|---|---|---|
| X-01 | Windows 11 build runs | `cargo run` launches window | manual | ⬜ |
| X-02 | Light mode only, Steamroller brand | Matches approved Paper design | manual | ⬜ |
| X-03 | Large job performance | 50 files / 200 sheets exports < 30 s, UI stays responsive | manual | ⬜ |
