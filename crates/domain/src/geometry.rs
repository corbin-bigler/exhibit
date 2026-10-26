//! Sheet-space geometry. Units are PDF points (1/72 in); origin top-left, y grows down.

/// Tolerance for comparing positions that came out of floating point arithmetic.
pub const EPSILON: f64 = 1e-6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl Size {
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    pub fn is_landscape(&self) -> bool {
        self.width > self.height + EPSILON
    }

    pub fn is_portrait(&self) -> bool {
        self.height > self.width + EPSILON
    }

    pub fn transposed(&self) -> Self {
        Self::new(self.height, self.width)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self { x, y, width, height }
    }

    pub fn right(&self) -> f64 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }

    pub fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    pub fn outset(&self, sides: Sides) -> Rect {
        Rect::new(
            self.x - sides.left,
            self.y - sides.top,
            self.width + sides.left + sides.right,
            self.height + sides.top + sides.bottom,
        )
    }

    pub fn union(&self, other: &Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Rect::new(x, y, self.right().max(other.right()) - x, self.bottom().max(other.bottom()) - y)
    }

    /// Whether the open intervals of the two rects overlap on the x axis.
    pub fn overlaps_horizontally(&self, other: &Rect) -> bool {
        self.x < other.right() - EPSILON && other.x < self.right() - EPSILON
    }

    /// Whether the open intervals of the two rects overlap on the y axis.
    pub fn overlaps_vertically(&self, other: &Rect) -> bool {
        self.y < other.bottom() - EPSILON && other.y < self.bottom() - EPSILON
    }
}

/// A per-side amount, e.g. how far bleed extends past each trim edge.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Sides {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Sides {
    pub const fn uniform(value: f64) -> Self {
        Self { left: value, top: value, right: value, bottom: value }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}
