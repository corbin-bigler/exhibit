//! The on-screen preview, composited pixel by pixel from the print job's geometry.
//!
//! Each piece of art is rasterised once at its slot size; every sheet pixel then looks up the art
//! pixel it shows, directly inside the trim, reflected about the trim (Mirror) or clamped to it
//! (Edge). Drawing through clip paths instead leaves anti-aliased hairlines at every trim edge.

use std::collections::HashMap;

use exhibit_domain::geometry::{Line, Rect};
use exhibit_domain::job::{Placement, PrintJob};
use exhibit_domain::marks::MARK_WEIGHT;
use exhibit_domain::ports::Bitmap;
use exhibit_domain::settings::BleedMode;

use crate::library::SourceStore;
use crate::render::render_art;

/// Largest preview side in pixels.
const MAX_PREVIEW_PX: f64 = 4096.0;

/// Art is rendered this many pixels oversize on each side, so its outermost pixels are solid
/// rather than anti-aliased against white; bleed reflects and stretches those pixels.
const ART_OVERSCAN: f32 = 1.0;

struct Canvas {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Canvas {
    fn white(width: u32, height: u32) -> Self {
        Self { width, height, rgba: vec![255; (width * height * 4) as usize] }
    }

    fn put(&mut self, x: u32, y: u32, color: [u8; 4]) {
        let at = ((y * self.width + x) * 4) as usize;
        let a = color[3] as u32;
        for (dst, src) in self.rgba[at..at + 3].iter_mut().zip(&color[..3]) {
            *dst = ((*src as u32 * a + *dst as u32 * (255 - a)) / 255) as u8;
        }
        self.rgba[at + 3] = 255;
    }

    fn fill(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, color: [u8; 4]) {
        let clamp = |v: f64, max: u32| v.max(0.0).min(max as f64) as u32;
        for y in clamp(y0.floor(), self.height)..clamp(y1.ceil(), self.height) {
            for x in clamp(x0.floor(), self.width)..clamp(x1.ceil(), self.width) {
                self.put(x, y, color);
            }
        }
    }
}

/// Art rendered at slot size, with the turn already applied.
struct ArtPixels {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl ArtPixels {
    fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]]
    }
}

pub(crate) fn render_preview(store: &SourceStore, job: &PrintJob, sheet: usize, pixels_per_point: f64) -> Result<Bitmap, String> {
    let scale = pixels_per_point.min(MAX_PREVIEW_PX / job.sheet.width.max(job.sheet.height)).max(0.01);
    let mut canvas = Canvas::white(
        ((job.sheet.width * scale).round() as u32).max(1),
        ((job.sheet.height * scale).round() as u32).max(1),
    );
    let Some(sheet) = job.sheets.get(sheet) else {
        return Ok(Bitmap { width: canvas.width, height: canvas.height, rgba: canvas.rgba });
    };

    let mut arts: HashMap<(u64, usize, u32, u32, bool), ArtPixels> = HashMap::new();
    for p in &sheet.placements {
        let fw = ((p.frame.rect.width * scale).round() as u32).max(1);
        let fh = ((p.frame.rect.height * scale).round() as u32).max(1);
        let key = (p.art_ref.source.0, p.art_ref.art.page, fw, fh, p.frame.rotated);
        if let std::collections::hash_map::Entry::Vacant(slot) = arts.entry(key) {
            let entry = store.get(p.art_ref.source).ok_or("a file used on this sheet was removed")?;
            slot.insert(if p.frame.rotated {
                rotate_clockwise(render_art(&entry, p.art_ref.art.page, fh, fw, ART_OVERSCAN)?)
            } else {
                let b = render_art(&entry, p.art_ref.art.page, fw, fh, ART_OVERSCAN)?;
                ArtPixels { width: b.width, height: b.height, rgba: b.rgba }
            });
        }
        paint_placement(&mut canvas, p, &arts[&key], scale);
    }

    let weight = (MARK_WEIGHT * scale).max(1.0);
    let black = [0, 0, 0, 255];
    for r in &sheet.trim_lines {
        let rect = |x: f64, y: f64, w: f64, h: f64| Rect::new(x, y, w, h);
        for edge in [
            rect(r.x, r.y, r.width, 0.0),
            rect(r.x, r.bottom(), r.width, 0.0),
            rect(r.x, r.y, 0.0, r.height),
            rect(r.right(), r.y, 0.0, r.height),
        ] {
            stroke(&mut canvas, &edge_line(&edge), scale, weight, black);
        }
    }
    for line in &sheet.crop_marks {
        stroke(&mut canvas, line, scale, weight, black);
    }
    Ok(Bitmap { width: canvas.width, height: canvas.height, rgba: canvas.rgba })
}

