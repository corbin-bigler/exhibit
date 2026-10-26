//! Writes sample artwork for manual testing: `cargo run -p exhibit-data --example fixtures -- <dir>`.

use image::{ImageBuffer, ImageEncoder, Rgb};
use krilla::geom::{PathBuilder, Rect};
use krilla::page::PageSettings;
use krilla::paint::Fill;

const SRGB_PROFILE: &str = r"C:\Windows\System32\spool\drivers\color\sRGB Color Space Profile.icm";

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("output directory"));
    std::fs::create_dir_all(&dir).unwrap();
    let icc = std::fs::read(SRGB_PROFILE).ok();

    // Two-page vector PDF: 5×7 postcards, front and back.
    let mut document = krilla::Document::new();
    for (bg, sun, land) in [((249, 217, 198), (237, 28, 36), (158, 59, 38)), ((31, 59, 77), (255, 231, 232), (61, 191, 176))] {
        let mut page = document.start_page_with(PageSettings::from_wh(360.0, 504.0).unwrap());
        let mut s = page.surface();
        let fill = |c: (u8, u8, u8)| Some(Fill { paint: krilla::color::rgb::Color::new(c.0, c.1, c.2).into(), ..Default::default() });
        let rect = |x: f32, y: f32, w: f32, h: f32| {
            let mut b = PathBuilder::new();
            b.push_rect(Rect::from_xywh(x, y, w, h).unwrap());
            b.finish().unwrap()
        };
        s.set_fill(fill(bg));
        s.draw_path(&rect(0.0, 0.0, 360.0, 504.0));
        s.set_fill(fill(sun));
        let mut circle = PathBuilder::new();
        let (cx, cy, r, k) = (245.0, 170.0, 80.0, 0.5523 * 80.0);
        circle.move_to(cx + r, cy);
        circle.cubic_to(cx + r, cy + k, cx + k, cy + r, cx, cy + r);
        circle.cubic_to(cx - k, cy + r, cx - r, cy + k, cx - r, cy);
        circle.cubic_to(cx - r, cy - k, cx - k, cy - r, cx, cy - r);
        circle.cubic_to(cx + k, cy - r, cx + r, cy - k, cx + r, cy);
        circle.close();
        s.draw_path(&circle.finish().unwrap());
        s.set_fill(fill(land));
        let mut hills = PathBuilder::new();
        hills.move_to(0.0, 374.0);
        for (x, y) in [(86.0, 310.0), (144.0, 331.0), (223.0, 266.0), (360.0, 353.0), (360.0, 504.0), (0.0, 504.0)] {
            hills.line_to(x, y);
        }
        hills.close();
        s.draw_path(&hills.finish().unwrap());
        s.finish();
        page.finish();
    }
    std::fs::write(dir.join("desert-sunset.pdf"), document.finish().unwrap()).unwrap();

    // 16-bit TIFF with an ICC profile.
    let img: ImageBuffer<Rgb<u16>, Vec<u16>> = ImageBuffer::from_fn(1500, 2100, |x, y| {
        Rgb([(x * 43) as u16, (y * 31) as u16, ((x + y) * 17) as u16])
    });
    let mut tiff = std::io::Cursor::new(Vec::new());
    let mut encoder = image::codecs::tiff::TiffEncoder::new(&mut tiff);
    if let Some(icc) = &icc {
        encoder.set_icc_profile(icc.clone()).unwrap();
    }
    let raw: Vec<u8> = img.as_raw().iter().flat_map(|v| v.to_ne_bytes()).collect();
    encoder.write_image(&raw, 1500, 2100, image::ExtendedColorType::Rgb16).unwrap();
    std::fs::write(dir.join("gradient-16bit.tiff"), tiff.into_inner()).unwrap();

    // Landscape-stored JPEG that EXIF says to rotate 90° (like a phone portrait shot).
    let photo = ImageBuffer::from_fn(1200, 800, |x, y| {
        if y < 80 { Rgb([237, 28, 36]) } else { Rgb([(x / 5) as u8, 120, (y / 4) as u8]) }
    });
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
        .write_image(photo.as_raw(), 1200, 800, image::ExtendedColorType::Rgb8)
        .unwrap();
    std::fs::write(dir.join("phone-portrait.jpg"), with_exif_orientation(&jpeg, 6)).unwrap();

    std::fs::write(
        dir.join("zion-sticker.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="300" height="300"><circle cx="150" cy="150" r="140" fill="#ED1C24"/><path d="M60 210 L120 110 L150 150 L190 80 L245 210 Z" fill="#fff"/></svg>"##,
    )
    .unwrap();

    // One small picture in every other raster format the app reads.
    let card = ImageBuffer::from_fn(900, 1260, |x, y| {
        let band = (y / 140) as u8;
        Rgb([200u8.wrapping_sub(band * 15), (x / 4) as u8, 90 + band * 12])
    });
    let save = |name: &str, format: image::ImageFormat| {
        image::DynamicImage::ImageRgb8(card.clone()).save_with_format(dir.join(name), format).unwrap();
    };
    save("menu-front.png", image::ImageFormat::Png);
    save("menu-front.webp", image::ImageFormat::WebP);
    save("menu-front.gif", image::ImageFormat::Gif);
    save("menu-front.bmp", image::ImageFormat::Bmp);
    save("menu-front.tga", image::ImageFormat::Tga);
    save("menu-front.qoi", image::ImageFormat::Qoi);
    save("menu-front.ppm", image::ImageFormat::Pnm);

    // 16-bit PNG with transparency and an ICC profile.
    let sticker: ImageBuffer<image::Rgba<u16>, Vec<u16>> = ImageBuffer::from_fn(1200, 1200, |x, y| {
        let (dx, dy) = (x as f32 - 600.0, y as f32 - 600.0);
        let inside = dx * dx + dy * dy < 560.0 * 560.0;
        image::Rgba([60000, (x * 40) as u16, (y * 30) as u16, if inside { 65535 } else { 0 }])
    });
    let mut png = Vec::new();
    let mut encoder = image::codecs::png::PngEncoder::new(&mut png);
    if let Some(icc) = &icc {
        encoder.set_icc_profile(icc.clone()).unwrap();
    }
    let raw: Vec<u8> = sticker.as_raw().iter().flat_map(|v| v.to_be_bytes()).collect();
    encoder.write_image(&raw, 1200, 1200, image::ExtendedColorType::Rgba16).unwrap();
    std::fs::write(dir.join("round-sticker-16bit-alpha.png"), png).unwrap();

    // 8K photo-sized JPEG: switch to By grid 6 × 10 to trigger the heavy-sheet warning.
    let huge = ImageBuffer::from_fn(7680, 4320, |x, y| Rgb([(x / 30) as u8, (y / 17) as u8, ((x + y) / 48) as u8]));
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85)
        .write_image(huge.as_raw(), 7680, 4320, image::ExtendedColorType::Rgb8)
        .unwrap();
    std::fs::write(dir.join("huge-8k-photo.jpg"), jpeg).unwrap();

    // Low-resolution JPEG: shows the amber DPI badge at 5 × 7.
    let small = ImageBuffer::from_fn(500, 700, |x, y| Rgb([(x / 2) as u8, 160, (y / 3) as u8]));
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
        .write_image(small.as_raw(), 500, 700, image::ExtendedColorType::Rgb8)
        .unwrap();
    std::fs::write(dir.join("low-res-flyer.jpg"), jpeg).unwrap();

    // Not artwork, and a broken file: both should be reported without blocking the rest.
    std::fs::write(dir.join("notes.txt"), "not artwork").unwrap();
    std::fs::write(dir.join("broken.png"), b"this is not really a png").unwrap();

    println!("wrote fixtures to {}", dir.display());
}

/// Insert a minimal APP1 Exif segment carrying only an orientation tag.
fn with_exif_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
    let mut tiff = vec![b'M', b'M', 0, 42, 0, 0, 0, 8, 0, 1];
    tiff.extend_from_slice(&[0x01, 0x12, 0, 3, 0, 0, 0, 1]);
    tiff.extend_from_slice(&orientation.to_be_bytes());
    tiff.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    let mut payload = b"Exif\0\0".to_vec();
    payload.extend_from_slice(&tiff);
    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(&payload);
    out.extend_from_slice(&jpeg[2..]);
    out
}
