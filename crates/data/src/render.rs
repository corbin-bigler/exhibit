//! Drawing print jobs with krilla. The on-screen preview is composited in `composite.rs` from the
//! same job geometry; each piece of art is rasterised here, with proxies standing in for big images.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use exhibit_domain::art::{ArtFrame, effective_dpi};
use exhibit_domain::geometry::{Line, Rect};
use exhibit_domain::items::ArtKind;
use exhibit_domain::job::{Placement, PrintJob, SheetJob};
use exhibit_domain::marks::MARK_WEIGHT;
use exhibit_domain::ports::{Bitmap, ExportOptions, PortError, SheetRenderer};
use exhibit_domain::units::POINTS_PER_INCH;
use exhibit_domain::settings::BleedMode;
use krilla::configure::{ConfigurationBuilder, PdfVersion};
use krilla::error::KrillaError;
use krilla::geom::{PathBuilder, Size, Transform};
use krilla::image::Image;
use krilla::page::PageSettings;
use krilla::paint::{FillRule, Stroke};
use krilla::pdf::PdfError;
use krilla::surface::Surface;
use krilla::{Document, SerializeSettings};
use krilla_svg::{SurfaceExt, SvgSettings};

use crate::library::{Content, Entry, SourceStore};

#[derive(Clone, Copy, PartialEq)]
enum Quality {
    Print { max_dpi: Option<f64> },
    Proxy,
}

/// Downsampled images made during one export, keyed by entry address and pixel size.
type DownsampleCache = RefCell<HashMap<(usize, u32, u32), Option<Image>>>;

/// Width of the art strip stretched by Edge bleed when the art is vector, in points.
const VECTOR_EDGE_SAMPLE: f64 = 0.5;

pub struct KrillaRenderer {
    store: Arc<SourceStore>,
}

impl KrillaRenderer {
    pub fn new(store: Arc<SourceStore>) -> Self {
        Self { store }
    }
}

impl SheetRenderer for KrillaRenderer {
    fn render_pdf(&self, job: &PrintJob, options: &ExportOptions) -> Result<Vec<u8>, PortError> {
        write_pdf(&self.store, job, &job.sheets, Quality::Print { max_dpi: options.max_dpi }).map_err(PortError)
    }

    fn render_preview(&self, job: &PrintJob, sheet: usize, pixels_per_point: f64) -> Result<Bitmap, PortError> {
        crate::composite::render_preview(&self.store, job, sheet, pixels_per_point).map_err(PortError)
    }
}

fn write_pdf(store: &SourceStore, job: &PrintJob, sheets: &[SheetJob], quality: Quality) -> Result<Vec<u8>, String> {
    let mut version = PdfVersion::Pdf17;
    let cache = DownsampleCache::default();
    loop {
        let configuration = ConfigurationBuilder::new().with_version(version).finish().map_err(|e| format!("{e:?}"))?;
        let mut document = Document::new_with(SerializeSettings { configuration, ..SerializeSettings::default() });
        for sheet in sheets {
            let settings = PageSettings::from_wh(job.sheet.width as f32, job.sheet.height as f32)
                .ok_or("sheet size is invalid")?;
            let mut page = document.start_page_with(settings);
            let mut surface = page.surface();
            draw_sheet(&mut surface, store, sheet, quality, &cache)?;
            surface.finish();
            page.finish();
        }
        match document.finish() {
            Ok(bytes) => return Ok(bytes),
            // An embedded PDF needs a newer output version: raise it and write again.
            Err(KrillaError::Pdf(_, PdfError::VersionMismatch(needed), _)) if needed != version => version = needed,
            Err(error) => return Err(error.to_string()),
        }
    }
}

