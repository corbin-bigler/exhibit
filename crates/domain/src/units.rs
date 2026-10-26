//! Display units. Everything inside the domain is stored in points; units only exist at the edges.

pub const POINTS_PER_INCH: f64 = 72.0;
pub const MILLIMETERS_PER_INCH: f64 = 25.4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LengthUnit {
    #[default]
    Inch,
    Millimeter,
}

impl LengthUnit {
    pub fn to_points(self, value: f64) -> f64 {
        match self {
            LengthUnit::Inch => value * POINTS_PER_INCH,
            LengthUnit::Millimeter => value / MILLIMETERS_PER_INCH * POINTS_PER_INCH,
        }
    }

    pub fn from_points(self, points: f64) -> f64 {
        match self {
            LengthUnit::Inch => points / POINTS_PER_INCH,
            LengthUnit::Millimeter => points / POINTS_PER_INCH * MILLIMETERS_PER_INCH,
        }
    }
}

pub const fn inches(value: f64) -> f64 {
    value * POINTS_PER_INCH
}

pub const fn millimeters(value: f64) -> f64 {
    value / MILLIMETERS_PER_INCH * POINTS_PER_INCH
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_between_units() {
        assert!((LengthUnit::Inch.to_points(1.0) - 72.0).abs() < 1e-9);
        assert!((LengthUnit::Millimeter.to_points(25.4) - 72.0).abs() < 1e-9);
        let pt = LengthUnit::Millimeter.to_points(297.0);
        assert!((LengthUnit::Millimeter.from_points(pt) - 297.0).abs() < 1e-9);
        assert!((LengthUnit::Inch.from_points(pt) - 297.0 / 25.4).abs() < 1e-9);
    }
}
