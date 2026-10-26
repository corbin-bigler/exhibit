//! Where the slots go on a sheet.

use crate::geometry::{EPSILON, Rect, Size};
use crate::settings::{JobSettings, LayoutMode};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    /// The cut rectangle.
    pub trim: Rect,
    /// Whether the slot is the item size turned 90°.
    pub rotated: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SlotLayout {
    /// The item size as entered (or derived, in grid mode), unrotated.
    pub item: Size,
    /// Slots in reading order: top to bottom, then left to right.
    pub slots: Vec<Slot>,
}

impl SlotLayout {
    pub fn per_sheet(&self) -> usize {
        self.slots.len()
    }
}

/// A uniform grid of one orientation. A layout is one band, or two stacked bands when
/// orientations are mixed.
#[derive(Clone, Copy, Debug)]
struct Band {
    cell: Size,
    rotated: bool,
    cols: u32,
    rows: u32,
}

impl Band {
    fn count(&self) -> u32 {
        self.cols * self.rows
    }

    fn extent(&self, gutter: f64) -> Size {
        Size::new(span(self.cols, self.cell.width, gutter), span(self.rows, self.cell.height, gutter))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stack {
    Vertical,
    Horizontal,
}

pub fn layout_slots(settings: &JobSettings) -> SlotLayout {
    let sheet = settings.sheet;
    let area = Rect::new(
        sheet.margin,
        sheet.margin,
        sheet.width - 2.0 * sheet.margin,
        sheet.height - 2.0 * sheet.margin,
    );
    let gutter = settings.gutter.max(0.0);

    let (item, bands, stack) = match settings.layout {
        LayoutMode::ByGrid { across, down } => {
            let item = Size::new(
                (area.width + gutter) / across.max(1) as f64 - gutter,
                (area.height + gutter) / down.max(1) as f64 - gutter,
            );
            if across == 0 || down == 0 || item.width <= EPSILON || item.height <= EPSILON {
                return SlotLayout { item, slots: Vec::new() };
            }
            (item, vec![Band { cell: item, rotated: false, cols: across, rows: down }], Stack::Vertical)
        }
        LayoutMode::BySize { item, mix_orientations } => {
            let (bands, stack) = best_bands(item, area.size(), gutter, mix_orientations);
            (item, bands, stack)
        }
    };

    if area.width <= EPSILON || area.height <= EPSILON {
        return SlotLayout { item, slots: Vec::new() };
    }
    SlotLayout { item, slots: place_bands(&bands, stack, area, gutter) }
}

/// How many cells of `cell` fit along `available`, with `gutter` between them.
pub fn fit_count(available: f64, cell: f64, gutter: f64) -> u32 {
    if cell <= EPSILON || available + EPSILON < cell {
        return 0;
    }
    ((available + gutter + EPSILON) / (cell + gutter)).floor() as u32
}

fn span(count: u32, cell: f64, gutter: f64) -> f64 {
    if count == 0 { 0.0 } else { count as f64 * cell + (count - 1) as f64 * gutter }
}

fn uniform_band(cell: Size, rotated: bool, area: Size, gutter: f64) -> Band {
    Band {
        cell,
        rotated,
        cols: fit_count(area.width, cell.width, gutter),
        rows: fit_count(area.height, cell.height, gutter),
    }
}

fn best_bands(item: Size, area: Size, gutter: f64, mix: bool) -> (Vec<Band>, Stack) {
    let upright = uniform_band(item, false, area, gutter);
    let turned = uniform_band(item.transposed(), true, area, gutter);
    // Ties keep the orientation the user typed.
    let uniform = if turned.count() > upright.count() { turned } else { upright };
    let mut best_count = uniform.count();
    let mut best_layout = (vec![uniform], Stack::Vertical);
    if !mix {
        return best_layout;
    }

    // Guillotine search: some rows (or columns) of one orientation, the rest of the other.
    for (first, second) in [(item, item.transposed()), (item.transposed(), item)] {
        let first_rotated = first != item;
        for stack in [Stack::Vertical, Stack::Horizontal] {
            let along = |s: Size| if stack == Stack::Vertical { s.height } else { s.width };
            let across_len = |s: Size| if stack == Stack::Vertical { s.width } else { s.height };
            let lanes_first = fit_count(across_len(area), across_len(first), gutter);
            let max_rows = fit_count(along(area), along(first), gutter);
            for rows in 1..=max_rows {
                let used = span(rows, along(first), gutter) + gutter;
                let remaining = along(area) - used;
                let lanes_second = fit_count(across_len(area), across_len(second), gutter);
                let rows_second = fit_count(remaining, along(second), gutter);
                let total = lanes_first * rows + lanes_second * rows_second;
                if total > best_count && rows_second > 0 {
                    let make = |cell: Size, rotated: bool, lanes: u32, rows: u32| match stack {
                        Stack::Vertical => Band { cell, rotated, cols: lanes, rows },
                        Stack::Horizontal => Band { cell, rotated, cols: rows, rows: lanes },
                    };
                    let a = make(first, first_rotated, lanes_first, rows);
                    let b = make(second, !first_rotated, lanes_second, rows_second);
                    best_count = total;
                    best_layout = (vec![a, b], stack);
                }
            }
        }
    }
    best_layout
}

fn place_bands(bands: &[Band], stack: Stack, area: Rect, gutter: f64) -> Vec<Slot> {
    let bands: Vec<Band> = bands.iter().copied().filter(|b| b.count() > 0).collect();
    if bands.is_empty() {
        return Vec::new();
    }
    let extents: Vec<Size> = bands.iter().map(|b| b.extent(gutter)).collect();
    let block = match stack {
        Stack::Vertical => Size::new(
            extents.iter().map(|e| e.width).fold(0.0, f64::max),
            extents.iter().map(|e| e.height).sum::<f64>() + gutter * (bands.len() - 1) as f64,
        ),
        Stack::Horizontal => Size::new(
            extents.iter().map(|e| e.width).sum::<f64>() + gutter * (bands.len() - 1) as f64,
            extents.iter().map(|e| e.height).fold(0.0, f64::max),
        ),
    };
    let origin_x = area.x + (area.width - block.width) / 2.0;
    let origin_y = area.y + (area.height - block.height) / 2.0;

    let mut slots = Vec::new();
    let mut cursor = 0.0;
    for (band, extent) in bands.iter().zip(&extents) {
        // Each band is centred across the block, so a narrower band sits in the middle.
        let (bx, by) = match stack {
            Stack::Vertical => (origin_x + (block.width - extent.width) / 2.0, origin_y + cursor),
            Stack::Horizontal => (origin_x + cursor, origin_y + (block.height - extent.height) / 2.0),
        };
        for row in 0..band.rows {
            for col in 0..band.cols {
                slots.push(Slot {
                    trim: Rect::new(
                        bx + col as f64 * (band.cell.width + gutter),
                        by + row as f64 * (band.cell.height + gutter),
                        band.cell.width,
                        band.cell.height,
                    ),
                    rotated: band.rotated,
                });
            }
        }
        cursor += match stack {
            Stack::Vertical => extent.height,
            Stack::Horizontal => extent.width,
        } + gutter;
    }
    slots.sort_by(|a, b| {
        (a.trim.y, a.trim.x).partial_cmp(&(b.trim.y, b.trim.x)).unwrap_or(std::cmp::Ordering::Equal)
    });
    slots
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SheetSettings;
    use crate::units::inches;

    fn settings(sheet: (f64, f64), margin: f64, gutter: f64, layout: LayoutMode) -> JobSettings {
        JobSettings {
            sheet: SheetSettings { width: inches(sheet.0), height: inches(sheet.1), margin: inches(margin) },
            layout,
            gutter: inches(gutter),
            ..JobSettings::default()
        }
    }

    fn by_size(w: f64, h: f64, mix: bool) -> LayoutMode {
        LayoutMode::BySize { item: Size::new(inches(w), inches(h)), mix_orientations: mix }
    }

    // L-01, L-06
    #[test]
    fn five_by_seven_on_twelve_by_eighteen_is_two_by_two_centred() {
        let layout = layout_slots(&settings((12.0, 18.0), 0.25, 0.125, by_size(5.0, 7.0, false)));
        assert_eq!(layout.per_sheet(), 4);
        let first = layout.slots[0].trim;
        assert!((first.x - inches(0.9375)).abs() < 1e-6);
        assert!((first.y - inches(1.9375)).abs() < 1e-6);
        let second = layout.slots[1].trim;
        assert!((second.x - first.right() - inches(0.125)).abs() < 1e-6, "gutter between columns");
        assert!(layout.slots.iter().all(|s| !s.rotated));
    }

    // L-02
    #[test]
    fn turns_the_grid_when_that_fits_more() {
        let layout = layout_slots(&settings((8.5, 11.0), 0.25, 0.0, by_size(2.0, 3.5, false)));
        // upright 2×3.5: 4 across × 3 down = 12; turned 3.5×2: 2 across × 5 down = 10.
        assert_eq!(layout.per_sheet(), 12);
        let layout = layout_slots(&settings((8.5, 11.0), 0.25, 0.0, by_size(3.5, 2.0, false)));
        assert_eq!(layout.per_sheet(), 12);
        assert!(layout.slots.iter().all(|s| s.rotated));
    }

    // L-03
    #[test]
    fn mixing_orientations_fits_three_four_by_sixes_on_letter() {
        let off = layout_slots(&settings((8.5, 11.0), 0.125, 0.125, by_size(4.0, 6.0, false)));
        assert_eq!(off.per_sheet(), 2);
        let on = layout_slots(&settings((8.5, 11.0), 0.125, 0.125, by_size(4.0, 6.0, true)));
        assert_eq!(on.per_sheet(), 3);
        assert_eq!(on.slots.iter().filter(|s| s.rotated).count(), 1);
        for (i, a) in on.slots.iter().enumerate() {
            for b in &on.slots[i + 1..] {
                assert!(!(a.trim.overlaps_horizontally(&b.trim) && a.trim.overlaps_vertically(&b.trim)));
            }
        }
    }

    #[test]
    fn mixing_never_does_worse_than_uniform() {
        for (w, h) in [(5.0, 7.0), (2.0, 3.5), (4.0, 6.0), (3.0, 3.0), (8.5, 11.0)] {
            let off = layout_slots(&settings((12.0, 18.0), 0.25, 0.125, by_size(w, h, false)));
            let on = layout_slots(&settings((12.0, 18.0), 0.25, 0.125, by_size(w, h, true)));
            assert!(on.per_sheet() >= off.per_sheet());
        }
    }

    // L-04
    #[test]
    fn grid_mode_derives_the_item_size() {
        let layout = layout_slots(&settings(
            (12.0, 18.0),
            0.25,
            0.125,
            LayoutMode::ByGrid { across: 2, down: 3 },
        ));
        assert_eq!(layout.per_sheet(), 6);
        assert!((layout.item.width - inches((11.5 + 0.125) / 2.0 - 0.125)).abs() < 1e-6);
        let last = layout.slots.last().unwrap().trim;
        assert!((last.right() - inches(11.75)).abs() < 1e-6);
        assert!((last.bottom() - inches(17.75)).abs() < 1e-6);
    }

    // L-10
    #[test]
    fn nothing_fits_gives_no_slots() {
        assert_eq!(layout_slots(&settings((8.5, 11.0), 0.25, 0.125, by_size(20.0, 20.0, true))).per_sheet(), 0);
        assert_eq!(layout_slots(&settings((8.5, 11.0), 5.0, 0.125, by_size(1.0, 1.0, false))).per_sheet(), 0);
        let grid = LayoutMode::ByGrid { across: 0, down: 2 };
        assert_eq!(layout_slots(&settings((8.5, 11.0), 0.25, 0.125, grid)).per_sheet(), 0);
    }
}
