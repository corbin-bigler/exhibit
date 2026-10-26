//! Decoding raster formats that krilla can't embed directly, without losing bit depth or ICC.

use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::sync::Arc;

use image::{DynamicImage, ImageDecoder, ImageReader, RgbaImage};
use krilla::image::{BitsPerComponent, CustomImage, ImageColorspace};

/// Decoded pixels ready for the PDF: planar colour and alpha, big-endian when 16-bit.
#[derive(Clone)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub colorspace: ImageColorspace,
    pub sixteen_bit: bool,
    pub color: Arc<Vec<u8>>,
    pub alpha: Option<Arc<Vec<u8>>>,
    pub icc: Option<Arc<Vec<u8>>>,
    /// Cheap identity for krilla's dedup cache.
    pub fingerprint: u128,
}

impl Hash for DecodedImage {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.fingerprint.hash(state);
    }
}

impl CustomImage for DecodedImage {
    fn color_channel(&self) -> &[u8] {
        &self.color
    }

    fn alpha_channel(&self) -> Option<&[u8]> {
        self.alpha.as_deref().map(|a| a.as_slice())
    }

    fn bits_per_component(&self) -> BitsPerComponent {
        if self.sixteen_bit { BitsPerComponent::Sixteen } else { BitsPerComponent::Eight }
    }

    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn icc_profile(&self) -> Option<&[u8]> {
        self.icc.as_deref().map(|i| i.as_slice())
    }

    fn color_space(&self) -> ImageColorspace {
        self.colorspace
    }
}

impl DecodedImage {
    /// Build from interleaved samples. `samples` holds `channels` values per pixel, the last one
    /// alpha when `has_alpha`. Fully opaque alpha is dropped.
    pub fn from_interleaved_u8(
        width: u32,
        height: u32,
        channels: usize,
        has_alpha: bool,
        samples: &[u8],
        icc: Option<Vec<u8>>,
        fingerprint: u128,
    ) -> Self {
        let color_channels = if has_alpha { channels - 1 } else { channels };
        let pixels = (width * height) as usize;
        let mut color = Vec::with_capacity(pixels * color_channels);
        let mut alpha = has_alpha.then(|| Vec::with_capacity(pixels));
        for px in samples.chunks_exact(channels) {
            color.extend_from_slice(&px[..color_channels]);
            if let Some(alpha) = alpha.as_mut() {
                alpha.push(px[color_channels]);
            }
        }
        let alpha = alpha.filter(|a| a.iter().any(|&v| v != u8::MAX));
        Self::assemble(width, height, color_channels, false, color, alpha, icc, fingerprint)
    }

    pub fn from_interleaved_u16(
        width: u32,
        height: u32,
        channels: usize,
        has_alpha: bool,
        samples: &[u16],
        icc: Option<Vec<u8>>,
        fingerprint: u128,
    ) -> Self {
        let color_channels = if has_alpha { channels - 1 } else { channels };
        let pixels = (width * height) as usize;
        let mut color = Vec::with_capacity(pixels * color_channels * 2);
        let mut alpha = has_alpha.then(|| Vec::with_capacity(pixels * 2));
        let mut opaque = true;
        for px in samples.chunks_exact(channels) {
            for v in &px[..color_channels] {
                color.extend_from_slice(&v.to_be_bytes());
            }
            if let Some(alpha) = alpha.as_mut() {
                opaque &= px[color_channels] == u16::MAX;
                alpha.extend_from_slice(&px[color_channels].to_be_bytes());
            }
        }
        let alpha = alpha.filter(|_| !opaque);
        Self::assemble(width, height, color_channels, true, color, alpha, icc, fingerprint)
    }

    #[allow(clippy::too_many_arguments)]
    fn assemble(
        width: u32,
        height: u32,
        color_channels: usize,
        sixteen_bit: bool,
        color: Vec<u8>,
        alpha: Option<Vec<u8>>,
        icc: Option<Vec<u8>>,
        fingerprint: u128,
    ) -> Self {
        let colorspace = match color_channels {
            1 => ImageColorspace::Luma,
            4 => ImageColorspace::Cmyk,
            _ => ImageColorspace::Rgb,
        };
        Self {
            width,
            height,
            colorspace,
            sixteen_bit,
            color: Arc::new(color),
            alpha: alpha.map(Arc::new),
            icc: icc.map(Arc::new),
            fingerprint,
        }
    }

    /// An 8-bit sRGB-ish RGBA copy, longest side at most `max_px`, for previews and thumbnails.
    pub fn proxy(&self, max_px: u32) -> RgbaImage {
        let channels = match self.colorspace {
            ImageColorspace::Luma => 1,
            ImageColorspace::Rgb => 3,
            ImageColorspace::Cmyk => 4,
        };
        let step = if self.sixteen_bit { 2 } else { 1 };
        let mut rgba = RgbaImage::new(self.width, self.height);
        for (i, px) in rgba.pixels_mut().enumerate() {
            let base = i * channels * step;
            let sample = |c: usize| self.color[base + c * step];
            let [r, g, b] = match self.colorspace {
                ImageColorspace::Luma => [sample(0); 3],
                ImageColorspace::Rgb => [sample(0), sample(1), sample(2)],
                ImageColorspace::Cmyk => {
                    let k = 255 - sample(3) as u16;
                    let ch = |c: usize| ((255 - sample(c) as u16) * k / 255) as u8;
                    [ch(0), ch(1), ch(2)]
                }
            };
            let a = self.alpha.as_ref().map(|a| a[i * step]).unwrap_or(255);
            *px = image::Rgba([r, g, b, a]);
        }
        downscale(DynamicImage::ImageRgba8(rgba), max_px)
    }
}

