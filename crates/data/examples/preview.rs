//! Save the app's on-screen preview of sheet 1 as PNG:
//! `cargo run -p exhibit-data --example preview -- out.png <pixels-per-point> [--edge|--scale] files...`

use std::path::PathBuf;
use std::sync::Arc;

use exhibit_data::{KrillaRenderer, SourceStore};
use exhibit_domain::ports::SourceFile;
use exhibit_domain::settings::BleedMode;
use exhibit_domain::usecases::{ImportFiles, RenderPreview};
use exhibit_domain::workspace::Workspace;

fn main() {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().expect("output path"));
    let scale: f64 = args.next().expect("pixels per point").parse().unwrap();
    let mut workspace = Workspace::default();
    let mut files = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--edge" => workspace.settings.bleed.mode = BleedMode::Edge,
            "--scale" => workspace.settings.bleed.mode = BleedMode::Scale,
            _ => files.push(read(&arg)),
        }
    }
    let store = Arc::new(SourceStore::default());
    ImportFiles { library: store.clone() }.execute(&mut workspace, files);
    let bitmap = RenderPreview { renderer: Arc::new(KrillaRenderer::new(store)) }
        .execute(&workspace, 0, scale)
        .expect("preview");
    image::RgbaImage::from_raw(bitmap.width, bitmap.height, bitmap.rgba).unwrap().save(&out).unwrap();
}

fn read(arg: &str) -> SourceFile {
    let path = PathBuf::from(arg);
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    SourceFile { bytes: std::fs::read(&path).unwrap_or_else(|e| panic!("{name}: {e}")), name }
}
