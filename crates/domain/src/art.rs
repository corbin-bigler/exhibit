//! How a piece of art sits inside its slot.

use crate::geometry::{Rect, Size};
use crate::items::ArtKind;
use crate::settings::Scaling;
use crate::units::POINTS_PER_INCH;

/// Where art is drawn on the sheet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArtFrame {
    /// The art's full box on the sheet, after rotation. With `Fill` it extends past the slot.
    pub rect: Rect,
    /// The art is turned 90° clockwise to match the slot's orientation.
    pub rotated: bool,
}

pub fn frame_art(art: Size, target: Rect, scaling: Scaling) -> ArtFrame {
    let slot = target.size();
    let rotated = (art.is_landscape() && slot.is_portrait()) || (art.is_portrait() && slot.is_landscape());
    let turned = if rotated { art.transposed() } else { art };
    let sx = slot.width / turned.width;
    let sy = slot.height / turned.height;
    let scale = match scaling {
        Scaling::Fill => sx.max(sy),
        Scaling::Fit => sx.min(sy),
    };
    let width = turned.width * scale;
    let height = turned.height * scale;
    ArtFrame {
        rect: Rect::new(
            target.x + (target.width - width) / 2.0,
            target.y + (target.height - height) / 2.0,
            width,
            height,
        ),
        rotated,
    }
}

/// Pixels per printed inch, or `None` for vector art.
pub fn effective_dpi(kind: ArtKind, frame: &ArtFrame) -> Option<f64> {
    match kind {
        ArtKind::Vector => None,
        ArtKind::Raster { width_px, height_px } => {
            let across = if frame.rotated { height_px } else { width_px };
            Some(across as f64 / (frame.rect.width / POINTS_PER_INCH))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::inches;

    // L-07
    #[test]
    fn fill_covers_and_centres() {
        let slot = Rect::new(0.0, 0.0, inches(5.0), inches(7.0));
        let frame = frame_art(Size::new(1000.0, 1000.0), slot, Scaling::Fill);
        assert!(!frame.rotated);
        assert!((frame.rect.width - inches(7.0)).abs() < 1e-9);
        assert!((frame.rect.x + inches(1.0)).abs() < 1e-9);
    }

    // L-08
    #[test]
    fn fit_letterboxes() {
        let slot = Rect::new(0.0, 0.0, inches(5.0), inches(7.0));
        let frame = frame_art(Size::new(1000.0, 1000.0), slot, Scaling::Fit);
        assert!((frame.rect.height - inches(5.0)).abs() < 1e-9);
        assert!((frame.rect.y - inches(1.0)).abs() < 1e-9);
    }

    // L-09, I-08
    #[test]
    fn landscape_art_turns_for_a_portrait_slot() {
        let slot = Rect::new(0.0, 0.0, inches(5.0), inches(7.0));
        let frame = frame_art(Size::new(2100.0, 1500.0), slot, Scaling::Fill);
        assert!(frame.rotated);
        assert!((frame.rect.width - inches(5.0)).abs() < 1e-9);
        assert!((frame.rect.height - inches(7.0)).abs() < 1e-9);
        let dpi = effective_dpi(ArtKind::Raster { width_px: 2100, height_px: 1500 }, &frame).unwrap();
        assert!((dpi - 300.0).abs() < 1e-9);
        assert_eq!(effective_dpi(ArtKind::Vector, &frame), None);
    }
}
