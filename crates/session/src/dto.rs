//! Shapes the UI sends and receives. Lengths are points; the UI converts for display.

use exhibit_domain::geometry::{Rect, Size};
use exhibit_domain::items::{ArtKind, Item};
use exhibit_domain::job::{HeavySheet, item_dpi};
use exhibit_domain::layout::SlotLayout;
use exhibit_domain::settings::{
    BleedMode, BleedSettings, CropMarkSettings, JobSettings, LayoutMode, MarkSettings, SHEET_PRESETS, Scaling,
    SheetSettings,
};
use exhibit_domain::usecases::SheetPlan;
use exhibit_domain::workspace::Workspace;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
    pub sheet_width: f64,
    pub sheet_height: f64,
    pub margin: f64,
    /// "size" or "grid".
    pub mode: String,
    pub item_width: f64,
    pub item_height: f64,
    pub mix_orientations: bool,
    pub across: u32,
    pub down: u32,
    pub gutter: f64,
    /// "fill" or "fit".
    pub scaling: String,
    pub bleed_width: f64,
    /// "mirror", "edge" or "scale".
    pub bleed_mode: String,
    pub crop_marks: bool,
    pub mark_length: f64,
    pub mark_offset: f64,
    pub trim_lines: bool,
}

impl SettingsDto {
    fn from_domain(s: &JobSettings, layout: &SlotLayout) -> Self {
        let (across, down) = grid_counts(layout);
        let (mode, mix) = match s.layout {
            LayoutMode::BySize { mix_orientations, .. } => ("size", mix_orientations),
            LayoutMode::ByGrid { .. } => ("grid", false),
        };
        Self {
            sheet_width: s.sheet.width,
            sheet_height: s.sheet.height,
            margin: s.sheet.margin,
            mode: mode.into(),
            item_width: layout.item.width,
            item_height: layout.item.height,
            mix_orientations: mix,
            across,
            down,
            gutter: s.gutter,
            scaling: match s.scaling {
                Scaling::Fill => "fill",
                Scaling::Fit => "fit",
            }
            .into(),
            bleed_width: s.bleed.width,
            bleed_mode: match s.bleed.mode {
                BleedMode::Mirror => "mirror",
                BleedMode::Edge => "edge",
                BleedMode::Scale => "scale",
            }
            .into(),
            crop_marks: s.marks.crop.enabled,
            mark_length: s.marks.crop.length,
            mark_offset: s.marks.crop.offset,
            trim_lines: s.marks.trim_lines,
        }
    }

    pub fn to_domain(&self) -> Result<JobSettings, String> {
        Ok(JobSettings {
            sheet: SheetSettings { width: self.sheet_width, height: self.sheet_height, margin: self.margin },
            layout: match self.mode.as_str() {
                "grid" => LayoutMode::ByGrid { across: self.across, down: self.down },
                "size" => LayoutMode::BySize {
                    item: Size::new(self.item_width, self.item_height),
                    mix_orientations: self.mix_orientations,
                },
                other => return Err(format!("unknown layout mode {other}")),
            },
            gutter: self.gutter,
            scaling: match self.scaling.as_str() {
                "fit" => Scaling::Fit,
                _ => Scaling::Fill,
            },
            bleed: BleedSettings {
                width: self.bleed_width,
                mode: match self.bleed_mode.as_str() {
                    "edge" => BleedMode::Edge,
                    "scale" => BleedMode::Scale,
                    _ => BleedMode::Mirror,
                },
            },
            marks: MarkSettings {
                crop: CropMarkSettings { enabled: self.crop_marks, length: self.mark_length, offset: self.mark_offset },
                trim_lines: self.trim_lines,
            },
        })
    }
}

/// Columns and rows of a uniform layout, used to seed grid mode with what currently fits.
fn grid_counts(layout: &SlotLayout) -> (u32, u32) {
    let distinct = |values: Vec<f64>| {
        let mut values = values;
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        values.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
        values.len() as u32
    };
    let xs = distinct(layout.slots.iter().map(|s| s.trim.x).collect());
    let ys = distinct(layout.slots.iter().map(|s| s.trim.y).collect());
    (xs.max(1), ys.max(1))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RectDto {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl From<Rect> for RectDto {
    fn from(r: Rect) -> Self {
        Self { x: r.x, y: r.y, width: r.width, height: r.height }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDto {
    pub id: u64,
    pub source: u64,
    /// Page index used for the thumbnail.
    pub page: usize,
    pub name: String,
    pub spec: String,
    pub is_pack: bool,
    pub vector: bool,
    pub dpi: Option<f64>,
    pub copies: Option<u32>,
}

impl ItemDto {
    fn from_domain(item: &Item, layout: &SlotLayout, scaling: Scaling) -> Self {
        let first = item.pages[0];
        let spec = match (item.is_pack(), first.kind) {
            (true, _) => format!("{} · {} pages", item.format.label(), item.pages.len()),
            (false, ArtKind::Raster { width_px, height_px }) => {
                format!("{} · {} × {}", item.format.label(), width_px, height_px)
            }
            (false, ArtKind::Vector) if item.format.label() == "PDF" => format!("PDF · p. {}", first.page + 1),
            (false, ArtKind::Vector) => item.format.label().to_string(),
        };
        Self {
            id: item.id.0,
            source: item.source.0,
            page: first.page,
            name: item.name.clone(),
            spec,
            is_pack: item.is_pack(),
            vector: first.kind == ArtKind::Vector,
            dpi: item_dpi(layout, &first, scaling),
            copies: item.copies,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanDto {
    pub per_sheet: usize,
    pub sheets: usize,
    pub slots: Vec<RectDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetDto {
    pub name: &'static str,
    pub width: f64,
    pub height: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateDto {
    pub settings: SettingsDto,
    pub items: Vec<ItemDto>,
    pub plan: PlanDto,
    pub presets: Vec<PresetDto>,
}

impl StateDto {
    pub fn new(workspace: &Workspace, plan: &SheetPlan) -> Self {
        Self {
            settings: SettingsDto::from_domain(&workspace.settings, &plan.layout),
            items: workspace
                .items()
                .iter()
                .map(|i| ItemDto::from_domain(i, &plan.layout, workspace.settings.scaling))
                .collect(),
            plan: PlanDto {
                per_sheet: plan.layout.per_sheet(),
                sheets: plan.job.sheets.len(),
                slots: plan.layout.slots.iter().map(|s| s.trim.into()).collect(),
            },
            presets: SHEET_PRESETS
                .iter()
                .map(|p| PresetDto { name: p.name, width: p.size.width, height: p.size.height })
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResultDto {
    pub state: StateDto,
    pub failed: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeavySheetDto {
    pub sheet: usize,
    pub megapixels: f64,
    pub max_dpi: f64,
}

impl From<HeavySheet> for HeavySheetDto {
    fn from(h: HeavySheet) -> Self {
        Self { sheet: h.sheet, megapixels: h.megapixels, max_dpi: h.max_dpi }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResultDto {
    pub path: String,
    pub sheets: usize,
}