fn draw_sheet(
    surface: &mut Surface,
    store: &SourceStore,
    sheet: &SheetJob,
    quality: Quality,
    cache: &DownsampleCache,
) -> Result<(), String> {
    let entries = sheet
        .placements
        .iter()
        .map(|p| store.get(p.art_ref.source).ok_or("a file used on this sheet was removed"))
        .collect::<Result<Vec<_>, _>>()?;
    let art = |i: usize| {
        let p = &sheet.placements[i];
        Art { entry: &entries[i], page: p.art_ref.art.page, quality, limit: downsample_target(p, quality), cache: Some(cache) }
    };
    // Two passes so every trim edge is drawn over solid colour: all bleed first, then all art.
    for (i, p) in sheet.placements.iter().enumerate() {
        draw_bleed(surface, &art(i), p, touching_sides(&sheet.placements, i));
    }
    for (i, p) in sheet.placements.iter().enumerate() {
        let clip = if p.bleed_mode == BleedMode::Scale { p.clip() } else { p.trim };
        with_clip(surface, &clip, |s| art(i).draw_framed(s, &p.frame));
    }
    surface.set_fill(None);
    surface.set_stroke(Some(Stroke { width: MARK_WEIGHT as f32, ..Stroke::default() }));
    for rect in &sheet.trim_lines {
        if let Some(path) = rect_path(rect) {
            surface.draw_path(&path);
        }
    }
    for line in &sheet.crop_marks {
        if let Some(path) = line_path(line) {
            surface.draw_path(&path);
        }
    }
    surface.set_stroke(None);
    Ok(())
}

/// Regions drawn separately overlap instead of meeting edge to edge, so anti-aliasing in viewers
/// and previews never shows a hairline of paper between them. Bleed strips reach this far into
/// their own trim, and where an item faces a neighbour its bleed runs across the whole gutter and
/// this far under the neighbour's trim. The art, drawn afterwards, covers both; the gutter is cut away.
const INNER_OVERLAP: f64 = 6.0;

/// Bleed copies are tucked this far under the main art (Mirror reflects about a line this far
/// inside the trim, Edge stretches to twice this past it), so on-screen viewers don't show a gap
/// at the trim. Bleed content shifts by 2 pt (0.7 mm), which disappears when cut.
const BLEED_TUCK: f64 = 1.0;

/// Which sides of a placement's bleed meet a neighbour's bleed: (left, top, right, bottom).
fn touching_sides(placements: &[Placement], index: usize) -> [bool; 4] {
    let me = placements[index].clip();
    let mut sides = [false; 4];
    for (i, other) in placements.iter().enumerate() {
        if i == index {
            continue;
        }
        let o = other.clip();
        let near = |a: f64, b: f64| (a - b).abs() < 0.01;
        if me.overlaps_vertically(&o) {
            sides[0] |= near(me.x, o.right());
            sides[2] |= near(me.right(), o.x);
        }
        if me.overlaps_horizontally(&o) {
            sides[1] |= near(me.y, o.bottom());
            sides[3] |= near(me.bottom(), o.y);
        }
    }
    sides
}

/// How far bleed extends on a side: its own width, or across the gutter and under the
/// neighbour's trim when it faces another item.
fn bleed_reach(bleed: f64, touching: bool) -> f64 {
    if touching { 2.0 * bleed + INNER_OVERLAP } else { bleed }
}

fn draw_bleed(surface: &mut Surface, art: &Art, p: &Placement, touching: [bool; 4]) {
    if p.bleed_mode == BleedMode::Scale {
        return;
    }
    for (strip, transform) in bleed_strips(p, edge_sample(p), touching, BLEED_TUCK) {
        with_clip(surface, &strip, |s| {
            s.push_transform(&transform);
            art.draw_framed(s, &p.frame);
            s.pop();
        });
    }
}

/// Pixel size to resample raster art to when it exceeds the export's resolution cap.
fn downsample_target(p: &Placement, quality: Quality) -> Option<(u32, u32)> {
    let Quality::Print { max_dpi: Some(max) } = quality else { return None };
    let dpi = effective_dpi(p.art_ref.art.kind, &p.frame)?;
    if dpi <= max + 0.5 {
        return None;
    }
    let r = p.frame.rect;
    let (w, h) = if p.frame.rotated { (r.height, r.width) } else { (r.width, r.height) };
    let px = |pt: f64| ((pt / POINTS_PER_INCH * max).ceil() as u32).max(1);
    Some((px(w), px(h)))
}

