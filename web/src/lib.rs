//! The browser shell around a [`Session`], compiled to WebAssembly. It runs inside a Web Worker
//! (`www/worker.js`) so long imports and exports never freeze the page. Files arrive as bytes
//! from JavaScript and the finished PDF goes back the same way; the page saves it.

use std::sync::Arc;

use exhibit_data::MemorySink;
use exhibit_domain::items::SourceFormat;
use exhibit_domain::ports::SourceFile;
use exhibit_session::Session;
use exhibit_session::dto::SettingsDto;
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn error(message: &str);
}

#[wasm_bindgen]
pub struct Exhibit {
    session: Session,
}

#[wasm_bindgen]
impl Exhibit {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        // A panic otherwise surfaces only as "unreachable executed".
        std::panic::set_hook(Box::new(|info| error(&info.to_string())));
        Self { session: Session::default() }
    }

    /// File extensions the importer reads, without dots.
    pub fn extensions(&self) -> Vec<String> {
        SourceFormat::EXTENSIONS.iter().map(|e| e.to_string()).collect()
    }

    pub fn state(&self) -> Result<JsValue, JsError> {
        to_js(self.session.state())
    }

    /// `contents[i]` (a `Uint8Array`) is the file called `names[i]`.
    pub fn import(&self, names: Vec<String>, contents: js_sys::Array) -> Result<JsValue, JsError> {
        let files = names
            .into_iter()
            .zip(contents.iter())
            .map(|(name, bytes)| SourceFile { name, bytes: js_sys::Uint8Array::new(&bytes).to_vec() })
            .collect();
        to_js(self.session.import(files, Vec::new()))
    }

    #[wasm_bindgen(js_name = removeItem)]
    pub fn remove_item(&self, id: f64) -> Result<JsValue, JsError> {
        to_js(self.session.remove_item(id as u64))
    }

    #[wasm_bindgen(js_name = clearItems)]
    pub fn clear_items(&self) -> Result<JsValue, JsError> {
        to_js(self.session.clear_items())
    }

    /// `order` is an array of item ids.
    #[wasm_bindgen(js_name = reorderItems)]
    pub fn reorder_items(&self, order: JsValue) -> Result<JsValue, JsError> {
        to_js(self.session.reorder_items(from_js(order)?))
    }

    #[wasm_bindgen(js_name = splitPack)]
    pub fn split_pack(&self, id: f64) -> Result<JsValue, JsError> {
        to_js(self.session.split_pack(id as u64))
    }

    #[wasm_bindgen(js_name = setCopies)]
    pub fn set_copies(&self, id: f64, copies: Option<u32>) -> Result<JsValue, JsError> {
        to_js(self.session.set_copies(id as u64, copies))
    }

    #[wasm_bindgen(js_name = updateSettings)]
    pub fn update_settings(&self, settings: JsValue) -> Result<JsValue, JsError> {
        let settings: SettingsDto = from_js(settings)?;
        to_js(self.session.update_settings(settings))
    }

    /// Width and height (4 bytes each, little-endian), then RGBA.
    pub fn preview(&self, sheet: usize, pixels_per_point: f64) -> Result<Vec<u8>, JsError> {
        self.session.preview(sheet, pixels_per_point).map_err(|e| JsError::new(&e))
    }

    /// Same layout as [`Exhibit::preview`].
    pub fn thumbnail(&self, source: f64, page: usize) -> Result<Vec<u8>, JsError> {
        self.session.thumbnail(source as u64, page).map_err(|e| JsError::new(&e))
    }

    #[wasm_bindgen(js_name = checkExport)]
    pub fn check_export(&self) -> Result<JsValue, JsError> {
        to_js(self.session.check_export())
    }

    /// `{ sheets, bytes }`, the PDF as a `Uint8Array`.
    #[wasm_bindgen(js_name = exportPdf)]
    pub fn export_pdf(&self, max_dpi: Option<f64>) -> Result<JsValue, JsError> {
        let sink = Arc::new(MemorySink::default());
        let sheets = self.session.export(sink.clone(), max_dpi).map_err(|e| JsError::new(&e))?;
        let result = js_sys::Object::new();
        js_sys::Reflect::set(&result, &"sheets".into(), &(sheets as f64).into()).map_err(|_| JsError::new("export failed"))?;
        js_sys::Reflect::set(&result, &"bytes".into(), &js_sys::Uint8Array::from(sink.take().as_slice()).into())
            .map_err(|_| JsError::new("export failed"))?;
        Ok(result.into())
    }
}

/// Plain objects with `null` for missing values, the same JSON the Tauri app receives.
fn to_js<T: Serialize>(result: Result<T, String>) -> Result<JsValue, JsError> {
    let value = result.map_err(|e| JsError::new(&e))?;
    value.serialize(&serde_wasm_bindgen::Serializer::json_compatible()).map_err(|e| JsError::new(&e.to_string()))
}

fn from_js<T: serde::de::DeserializeOwned>(value: JsValue) -> Result<T, JsError> {
    serde_wasm_bindgen::from_value(value).map_err(|e| JsError::new(&e.to_string()))
}
