//! Screen-space bounds shared by observations, targets, and elements.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A point in screen coordinates (physical pixels).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: i64,
    /// Vertical coordinate.
    pub y: i64,
}

impl Point {
    /// Create a new point.
    pub const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }
}

/// An axis-aligned bounding rectangle in screen coordinates.
///
/// `min` is the top-left corner, `max` the bottom-right (exclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    /// Top-left corner.
    pub min: Point,
    /// Bottom-right corner.
    pub max: Point,
}

impl Serialize for Bounds {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.to_array().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Bounds {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let values = <[i64; 4]>::deserialize(deserializer)?;
        Ok(Self::from_array(values))
    }
}

impl Bounds {
    /// Create bounds from `[x0, y0, x1, y1]` — matching the JSON
    /// representation used in the protocol.
    pub const fn new(x0: i64, y0: i64, x1: i64, y1: i64) -> Self {
        Self {
            min: Point::new(x0, y0),
            max: Point::new(x1, y1),
        }
    }

    /// Width in pixels.
    pub fn width(&self) -> i64 {
        (self.max.x - self.min.x).max(0)
    }

    /// Height in pixels.
    pub fn height(&self) -> i64 {
        (self.max.y - self.min.y).max(0)
    }

    /// Whether the point lies within the rectangle.
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.min.x && p.x < self.max.x && p.y >= self.min.y && p.y < self.max.y
    }

    /// The center point of the rectangle (truncated).
    pub fn center(&self) -> Point {
        Point::new(
            self.min.x + self.width() / 2,
            self.min.y + self.height() / 2,
        )
    }

    /// Whether this rectangle overlaps another.
    pub fn intersects(&self, other: &Bounds) -> bool {
        self.min.x < other.max.x
            && other.min.x < self.max.x
            && self.min.y < other.max.y
            && other.min.y < self.max.y
    }

    /// Convert from the `[x0, y0, x1, y1]` array form used in the wire format.
    pub fn from_array(v: [i64; 4]) -> Self {
        Self::new(v[0], v[1], v[2], v[3])
    }

    /// Convert to the `[x0, y0, x1, y1]` array form used in the wire format.
    pub fn to_array(&self) -> [i64; 4] {
        [self.min.x, self.min.y, self.max.x, self.max.y]
    }
}

impl From<[i64; 4]> for Bounds {
    fn from(v: [i64; 4]) -> Self {
        Self::from_array(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_of_known_rect() {
        let b = Bounds::new(800, 600, 920, 650);
        assert_eq!(b.center(), Point::new(860, 625));
    }

    #[test]
    fn contains_point() {
        let b = Bounds::new(0, 0, 10, 10);
        assert!(b.contains(Point::new(5, 5)));
        assert!(!b.contains(Point::new(10, 10))); // exclusive max
        assert!(!b.contains(Point::new(-1, 5)));
    }

    #[test]
    fn array_roundtrip() {
        let b = Bounds::from_array([800, 600, 920, 650]);
        assert_eq!(b.to_array(), [800, 600, 920, 650]);
    }

    #[test]
    fn intersects_detects_overlap() {
        let a = Bounds::new(0, 0, 10, 10);
        let b = Bounds::new(5, 5, 15, 15);
        let c = Bounds::new(20, 20, 30, 30);
        assert!(a.intersects(&b));
        assert!(!a.intersects(&c));
    }

    #[test]
    fn serializes_as_array() {
        let b = Bounds::new(800, 600, 920, 650);
        let s = serde_json::to_value(b).unwrap();
        assert_eq!(s, serde_json::json!([800, 600, 920, 650]));
    }
}