/// How much of the art's edge Edge bleed stretches: one source pixel, or a hairline for vectors.
fn edge_sample(p: &Placement) -> Option<(f64, f64)> {
    if p.bleed_mode != BleedMode::Edge {
        return None;
    }
    let ArtFrame { rect, rotated } = p.frame;
    Some(match p.art_ref.art.kind {
        ArtKind::Vector => (VECTOR_EDGE_SAMPLE, VECTOR_EDGE_SAMPLE),
        ArtKind::Raster { width_px, height_px } => {
            let (across, down) = if rotated { (height_px, width_px) } else { (width_px, height_px) };
            (rect.width / across.max(1) as f64, rect.height / down.max(1) as f64)
        }
    })
}

/// The eight bleed regions around a trim box, each with the transform that fills it:
/// a reflection (Mirror), or a stretch of the outermost sample of the art (Edge).
fn bleed_strips(
    p: &Placement,
    edge_sample: Option<(f64, f64)>,
    touching: [bool; 4],
    tuck: f64,
) -> Vec<(Rect, Transform)> {
    let t = p.trim;
    let b = p.bleed;
    let (l, r, top, bottom) = (t.x, t.right(), t.y, t.bottom());

    // Per-axis maps: (scale, translate) for x and y in each region. Each copy is tucked
    // `tuck` under the main art so no seam shows at the trim.
    let d = tuck;
    let x_map = |side: i8| -> (f64, f64) {
        match (side, edge_sample) {
            (0, _) => (1.0, 0.0),
            (-1, None) => (-1.0, 2.0 * (l + d)),
            (_, None) => (-1.0, 2.0 * (r - d)),
            // Edge samples the strip one sample in from the edge: the outermost one is
            // anti-aliased against the art's own boundary and would stretch into a pale band.
            (-1, Some((ex, _))) => {
                let ex = ex.min(t.width / 2.0);
                let s = (b.left + 2.0 * d) / ex;
                (s, l - b.left - (l + ex) * s)
            }
            (_, Some((ex, _))) => {
                let ex = ex.min(t.width / 2.0);
                let s = (b.right + 2.0 * d) / ex;
                (s, r - 2.0 * d + (2.0 * ex - r) * s)
            }
        }
    };
    let y_map = |side: i8| -> (f64, f64) {
        match (side, edge_sample) {
            (0, _) => (1.0, 0.0),
            (-1, None) => (-1.0, 2.0 * (top + d)),
            (_, None) => (-1.0, 2.0 * (bottom - d)),
            (-1, Some((_, ey))) => {
                let ey = ey.min(t.height / 2.0);
                let s = (b.top + 2.0 * d) / ey;
                (s, top - b.top - (top + ey) * s)
            }
            (_, Some((_, ey))) => {
                let ey = ey.min(t.height / 2.0);
                let s = (b.bottom + 2.0 * d) / ey;
                (s, bottom - 2.0 * d + (2.0 * ey - bottom) * s)
            }
        }
    };

    let mut strips = Vec::with_capacity(8);
    for sx in [-1i8, 0, 1] {
        for sy in [-1i8, 0, 1] {
            if sx == 0 && sy == 0 {
                continue;
            }
            let (x, w) = match sx {
                -1 => (l - b.left, b.left),
                0 => (l, t.width),
                _ => (r, b.right),
            };
            let (y, h) = match sy {
                -1 => (top - b.top, b.top),
                0 => (top, t.height),
                _ => (bottom, b.bottom),
            };
            if w <= 1e-6 || h <= 1e-6 {
                continue;
            }
            let (ax, cx) = x_map(sx);
            let (ay, cy) = y_map(sy);
            // Overlap neighbouring regions; an outer edge only grows where another item's bleed meets it.
            let inner = |span: f64| INNER_OVERLAP.min(span);
            let pad = |outer: bool, touch: bool, bleed: f64, span: f64| {
                if outer { bleed_reach(bleed, touch) - bleed } else { inner(span) }
            };
            let pl = pad(sx == -1, touching[0], b.left, t.width);
            let pr = pad(sx == 1, touching[2], b.right, t.width);
            let pt = pad(sy == -1, touching[1], b.top, t.height);
            let pb = pad(sy == 1, touching[3], b.bottom, t.height);
            strips.push((
                Rect::new(x - pl, y - pt, w + pl + pr, h + pt + pb),
                Transform::from_row(ax as f32, 0.0, 0.0, ay as f32, cx as f32, cy as f32),
            ));
        }
    }
    strips
}

