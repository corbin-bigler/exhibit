//! Turning the workspace into sheets: which art goes in which slot, and every mark to draw.

use crate::art::{ArtFrame, effective_dpi, frame_art};
use crate::geometry::{Line, Rect, Sides, Size};
use crate::items::{ItemId, PageArt, SourceId};
use crate::layout::{SlotLayout, layout_slots};
use crate::marks::{bleed_sides, crop_marks};
use crate::settings::{BleedMode, Scaling};
use crate::workspace::Workspace;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArtRef {
    pub item: ItemId,
    pub source: SourceId,
    pub art: PageArt,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub art_ref: ArtRef,
    pub trim: Rect,
    pub bleed: Sides,
    pub frame: ArtFrame,
    pub bleed_mode: BleedMode,
}

impl Placement {
    /// Everything this placement draws stays inside this rectangle.
    pub fn clip(&self) -> Rect {
        self.trim.outset(self.bleed)
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct SheetJob {
    pub placements: Vec<Placement>,
    pub crop_marks: Vec<Line>,
    pub trim_lines: Vec<Rect>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PrintJob {
    pub sheet: Size,
    pub sheets: Vec<SheetJob>,
}

/// Every copy of every page, in print order. Auto counts fill one sheet per page.
pub fn sequence(workspace: &Workspace, per_sheet: usize) -> Vec<ArtRef> {
    let mut sequence = Vec::new();
    if per_sheet == 0 {
        return sequence;
    }
    for item in workspace.items() {
        let copies = item.copies.map(|c| c as usize).unwrap_or(per_sheet);
        for art in &item.pages {
            let art_ref = ArtRef { item: item.id, source: item.source, art: *art };
            sequence.extend(std::iter::repeat_n(art_ref, copies));
        }
    }
    sequence
}

pub fn build_print_job(workspace: &Workspace) -> PrintJob {
    let settings = &workspace.settings;
    let layout = layout_slots(settings);
    let sheet_rect = Rect::new(0.0, 0.0, settings.sheet.width, settings.sheet.height);
    let trims: Vec<Rect> = layout.slots.iter().map(|s| s.trim).collect();
    let bleeds: Vec<Sides> = (0..trims.len())
        .map(|i| bleed_sides(&trims, i, settings.bleed.width, sheet_rect))
        .collect();

    let sheets = sequence(workspace, layout.per_sheet())
        .chunks(layout.per_sheet().max(1))
        .map(|chunk| {
            let placements: Vec<Placement> = chunk
                .iter()
                .zip(trims.iter().zip(&bleeds))
                .map(|(art_ref, (trim, bleed))| {
                    let target = match settings.bleed.mode {
                        BleedMode::Scale => trim.outset(*bleed),
                        BleedMode::Mirror | BleedMode::Edge => *trim,
                    };
                    Placement {
                        art_ref: *art_ref,
                        trim: *trim,
                        bleed: *bleed,
                        frame: frame_art(art_ref.art.size, target, settings.scaling),
                        bleed_mode: settings.bleed.mode,
                    }
                })
                .collect();
            let used_trims: Vec<Rect> = placements.iter().map(|p| p.trim).collect();
            let used_clips: Vec<Rect> = placements.iter().map(|p| p.clip()).collect();
            SheetJob {
                crop_marks: crop_marks(&used_trims, &used_clips, settings.marks.crop),
                trim_lines: if settings.marks.trim_lines { used_trims } else { Vec::new() },
                placements,
            }
        })
        .collect();

    PrintJob { sheet: Size::new(settings.sheet.width, settings.sheet.height), sheets }
}

/// Effective resolution of an item's first page at the current slot size.
pub fn item_dpi(layout: &SlotLayout, art: &PageArt, scaling: Scaling) -> Option<f64> {
    let slot = layout.slots.first()?;
    effective_dpi(art.kind, &frame_art(art.size, slot.trim, scaling))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::{ArtKind, ImportedSource, SourceFormat};

    fn add(ws: &mut Workspace, id: u64, pages: usize) -> ItemId {
        ws.add(ImportedSource {
            id: SourceId(id),
            name: format!("{id}"),
            format: SourceFormat::Pdf,
            pages: (0..pages)
                .map(|page| PageArt { page, size: Size::new(360.0, 504.0), kind: ArtKind::Vector })
                .collect(),
        })
    }

    // I-01, P-04
    #[test]
    fn auto_count_fills_one_sheet_per_page() {
        let mut ws = Workspace::default(); // 5×7 on 12×18 → 4 up
        add(&mut ws, 1, 1);
        add(&mut ws, 2, 3);
        let job = build_print_job(&ws);
        assert_eq!(job.sheets.len(), 4);
        assert!(job.sheets.iter().all(|s| s.placements.len() == 4));
        let pages: Vec<usize> = job.sheets[1..].iter().map(|s| s.placements[0].art_ref.art.page).collect();
        assert_eq!(pages, vec![0, 1, 2]);
    }

    // I-02
    #[test]
    fn overrides_flow_across_sheets() {
        let mut ws = Workspace::default();
        let a = add(&mut ws, 1, 1);
        add(&mut ws, 2, 1);
        ws.set_copies(a, Some(6)).unwrap();
        let job = build_print_job(&ws);
        // 6 + 4 = 10 items → 3 sheets, the last one half full.
        assert_eq!(job.sheets.len(), 3);
        assert_eq!(job.sheets[1].placements[1].art_ref.source, SourceId(1));
        assert_eq!(job.sheets[1].placements[2].art_ref.source, SourceId(2));
        assert_eq!(job.sheets[2].placements.len(), 2);
    }

    // M-04
    #[test]
    fn trim_lines_follow_the_setting() {
        let mut ws = Workspace::default();
        add(&mut ws, 1, 1);
        assert!(build_print_job(&ws).sheets[0].trim_lines.is_empty());
        ws.settings.marks.trim_lines = true;
        assert_eq!(build_print_job(&ws).sheets[0].trim_lines.len(), 4);
    }

    // B-05
    #[test]
    fn scale_bleed_frames_art_to_the_bleed_box() {
        let mut ws = Workspace::default();
        add(&mut ws, 1, 1);
        ws.settings.bleed.mode = BleedMode::Scale;
        let p = build_print_job(&ws).sheets[0].placements[0];
        assert!(p.frame.rect.width >= p.clip().width - 1e-9);
        assert!(p.frame.rect.height >= p.clip().height - 1e-9);
    }
}
