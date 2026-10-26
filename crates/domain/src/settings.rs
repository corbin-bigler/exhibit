//! Job settings. All lengths are points.

use crate::geometry::Size;
use crate::units::{inches, millimeters};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SheetSettings {
    pub width: f64,
    pub height: f64,
    pub margin: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LayoutMode {
    /// The user picks the item size; the most that fit are placed.
    BySize { item: Size, mix_orientations: bool },
    /// The user picks the grid; the item size is derived from the sheet.
    ByGrid { across: u32, down: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scaling {
    /// Cover the slot, cropping overflow.
    Fill,
    /// Show all art, leaving blank space.
    Fit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BleedMode {
    /// Reflect the art across each trim edge.
    Mirror,
    /// Stretch the outermost edge of the art.
    Edge,
    /// Scale the art up to cover trim plus bleed.
    Scale,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BleedSettings {
    pub width: f64,
    pub mode: BleedMode,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropMarkSettings {
    pub enabled: bool,
    pub length: f64,
    /// Gap between the outermost bleed edge and the start of a mark.
    pub offset: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkSettings {
    pub crop: CropMarkSettings,
    /// Print a thin line on every trim edge, for hand cutters.
    pub trim_lines: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JobSettings {
    pub sheet: SheetSettings,
    pub layout: LayoutMode,
    pub gutter: f64,
    pub scaling: Scaling,
    pub bleed: BleedSettings,
    pub marks: MarkSettings,
}

impl Default for JobSettings {
    fn default() -> Self {
        Self {
            sheet: SheetSettings { width: inches(12.0), height: inches(18.0), margin: inches(0.25) },
            layout: LayoutMode::BySize {
                item: Size::new(inches(5.0), inches(7.0)),
                mix_orientations: false,
            },
            gutter: inches(0.125),
            scaling: Scaling::Fill,
            bleed: BleedSettings { width: inches(0.125), mode: BleedMode::Mirror },
            marks: MarkSettings {
                crop: CropMarkSettings { enabled: true, length: inches(0.25), offset: inches(0.0625) },
                trim_lines: false,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SheetPreset {
    pub name: &'static str,
    pub size: Size,
}

pub const SHEET_PRESETS: &[SheetPreset] = &[
    SheetPreset { name: "Letter", size: Size::new(inches(8.5), inches(11.0)) },
    SheetPreset { name: "Legal", size: Size::new(inches(8.5), inches(14.0)) },
    SheetPreset { name: "Tabloid", size: Size::new(inches(11.0), inches(17.0)) },
    SheetPreset { name: "Tabloid Extra", size: Size::new(inches(12.0), inches(18.0)) },
    SheetPreset { name: "13 × 19", size: Size::new(inches(13.0), inches(19.0)) },
    SheetPreset { name: "A4", size: Size::new(millimeters(210.0), millimeters(297.0)) },
    SheetPreset { name: "A3", size: Size::new(millimeters(297.0), millimeters(420.0)) },
    SheetPreset { name: "SRA3", size: Size::new(millimeters(320.0), millimeters(450.0)) },
];
