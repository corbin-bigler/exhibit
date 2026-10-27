//! One open workspace and the commands the UI can run on it. The Tauri app and the web build are
//! thin shells around this: each supplies files and saves output its own way.

pub mod dto;

use std::sync::{Arc, Mutex, MutexGuard};

use exhibit_data::{KrillaRenderer, SourceStore};
use exhibit_domain::items::{ItemId, SourceId};
use exhibit_domain::ports::{Bitmap, DocumentSink, ExportOptions, SourceFile};
use exhibit_domain::usecases::{
    CheckExport, ClearItems, ExportPdf, GetThumbnail, ImportFiles, PlanSheets, RemoveItem, RenderPreview,
    ReorderItems, SetItemCopies, SplitPack, UpdateSettings,
};
use exhibit_domain::workspace::Workspace;

use crate::dto::{HeavySheetDto, ImportResultDto, SettingsDto, StateDto};

/// Longest side of list thumbnails, in pixels (2× the 52 px slot).
const THUMBNAIL_PX: u32 = 112;

pub struct Session {
    workspace: Mutex<Workspace>,
    store: Arc<SourceStore>,
    renderer: Arc<KrillaRenderer>,
}

impl Default for Session {
    fn default() -> Self {
        let store = Arc::new(SourceStore::default());
        Self { workspace: Mutex::new(Workspace::default()), renderer: Arc::new(KrillaRenderer::new(store.clone())), store }
    }
}

impl Session {
    fn workspace(&self) -> Result<MutexGuard<'_, Workspace>, String> {
        self.workspace.lock().map_err(|_| "workspace is unavailable".to_string())
    }

    /// A copy to render from, so long renders don't hold the lock.
    fn snapshot(&self) -> Result<Workspace, String> {
        Ok(self.workspace()?.clone())
    }

    pub fn state(&self) -> Result<StateDto, String> {
        Ok(state_of(&*self.workspace()?))
    }

    /// `unread` lists files the shell couldn't read, reported alongside the ones that fail to open.
    pub fn import(&self, files: Vec<SourceFile>, unread: Vec<String>) -> Result<ImportResultDto, String> {
        // Decode outside the lock: big files take a while and other commands shouldn't wait.
        let import = ImportFiles { library: self.store.clone() };
        let opened = import.open(files);
        let mut workspace = self.workspace()?;
        let report = import.add(&mut workspace, opened);
        let mut failed = unread;
        failed.extend(report.failed.into_iter().map(|(_, e)| e.0));
        Ok(ImportResultDto { state: state_of(&workspace), failed })
    }

    pub fn remove_item(&self, id: u64) -> Result<StateDto, String> {
        let mut workspace = self.workspace()?;
        RemoveItem { library: self.store.clone() }.execute(&mut workspace, ItemId(id)).map_err(|e| e.to_string())?;
        Ok(state_of(&workspace))
    }

    pub fn clear_items(&self) -> Result<StateDto, String> {
        let mut workspace = self.workspace()?;
        ClearItems { library: self.store.clone() }.execute(&mut workspace);
        Ok(state_of(&workspace))
    }

    pub fn reorder_items(&self, order: Vec<u64>) -> Result<StateDto, String> {
        let mut workspace = self.workspace()?;
        let order: Vec<ItemId> = order.into_iter().map(ItemId).collect();
        ReorderItems.execute(&mut workspace, &order).map_err(|e| e.to_string())?;
        Ok(state_of(&workspace))
    }

    pub fn split_pack(&self, id: u64) -> Result<StateDto, String> {
        let mut workspace = self.workspace()?;
        SplitPack.execute(&mut workspace, ItemId(id)).map_err(|e| e.to_string())?;
        Ok(state_of(&workspace))
    }

    pub fn set_copies(&self, id: u64, copies: Option<u32>) -> Result<StateDto, String> {
        let mut workspace = self.workspace()?;
        SetItemCopies.execute(&mut workspace, ItemId(id), copies).map_err(|e| e.to_string())?;
        Ok(state_of(&workspace))
    }

    pub fn update_settings(&self, settings: SettingsDto) -> Result<StateDto, String> {
        let mut workspace = self.workspace()?;
        UpdateSettings.execute(&mut workspace, settings.to_domain()?).map_err(|e| e.to_string())?;
        Ok(state_of(&workspace))
    }

    /// See [`encode_bitmap`] for the byte layout.
    pub fn preview(&self, sheet: usize, pixels_per_point: f64) -> Result<Vec<u8>, String> {
        let workspace = self.snapshot()?;
        let bitmap = RenderPreview { renderer: self.renderer.clone() }
            .execute(&workspace, sheet, pixels_per_point)
            .map_err(|e| e.0)?;
        Ok(encode_bitmap(bitmap))
    }

    /// See [`encode_bitmap`] for the byte layout.
    pub fn thumbnail(&self, source: u64, page: usize) -> Result<Vec<u8>, String> {
        let bitmap = GetThumbnail { library: self.store.clone() }
            .execute(SourceId(source), page, THUMBNAIL_PX)
            .map_err(|e| e.0)?;
        Ok(encode_bitmap(bitmap))
    }

    pub fn check_export(&self) -> Result<Vec<HeavySheetDto>, String> {
        Ok(CheckExport.execute(&*self.workspace()?).into_iter().map(Into::into).collect())
    }

    /// Writes the PDF to `sink`; returns the number of sheets.
    pub fn export(&self, sink: Arc<dyn DocumentSink>, max_dpi: Option<f64>) -> Result<usize, String> {
        let workspace = self.snapshot()?;
        let export = ExportPdf { renderer: self.renderer.clone(), sink };
        export.execute(&workspace, &ExportOptions { max_dpi }).map_err(|e| e.0)
    }
}

fn state_of(workspace: &Workspace) -> StateDto {
    StateDto::new(workspace, &PlanSheets.execute(workspace))
}

/// Bitmaps travel as raw bytes: 4-byte width, 4-byte height (little-endian), then RGBA.
pub fn encode_bitmap(bitmap: Bitmap) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8 + bitmap.rgba.len());
    bytes.extend_from_slice(&bitmap.width.to_le_bytes());
    bytes.extend_from_slice(&bitmap.height.to_le_bytes());
    bytes.extend_from_slice(&bitmap.rgba);
    bytes
}
