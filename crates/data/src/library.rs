//! Opened artwork, kept in memory in the form the PDF writer needs.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

use exhibit_domain::geometry::Size;
use exhibit_domain::items::{ArtKind, ImportedSource, PageArt, SourceFormat, SourceId};
use exhibit_domain::ports::{Bitmap, PortError, SourceFile, SourceLibrary};
use krilla::image::Image;
use krilla::pdf::{Pdf, PdfDocument};

use crate::decode::{self, DecodedImage};
use crate::render;

/// Longest side, in pixels, of the stand-in images used for on-screen previews.
pub(crate) const PROXY_PX: u32 = 1600;

pub(crate) enum Content {
    Pdf { document: PdfDocument, pages: Vec<(f32, f32)> },
    Svg(Arc<usvg::Tree>),
    /// JPEG, PNG, WebP and GIF go into the PDF as krilla embeds them (JPEG byte-for-byte).
    Embedded { image: Image, bytes: Arc<Vec<u8>>, exif_orientation: u8 },
    /// Everything else is decoded once, keeping bit depth and ICC.
    Decoded(DecodedImage),
}

pub(crate) struct Entry {
    pub content: Content,
    proxy: OnceLock<Option<Image>>,
    print: OnceLock<Option<Image>>,
}

impl Entry {
    fn new(content: Content) -> Self {
        Self { content, proxy: OnceLock::new(), print: OnceLock::new() }
    }

    /// Decoded pixels as one PDF image, built once: every copy and bleed piece reuses it.
    /// Building a fresh image per draw recompressed the pixels each time. `None` for other content.
    pub fn print_image(&self) -> Option<Image> {
        self.print
            .get_or_init(|| match &self.content {
                Content::Decoded(decoded) => Image::from_custom(decoded.clone(), false).ok(),
                _ => None,
            })
            .clone()
    }

    /// A small 8-bit stand-in for raster art, so previews stay fast. `None` for vector art.
    pub fn proxy(&self) -> Option<Image> {
        self.proxy
            .get_or_init(|| {
                let rgba = match &self.content {
                    Content::Embedded { bytes, .. } => decode::decode_for_proxy(bytes, PROXY_PX).ok()?,
                    Content::Decoded(decoded) => decoded.proxy(PROXY_PX),
                    Content::Pdf { .. } | Content::Svg(_) => return None,
                };
                let (w, h) = rgba.dimensions();
                Some(Image::from_rgba8(rgba.into_raw(), w, h))
            })
            .clone()
    }
}

#[derive(Default)]
pub struct SourceStore {
    entries: RwLock<HashMap<SourceId, Arc<Entry>>>,
    next: AtomicU64,
}

impl SourceStore {
    pub(crate) fn get(&self, id: SourceId) -> Option<Arc<Entry>> {
        self.entries.read().ok()?.get(&id).cloned()
    }
}

impl SourceLibrary for SourceStore {
    fn open(&self, file: SourceFile) -> Result<ImportedSource, PortError> {
        let SourceFile { name, bytes } = file;
        let format = name
            .rsplit_once('.')
            .and_then(|(_, extension)| SourceFormat::from_extension(extension))
            .ok_or_else(|| PortError(format!("{name}: unsupported file type")))?;
        let (content, pages) = load(format, bytes).map_err(|e| PortError(format!("{name}: {e}")))?;

        let id = SourceId(self.next.fetch_add(1, Ordering::Relaxed) + 1);
        self.entries
            .write()
            .map_err(|_| PortError("source store is poisoned".into()))?
            .insert(id, Arc::new(Entry::new(content)));
        Ok(ImportedSource { id, name, format, pages })
    }

    fn release(&self, source: SourceId) {
        if let Ok(mut entries) = self.entries.write() {
            entries.remove(&source);
        }
    }

    fn thumbnail(&self, source: SourceId, page: usize, max_px: u32) -> Result<Bitmap, PortError> {
        let entry = self.get(source).ok_or_else(|| PortError("unknown source".into()))?;
        render::thumbnail(&entry, page, max_px).map_err(PortError)
    }
}

