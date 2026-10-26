//! What the domain needs from the outside world. Implemented in `exhibit-data`.

use std::path::Path;

use crate::items::{ImportedSource, SourceId};
use crate::job::PrintJob;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortError(pub String);

impl std::fmt::Display for PortError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PortError {}

/// Opens artwork files and keeps them available for drawing.
pub trait SourceLibrary: Send + Sync {
    fn open(&self, path: &Path) -> Result<ImportedSource, PortError>;
    fn release(&self, source: SourceId);
    /// A small RGBA preview of one page, longest side at most `max_px`.
    fn thumbnail(&self, source: SourceId, page: usize, max_px: u32) -> Result<Bitmap, PortError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ExportOptions {
    /// Downsample raster placements above this effective resolution. `None` keeps originals.
    pub max_dpi: Option<f64>,
}

/// Draws a print job.
pub trait SheetRenderer: Send + Sync {
    /// The full-quality PDF.
    fn render_pdf(&self, job: &PrintJob, options: &ExportOptions) -> Result<Vec<u8>, PortError>;
    /// One sheet as a bitmap, drawn by the same code path as the PDF but from lightweight proxies.
    fn render_preview(&self, job: &PrintJob, sheet: usize, pixels_per_point: f64) -> Result<Bitmap, PortError>;
}

/// Writes finished files.
pub trait DocumentSink: Send + Sync {
    fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), PortError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    /// Straight (non-premultiplied) RGBA, row-major.
    pub rgba: Vec<u8>,
}
