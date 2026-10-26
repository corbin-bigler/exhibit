//! Bleed extents and printer's marks.

use crate::geometry::{EPSILON, Line, Rect, Sides};
use crate::settings::CropMarkSettings;

/// Line weight of crop marks and trim lines, in points.
pub const MARK_WEIGHT: f64 = 0.25;

/// How far bleed may extend past each edge of `trims[index]`: the full width on open sides,
/// half the gap on sides facing a neighbour, and never past the sheet.
pub fn bleed_sides(trims: &[Rect], index: usize, width: f64, sheet: Rect) -> Sides {
    let me = trims[index];
    let mut sides = Sides {
        left: width.min(me.x - sheet.x),
        top: width.min(me.y - sheet.y),
        right: width.min(sheet.right() - me.right()),
        bottom: width.min(sheet.bottom() - me.bottom()),
    };
    for (i, other) in trims.iter().enumerate() {
        if i == index {
            continue;
        }
        if me.overlaps_vertically(other) {
            if other.right() <= me.x + EPSILON {
                sides.left = sides.left.min((me.x - other.right()) / 2.0);
            }
            if other.x >= me.right() - EPSILON {
                sides.right = sides.right.min((other.x - me.right()) / 2.0);
            }
        }
        if me.overlaps_horizontally(other) {
            if other.bottom() <= me.y + EPSILON {
                sides.top = sides.top.min((me.y - other.bottom()) / 2.0);
            }
            if other.y >= me.bottom() - EPSILON {
                sides.bottom = sides.bottom.min((other.y - me.bottom()) / 2.0);
            }
        }
    }
    Sides {
        left: sides.left.max(0.0),
        top: sides.top.max(0.0),
        right: sides.right.max(0.0),
        bottom: sides.bottom.max(0.0),
    }
}

/// Marks for every cut line, all placed outside the box the items and their bleed occupy, so
/// mixed layouts never get a mark inside the artwork.
pub fn crop_marks(trims: &[Rect], bleeds: &[Rect], settings: CropMarkSettings) -> Vec<Line> {
    if !settings.enabled || trims.is_empty() || settings.length <= 0.0 {
        return Vec::new();
    }
    let zone = bleeds.iter().skip(1).fold(bleeds[0], |acc, r| acc.union(r));
    let xs = distinct(trims.iter().flat_map(|t| [t.x, t.right()]));
    let ys = distinct(trims.iter().flat_map(|t| [t.y, t.bottom()]));
    let (near, far) = (settings.offset, settings.offset + settings.length);

    let mut lines = Vec::with_capacity(2 * (xs.len() + ys.len()));
    for &x in &xs {
        lines.push(Line { x1: x, y1: zone.y - far, x2: x, y2: zone.y - near });
        lines.push(Line { x1: x, y1: zone.bottom() + near, x2: x, y2: zone.bottom() + far });
    }
    for &y in &ys {
        lines.push(Line { x1: zone.x - far, y1: y, x2: zone.x - near, y2: y });
        lines.push(Line { x1: zone.right() + near, y1: y, x2: zone.right() + far, y2: y });
    }
    lines
}

fn distinct(values: impl Iterator<Item = f64>) -> Vec<f64> {
    let mut values: Vec<f64> = values.collect();
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    values.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::inches;

    fn grid() -> Vec<Rect> {
        // Two 5×7 side by side with a 1/8" gutter.
        vec![
            Rect::new(inches(1.0), inches(1.0), inches(5.0), inches(7.0)),
            Rect::new(inches(6.125), inches(1.0), inches(5.0), inches(7.0)),
        ]
    }

    // B-01, B-02
    #[test]
    fn inner_bleed_is_capped_at_half_the_gutter() {
        let sheet = Rect::new(0.0, 0.0, inches(12.0), inches(18.0));
        let sides = bleed_sides(&grid(), 0, inches(0.125), sheet);
        assert!((sides.left - inches(0.125)).abs() < 1e-9);
        assert!((sides.right - inches(0.0625)).abs() < 1e-9);
        assert!((sides.top - inches(0.125)).abs() < 1e-9);
    }

    #[test]
    fn bleed_never_leaves_the_sheet() {
        let sheet = Rect::new(0.0, 0.0, inches(12.0), inches(18.0));
        let trims = [Rect::new(inches(0.05), 0.0, inches(5.0), inches(7.0))];
        let sides = bleed_sides(&trims, 0, inches(0.125), sheet);
        assert!((sides.left - inches(0.05)).abs() < 1e-9);
        assert_eq!(sides.top, 0.0);
    }

    // M-02, M-03
    #[test]
    fn marks_sit_outside_the_bleed_zone_at_every_cut() {
        let trims = grid();
        let bleed = Sides::uniform(inches(0.125));
        let bleeds: Vec<Rect> = trims.iter().map(|t| t.outset(bleed)).collect();
        let settings = CropMarkSettings { enabled: true, length: inches(0.25), offset: inches(0.0625) };
        let marks = crop_marks(&trims, &bleeds, settings);
        // 4 distinct x cuts × top/bottom + 2 distinct y cuts × left/right.
        assert_eq!(marks.len(), 4 * 2 + 2 * 2);
        let top = marks[0];
        assert!((top.x1 - inches(1.0)).abs() < 1e-9);
        assert!((top.y2 - (inches(1.0) - inches(0.125) - inches(0.0625))).abs() < 1e-9);
        assert!((top.y2 - top.y1 - inches(0.25)).abs() < 1e-9);
        let zone = bleeds[0].union(&bleeds[1]);
        for m in &marks {
            let inside = |x: f64, y: f64| x > zone.x + 1e-6 && x < zone.right() - 1e-6 && y > zone.y + 1e-6 && y < zone.bottom() - 1e-6;
            assert!(!inside(m.x1, m.y1) && !inside(m.x2, m.y2));
        }
        // M-01
        let off = CropMarkSettings { enabled: false, ..settings };
        assert!(crop_marks(&trims, &bleeds, off).is_empty());
    }
}