fn load(format: SourceFormat, bytes: Vec<u8>) -> Result<(Content, Vec<PageArt>), String> {
    let bytes = Arc::new(bytes);
    let raster = |w: u32, h: u32| {
        vec![PageArt { page: 0, size: Size::new(w as f64, h as f64), kind: ArtKind::Raster { width_px: w, height_px: h } }]
    };
    let decoded = |d: DecodedImage| {
        let pages = raster(d.width, d.height);
        (Content::Decoded(d), pages)
    };

    Ok(match format {
        SourceFormat::Pdf => {
            let pdf = Pdf::new(bytes.clone()).map_err(|e| {
                if format!("{e:?}").contains("Decryption") {
                    "password-protected PDFs aren't supported".to_string()
                } else {
                    "not a readable PDF".to_string()
                }
            })?;
            let sizes: Vec<(f32, f32)> = pdf.pages().iter().map(|p| p.render_dimensions()).collect();
            let pages = sizes
                .iter()
                .enumerate()
                .map(|(page, (w, h))| PageArt { page, size: Size::new(*w as f64, *h as f64), kind: ArtKind::Vector })
                .collect();
            (Content::Pdf { document: PdfDocument::new(Arc::new(pdf)), pages: sizes }, pages)
        }
        SourceFormat::Svg => {
            let tree = parse_svg(&bytes)?;
            // usvg works in CSS pixels (96 per inch); the page wants points.
            let size = Size::new(tree.size().width() as f64 * 0.75, tree.size().height() as f64 * 0.75);
            (Content::Svg(Arc::new(tree)), vec![PageArt { page: 0, size, kind: ArtKind::Vector }])
        }
        SourceFormat::Jpeg => {
            let image = Image::from_jpeg(krilla::Data::from(bytes.clone()), false)?;
            let exif_orientation = decode::exif_orientation(&bytes);
            let (w, h) = image.size();
            let (w, h) = if exif_orientation >= 5 { (h, w) } else { (w, h) };
            (Content::Embedded { image, bytes, exif_orientation }, raster(w, h))
        }
        SourceFormat::Png | SourceFormat::WebP | SourceFormat::Gif => {
            let data = krilla::Data::from(bytes.clone());
            let image = match format {
                SourceFormat::Png => Image::from_png(data, false)?,
                SourceFormat::WebP => Image::from_webp(data, false)?,
                _ => Image::from_gif(data, false)?,
            };
            let (w, h) = image.size();
            (Content::Embedded { image, bytes, exif_orientation: 1 }, raster(w, h))
        }
        SourceFormat::Bmp | SourceFormat::Tiff | SourceFormat::Tga | SourceFormat::Qoi | SourceFormat::Pnm => {
            let hint = (format == SourceFormat::Tga).then_some(image::ImageFormat::Tga);
            decoded(decode::decode_with_image_crate(&bytes, hint)?)
        }
        SourceFormat::JpegXl => decoded(decode::decode_jxl(&bytes)?),
        SourceFormat::Heif | SourceFormat::Avif => {
            #[cfg(windows)]
            {
                decoded(crate::wic::decode(&bytes)?)
            }
            #[cfg(not(windows))]
            {
                return Err("HEIC and AVIF can only be opened in the Windows app for now".into());
            }
        }
    })
}

/// Images the SVG links by file path can't be found (there's no folder to look in); embedded
/// images are fine.
fn parse_svg(bytes: &[u8]) -> Result<usvg::Tree, String> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    let fonts = FONTS.get_or_init(|| {
        // Finds nothing in a browser, where pages can't read installed fonts.
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    });
    parse_svg_with(bytes, fonts.clone())
}

/// Text whose font can't be found would silently vanish from the print, so it's an error instead.
fn parse_svg_with(bytes: &[u8], fonts: Arc<usvg::fontdb::Database>) -> Result<usvg::Tree, String> {
    let font_missing = Arc::new(AtomicBool::new(false));
    let select = usvg::FontResolver::default_font_selector();
    let flag = font_missing.clone();
    let options = usvg::Options {
        fontdb: fonts,
        font_resolver: usvg::FontResolver {
            select_font: Box::new(move |font, db| {
                let found = select(font, db);
                if found.is_none() {
                    flag.store(true, Ordering::Relaxed);
                }
                found
            }),
            select_fallback: usvg::FontResolver::default_fallback_selector(),
        },
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_data(bytes, &options).map_err(|e| e.to_string())?;
    if font_missing.load(Ordering::Relaxed) {
        return Err("its text uses fonts that aren't available here. Convert the text to outlines".into());
    }
    Ok(tree)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT_SVG: &[u8] =
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="40"><text x="5" y="30">Hi</text></svg>"#;

    #[test]
    fn svg_text_without_fonts_is_an_error() {
        let error = parse_svg_with(TEXT_SVG, Arc::new(usvg::fontdb::Database::new())).unwrap_err();
        assert!(error.contains("outlines"), "{error}");
    }

    #[test]
    fn svg_without_text_needs_no_fonts() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10"/></svg>"#;
        assert!(parse_svg_with(svg, Arc::new(usvg::fontdb::Database::new())).is_ok());
    }
}