fn paint_placement(canvas: &mut Canvas, p: &Placement, art: &ArtPixels, scale: f64) {
    let clip = p.clip();
    let t = p.trim;
    let frame = p.frame.rect;
    let x0 = (clip.x * scale).floor().max(0.0) as u32;
    let y0 = (clip.y * scale).floor().max(0.0) as u32;
    let x1 = ((clip.right() * scale).ceil() as u32).min(canvas.width);
    let y1 = ((clip.bottom() * scale).ceil() as u32).min(canvas.height);
    let reflect = |v: f64, lo: f64, hi: f64| if v < lo { 2.0 * lo - v } else if v > hi { 2.0 * hi - v } else { v };
    let clamp = |v: f64, lo: f64, hi: f64| v.max(lo).min(hi - 1e-6);

    for py in y0..y1 {
        let y = (py as f64 + 0.5) / scale;
        if y < clip.y || y >= clip.bottom() {
            continue;
        }
        for px in x0..x1 {
            let x = (px as f64 + 0.5) / scale;
            if x < clip.x || x >= clip.right() {
                continue;
            }
            let (u, v) = match p.bleed_mode {
                BleedMode::Scale => (x, y),
                BleedMode::Mirror => (reflect(x, t.x, t.right()), reflect(y, t.y, t.bottom())),
                BleedMode::Edge => (clamp(x, t.x, t.right()), clamp(y, t.y, t.bottom())),
            };
            let inside = |w: f64, lo: f64, len: f64| w >= lo - 1e-6 && w <= lo + len + 1e-6;
            if !inside(u, frame.x, frame.width) || !inside(v, frame.y, frame.height) {
                continue; // Fit leaves blank paper around the art.
            }
            // A point exactly on the art's far edge belongs to its last pixel.
            let index = |w: f64, lo: f64, len: f64, n: u32| (((w - lo) / len * n as f64).floor().max(0.0) as u32).min(n - 1);
            let ax = index(u, frame.x, frame.width, art.width);
            let ay = index(v, frame.y, frame.height, art.height);
            canvas.put(px, py, art.at(ax, ay));
        }
    }
}

fn rotate_clockwise(b: Bitmap) -> ArtPixels {
    let (w, h) = (b.width, b.height);
    let mut rgba = vec![0u8; b.rgba.len()];
    for y in 0..h {
        for x in 0..w {
            // Source (x, y) lands at (h - 1 - y, x) in the turned image, which is h wide.
            let src = ((y * w + x) * 4) as usize;
            let dst = ((x * h + (h - 1 - y)) * 4) as usize;
            rgba[dst..dst + 4].copy_from_slice(&b.rgba[src..src + 4]);
        }
    }
    ArtPixels { width: h, height: w, rgba }
}

fn edge_line(r: &Rect) -> Line {
    Line { x1: r.x, y1: r.y, x2: r.right(), y2: r.bottom() }
}

/// Axis-aligned lines only, which is all marks ever are.
fn stroke(canvas: &mut Canvas, line: &Line, scale: f64, weight: f64, color: [u8; 4]) {
    let half = weight / 2.0;
    if (line.x1 - line.x2).abs() < 1e-9 {
        let x = line.x1 * scale;
        canvas.fill(x - half, line.y1.min(line.y2) * scale, x + half, line.y1.max(line.y2) * scale, color);
    } else {
        let y = line.y1 * scale;
        canvas.fill(line.x1.min(line.x2) * scale, y - half, line.x1.max(line.x2) * scale, y + half, color);
    }
}
