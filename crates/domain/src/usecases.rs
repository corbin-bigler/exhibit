//! The only ways into the domain. One struct per use case.

use std::sync::Arc;

use crate::items::{ImportedSource, ItemId, SourceId};
use crate::job::{HeavySheet, PrintJob, build_print_job, heavy_sheets};
use crate::layout::{SlotLayout, layout_slots};
use crate::ports::{Bitmap, DocumentSink, ExportOptions, PortError, SheetRenderer, SourceFile, SourceLibrary};
use crate::settings::{JobSettings, LayoutMode};
use crate::workspace::{Workspace, WorkspaceError};

pub struct ImportFiles {
    pub library: Arc<dyn SourceLibrary>,
}

#[derive(Debug, Default)]
pub struct ImportReport {
    pub added: Vec<ItemId>,
    /// File name and why it failed.
    pub failed: Vec<(String, PortError)>,
}

/// Files opened but not yet in the workspace.
#[derive(Debug, Default)]
pub struct OpenedFiles {
    sources: Vec<ImportedSource>,
    failed: Vec<(String, PortError)>,
}

impl ImportFiles {
    /// Files that fail don't stop the rest; each failure is reported with its file name.
    pub fn execute(&self, workspace: &mut Workspace, files: Vec<SourceFile>) -> ImportReport {
        let opened = self.open(files);
        self.add(workspace, opened)
    }

    /// The slow half: decoding. Needs no access to the workspace.
    pub fn open(&self, files: Vec<SourceFile>) -> OpenedFiles {
        let mut opened = OpenedFiles::default();
        for file in files {
            let name = file.name.clone();
            match self.library.open(file) {
                Ok(source) if source.pages.is_empty() => {
                    self.library.release(source.id);
                    opened.failed.push((name, PortError("the file has no pages".into())));
                }
                Ok(source) => opened.sources.push(source),
                Err(error) => opened.failed.push((name, error)),
            }
        }
        opened
    }

    /// The fast half: appending to the list.
    pub fn add(&self, workspace: &mut Workspace, opened: OpenedFiles) -> ImportReport {
        ImportReport {
            added: opened.sources.into_iter().map(|source| workspace.add(source)).collect(),
            failed: opened.failed,
        }
    }
}

pub struct RemoveItem {
    pub library: Arc<dyn SourceLibrary>,
}

impl RemoveItem {
    pub fn execute(&self, workspace: &mut Workspace, id: ItemId) -> Result<(), WorkspaceError> {
        let item = workspace.remove(id)?;
        release_if_unused(&*self.library, workspace, item.source);
        Ok(())
    }
}

pub struct ClearItems {
    pub library: Arc<dyn SourceLibrary>,
}

impl ClearItems {
    pub fn execute(&self, workspace: &mut Workspace) {
        for item in workspace.clear() {
            release_if_unused(&*self.library, workspace, item.source);
        }
    }
}

pub struct ReorderItems;

impl ReorderItems {
    pub fn execute(&self, workspace: &mut Workspace, order: &[ItemId]) -> Result<(), WorkspaceError> {
        workspace.reorder(order)
    }
}

pub struct SplitPack;

impl SplitPack {
    pub fn execute(&self, workspace: &mut Workspace, id: ItemId) -> Result<Vec<ItemId>, WorkspaceError> {
        workspace.split(id)
    }
}

pub struct SetItemCopies;

impl SetItemCopies {
    pub fn execute(&self, workspace: &mut Workspace, id: ItemId, copies: Option<u32>) -> Result<(), WorkspaceError> {
        workspace.set_copies(id, copies)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidSettings(pub &'static str);

impl std::fmt::Display for InvalidSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for InvalidSettings {}

pub struct UpdateSettings;

impl UpdateSettings {
    /// Invalid settings are rejected whole; the workspace keeps its last valid settings.
    pub fn execute(&self, workspace: &mut Workspace, settings: JobSettings) -> Result<(), InvalidSettings> {
        let finite = |v: f64| v.is_finite();
        let s = &settings;
        if !(finite(s.sheet.width) && s.sheet.width > 0.0 && finite(s.sheet.height) && s.sheet.height > 0.0) {
            return Err(InvalidSettings("sheet size must be positive"));
        }
        if !finite(s.sheet.margin) || s.sheet.margin < 0.0 || 2.0 * s.sheet.margin >= s.sheet.width.min(s.sheet.height) {
            return Err(InvalidSettings("margin must leave room on the sheet"));
        }
        if let LayoutMode::BySize { item, .. } = s.layout
            && !(finite(item.width) && item.width > 0.0 && finite(item.height) && item.height > 0.0) {
                return Err(InvalidSettings("item size must be positive"));
            }
        if let LayoutMode::ByGrid { across, down } = s.layout
            && (across == 0 || down == 0) {
                return Err(InvalidSettings("grid needs at least one row and column"));
            }
        for v in [s.gutter, s.bleed.width, s.marks.crop.length, s.marks.crop.offset] {
            if !finite(v) || v < 0.0 {
                return Err(InvalidSettings("lengths can't be negative"));
            }
        }
        workspace.settings = settings;
        Ok(())
    }
}

/// The numbers the UI shows: how many fit, how many sheets, and the job itself.
pub struct PlanSheets;

pub struct SheetPlan {
    pub layout: SlotLayout,
    pub job: PrintJob,
}

impl PlanSheets {
    pub fn execute(&self, workspace: &Workspace) -> SheetPlan {
        SheetPlan { layout: layout_slots(&workspace.settings), job: build_print_job(workspace) }
    }
}

pub struct RenderPreview {
    pub renderer: Arc<dyn SheetRenderer>,
}

impl RenderPreview {
    pub fn execute(&self, workspace: &Workspace, sheet: usize, pixels_per_point: f64) -> Result<Bitmap, PortError> {
        self.renderer.render_preview(&build_print_job(workspace), sheet, pixels_per_point)
    }
}

/// Run before exporting: sheets heavy enough that a printer may choke on them.
pub struct CheckExport;

impl CheckExport {
    pub fn execute(&self, workspace: &Workspace) -> Vec<HeavySheet> {
        heavy_sheets(&build_print_job(workspace))
    }
}

pub struct ExportPdf {
    pub renderer: Arc<dyn SheetRenderer>,
    pub sink: Arc<dyn DocumentSink>,
}

impl ExportPdf {
    /// Returns the number of sheets written.
    pub fn execute(&self, workspace: &Workspace, options: &ExportOptions) -> Result<usize, PortError> {
        let job = build_print_job(workspace);
        if job.sheets.is_empty() {
            return Err(PortError("there are no sheets to export".into()));
        }
        let bytes = self.renderer.render_pdf(&job, options)?;
        self.sink.write(&bytes)?;
        Ok(job.sheets.len())
    }
}

pub struct GetThumbnail {
    pub library: Arc<dyn SourceLibrary>,
}

impl GetThumbnail {
    pub fn execute(&self, source: SourceId, page: usize, max_px: u32) -> Result<Bitmap, PortError> {
        self.library.thumbnail(source, page, max_px)
    }
}

fn release_if_unused(library: &dyn SourceLibrary, workspace: &Workspace, source: SourceId) {
    if !workspace.references(source) {
        library.release(source);
    }
}