struct Art<'a> {
    entry: &'a Entry,
    page: usize,
    quality: Quality,
    /// Resample raster art to this pixel size instead of embedding the original.
    limit: Option<(u32, u32)>,
    cache: Option<&'a DownsampleCache>,
}

impl Art<'_> {
    fn downsampled(&self, (w, h): (u32, u32)) -> Option<Image> {
        let key = (self.entry as *const Entry as usize, w, h);
        let mut cache = self.cache?.borrow_mut();
        cache
            .entry(key)
            .or_insert_with(|| {
                let full = match &self.entry.content {
                    // Decoding applies EXIF orientation, so the result is drawn upright.
                    Content::Embedded { bytes, .. } => crate::decode::decode_with_image_crate(bytes, None).ok()?,
                    Content::Decoded(decoded) => decoded.clone(),
                    Content::Pdf { .. } | Content::Svg(_) => return None,
                };
                Image::from_custom(full.resampled(w, h)?, false).ok()
            })
            .clone()
    }

    /// Draw the art into its frame, turning it 90° clockwise when the frame says so.
    fn draw_framed(&self, surface: &mut Surface, frame: &ArtFrame) {
        let r = frame.rect;
        let (w, h, transform) = if frame.rotated {
            (r.height, r.width, Transform::from_row(0.0, 1.0, -1.0, 0.0, (r.x + r.width) as f32, r.y as f32))
        } else {
            (r.width, r.height, Transform::from_translate(r.x as f32, r.y as f32))
        };
        surface.push_transform(&transform);
        self.draw_natural(surface, w as f32, h as f32);
        surface.pop();
    }

    /// Draw the art upright into `(0, 0, w, h)`.
    fn draw_natural(&self, surface: &mut Surface, w: f32, h: f32) {
        let Some(size) = Size::from_wh(w, h) else { return };
        if let Some(image) = self.limit.and_then(|limit| self.downsampled(limit)) {
            surface.draw_image(image, size);
            return;
        }
        if self.quality == Quality::Proxy
            && let Some(proxy) = self.entry.proxy() {
                surface.draw_image(proxy, size);
                return;
            }
        match &self.entry.content {
            Content::Pdf { document, .. } => surface.draw_pdf_page(document, size, self.page),
            Content::Svg(tree) => {
                // Text is outlined so output never depends on fonts the print shop lacks.
                let settings = SvgSettings { embed_text: false, ..SvgSettings::default() };
                surface.draw_svg(tree, size, settings);
            }
            Content::Embedded { image, exif_orientation, .. } => {
                draw_oriented(surface, image.clone(), *exif_orientation, w, h)
            }
            Content::Decoded(_) => {
                if let Some(image) = self.entry.print_image() {
                    surface.draw_image(image, size);
                }
            }
        }
    }
}

/// Draw stored pixels into an upright `w × h` box, applying their EXIF orientation as a transform
/// so the JPEG data itself is never re-encoded.
fn draw_oriented(surface: &mut Surface, image: Image, orientation: u8, w: f32, h: f32) {
    let (raw_w, raw_h) = if orientation >= 5 { (h, w) } else { (w, h) };
    let t = match orientation {
        2 => Transform::from_row(-1.0, 0.0, 0.0, 1.0, w, 0.0),
        3 => Transform::from_row(-1.0, 0.0, 0.0, -1.0, w, h),
        4 => Transform::from_row(1.0, 0.0, 0.0, -1.0, 0.0, h),
        5 => Transform::from_row(0.0, 1.0, 1.0, 0.0, 0.0, 0.0),
        6 => Transform::from_row(0.0, 1.0, -1.0, 0.0, w, 0.0),
        7 => Transform::from_row(0.0, -1.0, -1.0, 0.0, w, h),
        8 => Transform::from_row(0.0, -1.0, 1.0, 0.0, 0.0, h),
        _ => Transform::identity(),
    };
    let Some(size) = Size::from_wh(raw_w, raw_h) else { return };
    surface.push_transform(&t);
    surface.draw_image(image, size);
    surface.pop();
}

