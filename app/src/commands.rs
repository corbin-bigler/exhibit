//! Tauri commands: translate UI calls into use cases and shape the results.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use exhibit_data::{FileSink, KrillaRenderer, SourceStore};
use exhibit_domain::items::{ItemId, SourceFormat, SourceId};
use exhibit_domain::ports::{Bitmap, ExportOptions};
use exhibit_domain::usecases::{
    CheckExport, ClearItems, ExportPdf, GetThumbnail, ImportFiles, PlanSheets, RemoveItem, RenderPreview,
    ReorderItems, SetItemCopies, SplitPack, UpdateSettings,
};
use exhibit_domain::workspace::Workspace;
use tauri::ipc::Response;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::dto::{ExportResultDto, HeavySheetDto, ImportResultDto, SettingsDto, StateDto};

/// Longest side of list thumbnails, in pixels (2× the 52 px slot).
const THUMBNAIL_PX: u32 = 112;

pub struct AppState {
    workspace: Mutex<Workspace>,
    store: Arc<SourceStore>,
    renderer: Arc<KrillaRenderer>,
}

impl AppState {
    pub fn new() -> Self {
        let store = Arc::new(SourceStore::default());
        Self { workspace: Mutex::new(Workspace::default()), renderer: Arc::new(KrillaRenderer::new(store.clone())), store }
    }

    /// Files given on the command line ("Open with", or dropped onto the exe).
    pub fn open_at_launch(&self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        let import = ImportFiles { library: self.store.clone() };
        let opened = import.open(&paths);
        if let Ok(mut workspace) = self.workspace() {
            import.add(&mut workspace, opened);
        }
    }

    fn workspace(&self) -> Result<MutexGuard<'_, Workspace>, String> {
        self.workspace.lock().map_err(|_| "workspace is unavailable".to_string())
    }

    /// A copy to render from, so long renders don't hold the lock.
    fn snapshot(&self) -> Result<Workspace, String> {
        Ok(self.workspace()?.clone())
    }
}

fn state_of(workspace: &Workspace) -> StateDto {
    StateDto::new(workspace, &PlanSheets.execute(workspace))
}

/// Bitmaps travel as raw bytes: 4-byte width, 4-byte height (little-endian), then RGBA.
fn bitmap_response(bitmap: Bitmap) -> Response {
    let mut bytes = Vec::with_capacity(8 + bitmap.rgba.len());
    bytes.extend_from_slice(&bitmap.width.to_le_bytes());
    bytes.extend_from_slice(&bitmap.height.to_le_bytes());
    bytes.extend_from_slice(&bitmap.rgba);
    Response::new(bytes)
}

#[tauri::command]
pub fn get_state(state: State<'_, AppState>) -> Result<StateDto, String> {
    Ok(state_of(&*state.workspace()?))
}

#[tauri::command]
pub async fn import_files(state: State<'_, AppState>, paths: Vec<PathBuf>) -> Result<ImportResultDto, String> {
    import(&state, paths)
}

#[tauri::command]
pub async fn pick_files(app: AppHandle, state: State<'_, AppState>) -> Result<ImportResultDto, String> {
    let picked = app
        .dialog()
        .file()
        .set_title("Add artwork")
        .add_filter("Artwork", SourceFormat::EXTENSIONS)
        .blocking_pick_files()
        .unwrap_or_default();
    let paths = picked.into_iter().filter_map(|p| p.into_path().ok()).collect();
    import(&state, paths)
}

fn import(state: &AppState, paths: Vec<PathBuf>) -> Result<ImportResultDto, String> {
    // Decode outside the lock: big files take a while and other commands shouldn't wait.
    let import = ImportFiles { library: state.store.clone() };
    let opened = import.open(&paths);
    let mut workspace = state.workspace()?;
    let report = import.add(&mut workspace, opened);
    Ok(ImportResultDto {
        state: state_of(&workspace),
        failed: report.failed.into_iter().map(|(_, e)| e.0).collect(),
    })
}

