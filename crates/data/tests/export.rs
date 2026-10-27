//! Builds real PDFs from generated fixtures and checks the quality promises in docs/VALIDATION.md.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use exhibit_data::{FileSink, KrillaRenderer, SourceStore};
use exhibit_domain::items::{ArtKind, ItemId};
use exhibit_domain::job::build_print_job;
use exhibit_domain::ports::{ExportOptions, SheetRenderer, SourceFile, SourceLibrary};
use exhibit_domain::usecases::{ExportPdf, ImportFiles};
use exhibit_domain::workspace::Workspace;
use image::{ImageBuffer, ImageEncoder, Rgb, Rgba};

const SRGB_PROFILE: &str = r"C:\Windows\System32\spool\drivers\color\sRGB Color Space Profile.icm";

struct Fixtures {
    dir: PathBuf,
    icc: Option<Vec<u8>>,
}

impl Fixtures {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("exhibit-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Self { dir, icc: std::fs::read(SRGB_PROFILE).ok() }
    }

    fn jpeg(&self) -> PathBuf {
        let img = ImageBuffer::from_fn(300, 200, |x, y| Rgb([(x % 256) as u8, (y % 256) as u8, 128]));
        let mut bytes = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 90);
        if let Some(icc) = &self.icc {
            encoder.set_icc_profile(icc.clone()).unwrap();
        }
        encoder.write_image(img.as_raw(), 300, 200, image::ExtendedColorType::Rgb8).unwrap();
        self.write("photo.jpg", &bytes)
    }

    fn png16_alpha(&self) -> PathBuf {
        let img: ImageBuffer<Rgba<u16>, Vec<u16>> =
            ImageBuffer::from_fn(64, 48, |x, y| Rgba([x as u16 * 1000, y as u16 * 1300, 40000, if x < 32 { 65535 } else { 20000 }]));
        let mut bytes = Vec::new();
        let mut encoder = image::codecs::png::PngEncoder::new(&mut bytes);
        if let Some(icc) = &self.icc {
            encoder.set_icc_profile(icc.clone()).unwrap();
        }
        let raw: Vec<u8> = img.as_raw().iter().flat_map(|v| v.to_ne_bytes()).collect();
        encoder.write_image(&raw, 64, 48, image::ExtendedColorType::Rgba16).unwrap();
        self.write("deep.png", &bytes)
    }

    fn tiff16(&self) -> PathBuf {
        let img: ImageBuffer<Rgb<u16>, Vec<u16>> =
            ImageBuffer::from_fn(64, 48, |x, y| Rgb([x as u16 * 1000, y as u16 * 1300, 30000]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        let mut encoder = image::codecs::tiff::TiffEncoder::new(&mut bytes);
        if let Some(icc) = &self.icc {
            encoder.set_icc_profile(icc.clone()).unwrap();
        }
        let raw: Vec<u8> = img.as_raw().iter().flat_map(|v| v.to_ne_bytes()).collect();
        encoder.write_image(&raw, 64, 48, image::ExtendedColorType::Rgb16).unwrap();
        self.write("scan.tiff", bytes.get_ref())
    }

    fn svg(&self) -> PathBuf {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="300">
            <rect width="200" height="300" fill="#F9D9C6"/><circle cx="130" cy="90" r="45" fill="#ED1C24"/>
            <text x="20" y="260" font-size="24" font-family="Arial">Steamroller</text></svg>"##;
        self.write("logo.svg", svg.as_bytes())
    }

    /// A two-page vector PDF made with krilla.
    fn pdf(&self) -> PathBuf {
        use krilla::geom::{PathBuilder, Rect};
        use krilla::page::PageSettings;
        let mut document = krilla::Document::new();
        for shade in [40u8, 200] {
            let mut page = document.start_page_with(PageSettings::from_wh(360.0, 504.0).unwrap());
            let mut surface = page.surface();
            surface.set_fill(Some(krilla::paint::Fill {
                paint: krilla::color::rgb::Color::new(shade, 30, 36).into(),
                ..Default::default()
            }));
            let mut builder = PathBuilder::new();
            builder.push_rect(Rect::from_xywh(20.0, 20.0, 320.0, 464.0).unwrap());
            surface.draw_path(&builder.finish().unwrap());
            surface.finish();
            page.finish();
        }
        self.write("flyers.pdf", &document.finish().unwrap())
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }
}

impl Drop for Fixtures {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Reads fixture files the way the app does before importing them.
fn files(paths: &[PathBuf]) -> Vec<SourceFile> {
    paths
        .iter()
        .map(|p| SourceFile { name: p.file_name().unwrap().to_string_lossy().into_owned(), bytes: std::fs::read(p).unwrap() })
        .collect()
}

fn setup() -> (Arc<SourceStore>, KrillaRenderer, ImportFiles) {
    let store = Arc::new(SourceStore::default());
    let renderer = KrillaRenderer::new(store.clone());
    let import = ImportFiles { library: store.clone() };
    (store, renderer, import)
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    let exact = |n: &[u8]| haystack.windows(n.len()).filter(|w| *w == n).count();
    // Copied objects may be written without the space between a key and a name value.
    let compact: Vec<u8> = needle.iter().copied().filter(|b| *b != b' ').collect();
    if compact.len() == needle.len() || !needle.starts_with(b"/") || !needle[1..].contains(&b'/') {
        exact(needle)
    } else {
        exact(needle) + exact(&compact)
    }
}

fn page_count(pdf: &[u8]) -> usize {
    hayro::hayro_syntax::Pdf::new(Arc::new(pdf.to_vec())).unwrap().pages().len()
}

fn export_only(workspace: &mut Workspace, renderer: &KrillaRenderer, keep: &[ItemId]) -> Vec<u8> {
    let order: Vec<ItemId> = keep.to_vec();
    for id in workspace.items().iter().map(|i| i.id).collect::<Vec<_>>() {
        if !order.contains(&id) {
            workspace.remove(id).unwrap();
        }
    }
    renderer.render_pdf(&build_print_job(workspace), &ExportOptions::default()).unwrap()
}

// IMP-01..08, IMP-11..13
#[test]
fn imports_every_fixture_format_and_reports_bad_files() {
    let fx = Fixtures::new("import");
    let (_, _, import) = setup();
    let mut ws = Workspace::default();
    let bad = fx.write("broken.png", b"not a png at all");
    let unknown = fx.write("notes.txt", b"hello");
    let paths = vec![fx.jpeg(), fx.png16_alpha(), fx.tiff16(), fx.svg(), fx.pdf(), bad, unknown];
    let report = import.execute(&mut ws, files(&paths));
    assert_eq!(report.added.len(), 5, "failures: {:?}", report.failed);
    assert_eq!(report.failed.len(), 2);
    assert!(report.failed.iter().any(|(_, e)| e.0.starts_with("broken.png")));
    assert!(report.failed.iter().any(|(_, e)| e.0.contains("unsupported")));

    let items = ws.items();
    assert_eq!(items[0].pages[0].kind, ArtKind::Raster { width_px: 300, height_px: 200 });
    assert_eq!(items[3].pages[0].kind, ArtKind::Vector);
    assert!(items[4].is_pack());
    assert_eq!(items[4].pages.len(), 2);
}

// Q-01, Q-02, B-06, P-04
#[test]
fn pdf_pages_stay_vector_and_are_embedded_once() {
    let fx = Fixtures::new("vector");
    let (_, renderer, import) = setup();
    let mut ws = Workspace::default();
    let report = import.execute(&mut ws, files(&[fx.pdf(), fx.svg()]));
    let pdf = export_only(&mut ws, &renderer, &report.added);
    assert_eq!(count(&pdf, b"/Subtype /Image"), 0, "no raster images for vector input");
    // Two PDF pages, each drawn 4 copies × 9 times (main + 8 bleed regions): still one XObject per page.
    let forms = count(&pdf, b"/Subtype /Form");
    assert!((2..=4).contains(&forms), "expected one form per source page, found {forms}");
    assert_eq!(page_count(&pdf), build_print_job(&ws).sheets.len());
    assert_eq!(page_count(&pdf), 3);
}

// Q-03
#[test]
fn jpeg_bytes_are_embedded_untouched() {
    let fx = Fixtures::new("jpeg");
    let (_, renderer, import) = setup();
    let mut ws = Workspace::default();
    let path = fx.jpeg();
    let report = import.execute(&mut ws, files(std::slice::from_ref(&path)));
    let pdf = export_only(&mut ws, &renderer, &report.added);
    let source = std::fs::read(&path).unwrap();
    assert_eq!(count(&pdf, b"/DCTDecode"), 1, "one JPEG stream for all four copies");
    assert!(pdf.windows(source.len()).any(|w| w == source.as_slice()), "source JPEG bytes appear verbatim");
    assert_eq!(count(&pdf, b"/Width 300"), 1, "Q-05: not resampled");
}

// Q-04, Q-05, Q-06, Q-09
#[test]
fn deep_images_keep_bit_depth_alpha_and_icc() {
    let fx = Fixtures::new("deep");
    let (_, renderer, import) = setup();
    let mut ws = Workspace::default();
    let report = import.execute(&mut ws, files(&[fx.png16_alpha(), fx.tiff16()]));
    let pdf = export_only(&mut ws, &renderer, &report.added);
    assert!(count(&pdf, b"/BitsPerComponent 16") >= 2, "16-bit colour kept");
    assert!(count(&pdf, b"/SMask") >= 1, "alpha kept");
    assert!(count(&pdf, b"/Width 64") >= 2, "pixel size kept");
    if fx.icc.is_some() {
        assert!(count(&pdf, b"/ICCBased") >= 1, "ICC profile kept");
    }
}

// P-01 smoke test, I-08 thumbnails
#[test]
fn previews_and_thumbnails_render() {
    let fx = Fixtures::new("preview");
    let (store, renderer, import) = setup();
    let mut ws = Workspace::default();
    import.execute(&mut ws, files(&[fx.jpeg(), fx.svg(), fx.pdf(), fx.tiff16()]));
    let job = build_print_job(&ws);
    let preview = renderer.render_preview(&job, 0, 0.5).unwrap();
    assert_eq!((preview.width, preview.height), (432, 648));
    assert_eq!(preview.rgba.len(), (432 * 648 * 4) as usize);
    for item in ws.items() {
        let thumb = store.thumbnail(item.source, 0, 96).unwrap();
        assert!(thumb.width.max(thumb.height) <= 96 && thumb.width > 0);
    }
}

// P-05 (file half)
#[test]
fn export_writes_the_file() {
    let fx = Fixtures::new("export");
    let (store, renderer, import) = setup();
    let mut ws = Workspace::default();
    import.execute(&mut ws, files(&[fx.pdf()]));
    let out = fx.dir.join("out.pdf");
    let export = ExportPdf { renderer: Arc::new(renderer), sink: Arc::new(FileSink::new(&out)) };
    assert_eq!(export.execute(&ws, &ExportOptions::default()).unwrap(), 2);
    assert_eq!(page_count(&std::fs::read(&out).unwrap()), 2);
    assert!(!Path::new(&fx.dir.join("out.pdf.partial")).exists());
    drop(store);
}

// X-04: the "downsample to N dpi" export choice
#[test]
fn capping_resolution_resamples_only_images_above_it() {
    let fx = Fixtures::new("downsample");
    let (_, renderer, import) = setup();
    let mut ws = Workspace::default();
    import.execute(&mut ws, files(&[fx.jpeg()]));
    // 10 × 10 grid on 12×18 → ~1.1 × 1.7 in slots, so the 300×200 JPEG prints at ~170 dpi.
    ws.settings.layout = exhibit_domain::settings::LayoutMode::ByGrid { across: 10, down: 10 };
    let job = build_print_job(&ws);

    let untouched = renderer.render_pdf(&job, &ExportOptions { max_dpi: Some(600.0) }).unwrap();
    assert_eq!(count(&untouched, b"/DCTDecode"), 1, "below the cap: original JPEG kept");

    let capped = renderer.render_pdf(&job, &ExportOptions { max_dpi: Some(72.0) }).unwrap();
    assert_eq!(count(&capped, b"/DCTDecode"), 0, "above the cap: resampled");
    assert_eq!(count(&capped, b"/Width 300"), 0);
    if fx.icc.is_some() {
        assert!(count(&capped, b"/ICCBased") >= 1, "resampled image keeps its ICC profile");
    }
}