pub fn downscale(image: DynamicImage, max_px: u32) -> RgbaImage {
    let longest = image.width().max(image.height());
    if longest <= max_px {
        return image.into_rgba8();
    }
    let scale = max_px as f64 / longest as f64;
    let w = ((image.width() as f64 * scale).round() as u32).max(1);
    let h = ((image.height() as f64 * scale).round() as u32).max(1);
    image.resize_exact(w, h, image::imageops::FilterType::Triangle).into_rgba8()
}

pub fn fingerprint(bytes: &[u8]) -> u128 {
    // FNV-1a over a sample of the file plus its length: identity only, not security.
    let mut hash: u128 = 0x6c62272e07bb014262b821756295c58d;
    let stride = (bytes.len() / 65536).max(1);
    for b in bytes.iter().step_by(stride) {
        hash ^= *b as u128;
        hash = hash.wrapping_mul(0x0000000001000000000000000000013B);
    }
    hash ^ bytes.len() as u128
}

/// EXIF orientation (1–8) of an encoded image, defaulting to 1.
pub fn exif_orientation(bytes: &[u8]) -> u8 {
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_decoder().ok())
        .and_then(|mut d| d.orientation().ok())
        .map(|o| o.to_exif())
        .unwrap_or(1)
}

/// Decode any format the `image` crate reads, keeping 16-bit depth and the ICC profile and
/// applying EXIF orientation to the pixels.
pub fn decode_with_image_crate(bytes: &[u8]) -> Result<DecodedImage, String> {
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?;
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let icc = decoder.icc_profile().ok().flatten();
    let orientation = decoder.orientation().ok();
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    if let Some(orientation) = orientation {
        image.apply_orientation(orientation);
    }
    Ok(from_dynamic(image, icc, fingerprint(bytes)))
}

/// Decoded (not passthrough) pixels from a format krilla embeds natively; used for proxies.
pub fn decode_for_proxy(bytes: &[u8], max_px: u32) -> Result<RgbaImage, String> {
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?;
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let orientation = decoder.orientation().ok();
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    if let Some(orientation) = orientation {
        image.apply_orientation(orientation);
    }
    Ok(downscale(image, max_px))
}

fn from_dynamic(image: DynamicImage, icc: Option<Vec<u8>>, fingerprint: u128) -> DecodedImage {
    let (w, h) = (image.width(), image.height());
    match image {
        DynamicImage::ImageLuma8(i) => DecodedImage::from_interleaved_u8(w, h, 1, false, i.as_raw(), icc, fingerprint),
        DynamicImage::ImageLumaA8(i) => DecodedImage::from_interleaved_u8(w, h, 2, true, i.as_raw(), icc, fingerprint),
        DynamicImage::ImageRgb8(i) => DecodedImage::from_interleaved_u8(w, h, 3, false, i.as_raw(), icc, fingerprint),
        DynamicImage::ImageRgba8(i) => DecodedImage::from_interleaved_u8(w, h, 4, true, i.as_raw(), icc, fingerprint),
        DynamicImage::ImageLuma16(i) => DecodedImage::from_interleaved_u16(w, h, 1, false, i.as_raw(), icc, fingerprint),
        DynamicImage::ImageLumaA16(i) => DecodedImage::from_interleaved_u16(w, h, 2, true, i.as_raw(), icc, fingerprint),
        DynamicImage::ImageRgb16(i) => DecodedImage::from_interleaved_u16(w, h, 3, false, i.as_raw(), icc, fingerprint),
        DynamicImage::ImageRgba16(i) => DecodedImage::from_interleaved_u16(w, h, 4, true, i.as_raw(), icc, fingerprint),
        other if other.color().has_alpha() => {
            let i = other.into_rgba16();
            DecodedImage::from_interleaved_u16(w, h, 4, true, i.as_raw(), icc, fingerprint)
        }
        other => {
            let i = other.into_rgb16();
            DecodedImage::from_interleaved_u16(w, h, 3, false, i.as_raw(), icc, fingerprint)
        }
    }
}

/// JPEG XL via jxl-oxide. The ICC returned describes the rendered pixels exactly.
pub fn decode_jxl(bytes: &[u8]) -> Result<DecodedImage, String> {
    use jxl_oxide::{JxlImage, PixelFormat};
    let image = JxlImage::builder().read(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let icc = image.rendered_icc();
    let sixteen = image.image_header().metadata.bit_depth.bits_per_sample() > 8;
    let (channels, has_alpha) = match image.pixel_format() {
        PixelFormat::Gray => (1, false),
        PixelFormat::Graya => (2, true),
        PixelFormat::Rgb => (3, false),
        PixelFormat::Rgba => (4, true),
        PixelFormat::Cmyk => (4, false),
        PixelFormat::Cmyka => (5, true),
    };
    let render = image.render_frame(0).map_err(|e| e.to_string())?;
    let mut stream = render.stream();
    let (w, h) = (stream.width(), stream.height());
    let len = (w * h) as usize * stream.channels() as usize;
    let print = fingerprint(bytes);
    let icc = (!icc.is_empty()).then_some(icc);
    if sixteen {
        let mut buf = vec![0u16; len];
        stream.write_to_buffer(&mut buf);
        Ok(DecodedImage::from_interleaved_u16(w, h, channels, has_alpha, &buf, icc, print))
    } else {
        let mut buf = vec![0u8; len];
        stream.write_to_buffer(&mut buf);
        Ok(DecodedImage::from_interleaved_u8(w, h, channels, has_alpha, &buf, icc, print))
    }
}