fn with_clip(surface: &mut Surface, rect: &Rect, draw: impl FnOnce(&mut Surface)) {
    let Some(path) = rect_path(rect) else { return };
    surface.push_clip_path(&path, &FillRule::NonZero);
    draw(surface);
    surface.pop();
}

fn rect_path(rect: &Rect) -> Option<krilla::geom::Path> {
    let mut builder = PathBuilder::new();
    builder.push_rect(krilla::geom::Rect::from_xywh(
        rect.x as f32,
        rect.y as f32,
        rect.width as f32,
        rect.height as f32,
    )?);
    builder.finish()
}

fn line_path(line: &Line) -> Option<krilla::geom::Path> {
    let mut builder = PathBuilder::new();
    builder.move_to(line.x1 as f32, line.y1 as f32);
    builder.line_to(line.x2 as f32, line.y2 as f32);
    builder.finish()
}

/// Render a single piece of art on its own, for list thumbnails.
pub(crate) fn thumbnail(entry: &Entry, page: usize, max_px: u32) -> Result<Bitmap, String> {
    let (w, h) = natural_size(entry, page).ok_or("page not found")?;
    let scale = max_px as f32 / w.max(h);
    render_art(entry, page, ((w * scale).round() as u32).max(1), ((h * scale).round() as u32).max(1), 0.0)
}

/// Rasterise one page of art, upright and unclipped, at exactly `width × height` pixels.
/// `overscan` draws the art that many pixels larger on every side, so the outermost pixels are
/// solid art rather than its anti-aliased edge (which bleed would mirror or stretch).
pub(crate) fn render_art(entry: &Entry, page: usize, width: u32, height: u32, overscan: f32) -> Result<Bitmap, String> {
    let (w, h) = (width as f32, height as f32);
    let mut document = Document::new();
    let mut pdf_page = document.start_page_with(PageSettings::from_wh(w, h).ok_or("empty art")?);
    let mut surface = pdf_page.surface();
    surface.push_transform(&Transform::from_translate(-overscan, -overscan));
    Art { entry, page, quality: Quality::Proxy, limit: None, cache: None }.draw_natural(
        &mut surface,
        w + 2.0 * overscan,
        h + 2.0 * overscan,
    );
    surface.pop();
    surface.finish();
    pdf_page.finish();
    rasterize(document.finish().map_err(|e| e.to_string())?, 1.0)
}

fn natural_size(entry: &Entry, page: usize) -> Option<(f32, f32)> {
    Some(match &entry.content {
        Content::Pdf { pages, .. } => *pages.get(page)?,
        Content::Svg(tree) => (tree.size().width(), tree.size().height()),
        Content::Embedded { image, exif_orientation, .. } => {
            let (w, h) = image.size();
            if *exif_orientation >= 5 { (h as f32, w as f32) } else { (w as f32, h as f32) }
        }
        Content::Decoded(d) => (d.width as f32, d.height as f32),
    })
}

/// Rasterise the first page of a PDF with hayro.
fn rasterize(bytes: Vec<u8>, pixels_per_point: f64) -> Result<Bitmap, String> {
    use hayro::hayro_interpret::InterpreterSettings;
    use hayro::vello_cpu::color::palette::css::WHITE;
    use hayro::{PixmapSettings, RenderCache, RenderSettings};

    let pdf = hayro::hayro_syntax::Pdf::new(Arc::new(bytes)).map_err(|e| format!("{e:?}"))?;
    let page = pdf.pages().first().ok_or("empty PDF")?;
    let (w, h) = page.render_dimensions();
    // Keep the bitmap within what a preview needs and hayro's u16 limits.
    let scale = (pixels_per_point as f32).min(4096.0 / w.max(h)).max(0.01);
    let pixmap = hayro::render(
        page,
        &RenderCache::new(),
        &InterpreterSettings::default(),
        &RenderSettings::default(),
        &PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE },
    );
    Ok(Bitmap {
        width: pixmap.width() as u32,
        height: pixmap.height() as u32,
        // Opaque white background, so premultiplied and straight alpha are identical.
        rgba: pixmap.data_as_u8_slice().to_vec(),
    })
}
