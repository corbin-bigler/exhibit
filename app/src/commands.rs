//! Tauri commands: the native shell around a [`Session`]. Reads files, shows the Windows file
//! dialogs and saves to disk; everything else is the session's.

use std::path::PathBuf;
use std::sync::Arc;

use exhibit_data::FileSink;
use exhibit_domain::items::SourceFormat;
use exhibit_domain::ports::SourceFile;
use exhibit_session::Session;
use exhibit_session::dto::{ExportResultDto, HeavySheetDto, ImportResultDto, SettingsDto, StateDto};
use tauri::ipc::Response;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

pub struct AppState {
    session: Session,
}

impl AppState {
    pub fn new() -> Self {
        Self { session: Session::default() }
    }

    /// Files given on the command line ("Open with", or dropped onto the exe).
    pub fn open_at_launch(&self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        let (files, unread) = read_files(paths);
        let _ = self.session.import(files, unread);
    }
}

#[tauri::command]
pub fn get_state(state: State<'_, AppState>) -> Result<StateDto, String> {
    state.session.state()
}

#[tauri::command]
pub async fn import_files(state: State<'_, AppState>, paths: Vec<PathBuf>) -> Result<ImportResultDto, String> {
    let (files, unread) = read_files(paths);
    state.session.import(files, unread)
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
    let (files, unread) = read_files(picked.into_iter().filter_map(|p| p.into_path().ok()).collect());
    state.session.import(files, unread)
}

/// Reads files from disk for import. Unreadable files come back as messages naming them.
fn read_files(paths: Vec<PathBuf>) -> (Vec<SourceFile>, Vec<String>) {
    let mut files = Vec::new();
    let mut failed = Vec::new();
    for path in paths {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match std::fs::read(&path) {
            Ok(bytes) => files.push(SourceFile { name, bytes }),
            Err(e) => failed.push(format!("{name}: {e}")),
        }
    }
    (files, failed)
}

#[tauri::command]
pub fn remove_item(state: State<'_, AppState>, id: u64) -> Result<StateDto, String> {
    state.session.remove_item(id)
}

#[tauri::command]
pub fn clear_items(state: State<'_, AppState>) -> Result<StateDto, String> {
    state.session.clear_items()
}

#[tauri::command]
pub fn reorder_items(state: State<'_, AppState>, order: Vec<u64>) -> Result<StateDto, String> {
    state.session.reorder_items(order)
}

#[tauri::command]
pub fn split_pack(state: State<'_, AppState>, id: u64) -> Result<StateDto, String> {
    state.session.split_pack(id)
}

#[tauri::command]
pub fn set_copies(state: State<'_, AppState>, id: u64, copies: Option<u32>) -> Result<StateDto, String> {
    state.session.set_copies(id, copies)
}

#[tauri::command]
pub fn update_settings(state: State<'_, AppState>, settings: SettingsDto) -> Result<StateDto, String> {
    state.session.update_settings(settings)
}

#[tauri::command]
pub async fn preview(state: State<'_, AppState>, sheet: usize, pixels_per_point: f64) -> Result<Response, String> {
    state.session.preview(sheet, pixels_per_point).map(Response::new)
}

#[tauri::command]
pub async fn thumbnail(state: State<'_, AppState>, source: u64, page: usize) -> Result<Response, String> {
    state.session.thumbnail(source, page).map(Response::new)
}

#[tauri::command]
pub fn check_export(state: State<'_, AppState>) -> Result<Vec<HeavySheetDto>, String> {
    state.session.check_export()
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
    let sheets = state.session.export(Arc::new(FileSink::new(&path)), max_dpi)?;
    Ok(Some(ExportResultDto { path: path.display().to_string(), sheets }))
}