#[tauri::command]
pub fn remove_item(state: State<'_, AppState>, id: u64) -> Result<StateDto, String> {
    let mut workspace = state.workspace()?;
    RemoveItem { library: state.store.clone() }.execute(&mut workspace, ItemId(id)).map_err(|e| e.to_string())?;
    Ok(state_of(&workspace))
}

#[tauri::command]
pub fn clear_items(state: State<'_, AppState>) -> Result<StateDto, String> {
    let mut workspace = state.workspace()?;
    ClearItems { library: state.store.clone() }.execute(&mut workspace);
    Ok(state_of(&workspace))
}

#[tauri::command]
pub fn reorder_items(state: State<'_, AppState>, order: Vec<u64>) -> Result<StateDto, String> {
    let mut workspace = state.workspace()?;
    let order: Vec<ItemId> = order.into_iter().map(ItemId).collect();
    ReorderItems.execute(&mut workspace, &order).map_err(|e| e.to_string())?;
    Ok(state_of(&workspace))
}

#[tauri::command]
pub fn split_pack(state: State<'_, AppState>, id: u64) -> Result<StateDto, String> {
    let mut workspace = state.workspace()?;
    SplitPack.execute(&mut workspace, ItemId(id)).map_err(|e| e.to_string())?;
    Ok(state_of(&workspace))
}

#[tauri::command]
pub fn set_copies(state: State<'_, AppState>, id: u64, copies: Option<u32>) -> Result<StateDto, String> {
    let mut workspace = state.workspace()?;
    SetItemCopies.execute(&mut workspace, ItemId(id), copies).map_err(|e| e.to_string())?;
    Ok(state_of(&workspace))
}

#[tauri::command]
pub fn update_settings(state: State<'_, AppState>, settings: SettingsDto) -> Result<StateDto, String> {
    let mut workspace = state.workspace()?;
    UpdateSettings.execute(&mut workspace, settings.to_domain()?).map_err(|e| e.to_string())?;
    Ok(state_of(&workspace))
}

#[tauri::command]
pub async fn preview(state: State<'_, AppState>, sheet: usize, pixels_per_point: f64) -> Result<Response, String> {
    let workspace = state.snapshot()?;
    let bitmap = RenderPreview { renderer: state.renderer.clone() }
        .execute(&workspace, sheet, pixels_per_point)
        .map_err(|e| e.0)?;
    Ok(bitmap_response(bitmap))
}

#[tauri::command]
pub async fn thumbnail(state: State<'_, AppState>, source: u64, page: usize) -> Result<Response, String> {
    let bitmap = GetThumbnail { library: state.store.clone() }
        .execute(SourceId(source), page, THUMBNAIL_PX)
        .map_err(|e| e.0)?;
    Ok(bitmap_response(bitmap))
}

#[tauri::command]
pub fn check_export(state: State<'_, AppState>) -> Result<Vec<HeavySheetDto>, String> {
    Ok(CheckExport.execute(&*state.workspace()?).into_iter().map(Into::into).collect())
}

/// Asks where to save, then writes. `None` when the user cancels the dialog.
#[tauri::command]
pub async fn export_pdf(
    app: AppHandle,
    state: State<'_, AppState>,
    max_dpi: Option<f64>,
) -> Result<Option<ExportResultDto>, String> {
    let Some(target) = app
        .dialog()
        .file()
        .set_title("Export PDF")
        .add_filter("PDF", &["pdf"])
        .set_file_name("exhibit.pdf")
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = target.into_path().map_err(|e| e.to_string())?;
    let workspace = state.snapshot()?;
    let export = ExportPdf { renderer: state.renderer.clone(), sink: Arc::new(FileSink) };
    let sheets = export.execute(&workspace, &path, &ExportOptions { max_dpi }).map_err(|e| e.0)?;
    Ok(Some(ExportResultDto { path: path.display().to_string(), sheets }))
}
