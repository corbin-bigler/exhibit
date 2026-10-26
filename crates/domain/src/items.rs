//! Artwork the user has added, and the list they arrange it in.

use crate::geometry::Size;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SourceFormat {
    Pdf,
    Svg,
    Jpeg,
    Png,
    WebP,
    Gif,
    Bmp,
    Tiff,
    Tga,
    Qoi,
    Pnm,
    JpegXl,
    Heif,
    Avif,
}

impl SourceFormat {
    pub fn label(self) -> &'static str {
        match self {
            SourceFormat::Pdf => "PDF",
            SourceFormat::Svg => "SVG",
            SourceFormat::Jpeg => "JPG",
            SourceFormat::Png => "PNG",
            SourceFormat::WebP => "WebP",
            SourceFormat::Gif => "GIF",
            SourceFormat::Bmp => "BMP",
            SourceFormat::Tiff => "TIFF",
            SourceFormat::Tga => "TGA",
            SourceFormat::Qoi => "QOI",
            SourceFormat::Pnm => "PNM",
            SourceFormat::JpegXl => "JXL",
            SourceFormat::Heif => "HEIC",
            SourceFormat::Avif => "AVIF",
        }
    }

    pub fn from_extension(extension: &str) -> Option<Self> {
        Some(match extension.to_ascii_lowercase().as_str() {
            "pdf" => SourceFormat::Pdf,
            "svg" => SourceFormat::Svg,
            "jpg" | "jpeg" | "jpe" | "jfif" | "pjpeg" | "pjp" => SourceFormat::Jpeg,
            "png" => SourceFormat::Png,
            "webp" => SourceFormat::WebP,
            "gif" => SourceFormat::Gif,
            "bmp" | "dib" => SourceFormat::Bmp,
            "tif" | "tiff" => SourceFormat::Tiff,
            "tga" => SourceFormat::Tga,
            "qoi" => SourceFormat::Qoi,
            "pbm" | "pgm" | "ppm" | "pnm" | "pam" => SourceFormat::Pnm,
            "jxl" => SourceFormat::JpegXl,
            "heic" | "heif" | "hif" => SourceFormat::Heif,
            "avif" => SourceFormat::Avif,
            _ => return None,
        })
    }

    pub const EXTENSIONS: &'static [&'static str] = &[
        "pdf", "svg", "jpg", "jpeg", "jpe", "jfif", "pjpeg", "pjp", "png", "webp", "gif", "bmp", "dib",
        "tif", "tiff", "tga", "qoi", "pbm", "pgm", "ppm", "pnm", "pam", "jxl", "heic", "heif", "hif",
        "avif",
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtKind {
    Vector,
    Raster { width_px: u32, height_px: u32 },
}

/// One drawable page of a source: a PDF page, or the single image of any other format.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageArt {
    pub page: usize,
    /// Natural size. Points for vector art; only the aspect ratio matters for raster art.
    pub size: Size,
    pub kind: ArtKind,
}

/// What a source reader hands back after opening a file.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedSource {
    pub id: SourceId,
    pub name: String,
    pub format: SourceFormat,
    pub pages: Vec<PageArt>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub id: ItemId,
    pub source: SourceId,
    pub name: String,
    pub format: SourceFormat,
    /// More than one page makes this a pack: the pages share settings until split.
    pub pages: Vec<PageArt>,
    /// `None` means auto: one sheet's worth.
    pub copies: Option<u32>,
}

impl Item {
    pub fn is_pack(&self) -> bool {
        self.pages.len() > 1
    }
}
