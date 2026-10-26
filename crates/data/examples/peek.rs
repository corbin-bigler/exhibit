//! Rasterise a region of a PDF page to PNG for close inspection:
//! `cargo run -p exhibit-data --example peek -- in.pdf out.png <page> <scale> [x y w h in points]`

use std::sync::Arc;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let pdf = hayro::hayro_syntax::Pdf::new(Arc::new(std::fs::read(&args[0]).unwrap())).unwrap();
    let page = &pdf.pages()[args[2].parse::<usize>().unwrap()];
    let scale: f32 = args[3].parse().unwrap();
    let pixmap = hayro::render(
        page,
        &RenderCache::new(),
        &InterpreterSettings::default(),
        &RenderSettings::default(),
        &PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE },
    );
    let mut image = image::RgbaImage::from_raw(
        pixmap.width() as u32,
        pixmap.height() as u32,
        pixmap.data_as_u8_slice().to_vec(),
    )
    .unwrap();
    if args.len() >= 8 {
        let n: Vec<f32> = args[4..8].iter().map(|v| v.parse::<f32>().unwrap() * scale).collect();
        image = image::imageops::crop_imm(&image, n[0] as u32, n[1] as u32, n[2] as u32, n[3] as u32).to_image();
    }
    image.save(&args[1]).unwrap();
}
