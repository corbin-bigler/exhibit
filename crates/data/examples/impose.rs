//! Headless export for manual checks: `cargo run -p exhibit-data --example impose -- out.pdf [--edge|--scale] files...`

use std::path::PathBuf;
use std::sync::Arc;

use exhibit_data::{FileSink, KrillaRenderer, SourceStore};
use exhibit_domain::ports::{ExportOptions, SourceFile};
use exhibit_domain::settings::BleedMode;
use exhibit_domain::usecases::{ExportPdf, ImportFiles};
use exhibit_domain::workspace::Workspace;

fn main() {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().expect("output path"));
    let mut workspace = Workspace::default();
    let mut files = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--edge" => workspace.settings.bleed.mode = BleedMode::Edge,
            "--scale" => workspace.settings.bleed.mode = BleedMode::Scale,
            "--trim" => workspace.settings.marks.trim_lines = true,
            _ => files.push(read(&arg)),
        }
    }
    let store = Arc::new(SourceStore::default());
    let report = ImportFiles { library: store.clone() }.execute(&mut workspace, files);
    for (_, error) in &report.failed {
        eprintln!("{error}");
    }
    let export = ExportPdf { renderer: Arc::new(KrillaRenderer::new(store)), sink: Arc::new(FileSink::new(&out)) };
    let sheets = export.execute(&workspace, &ExportOptions::default()).expect("export");
    println!("{sheets} sheets → {}", out.display());
}

fn read(arg: &str) -> SourceFile {
    let path = PathBuf::from(arg);
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    SourceFile { bytes: std::fs::read(&path).unwrap_or_else(|e| panic!("{name}: {e}")), name }
}
