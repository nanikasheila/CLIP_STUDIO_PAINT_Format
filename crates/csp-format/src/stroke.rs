//! Drawing vector layers (R10): CSP saves no pixels for vector layers and
//! draws them from `VectorObjectList` when it opens a file, with its brush
//! engine. This is an approximation of that engine for the common case of
//! pens: each object is a round brush swept along its points, with the
//! size and opacity following the pen pressure through the brush style's
//! effector curves.
//!
//! What is read (big endian):
//! - `BrushStyle.SizeEffector` / `OpacityEffector`: `u32 flags`, then for
//!   flag `0x10` (pressure) `f32 minimum, u32 curve` (a
//!   `BrushEffectorGraphData.MainId`); other flags are not used here.
//! - `BrushEffectorGraphData.ControlPoints`: the first `ControlNumber`
//!   `f64 x, f64 y` pairs, the control points of a curve from pressure (x)
//!   to the size or opacity factor (y).
//! - Point value 0 (`+36`, f32) is the pressure; size and opacity at a
//!   point are `base × (min + (1 − min) × curve(pressure))`.
//!
//! Measured against CSP's exports of strokes of known pressure and size
//! (`docs/validation/V6-made-files.md`, section 7):
//!
//! - The object's brush size is the radius: at factor 1 a stroke is
//!   `2 × size` wide.
//! - The curve is the clamped quadratic B-spline of its control points
//!   (three points: one quadratic Bézier with the middle point as its
//!   control); past the last point's x it continues as `y × x / x_last`.
//! - The edge: no anti-aliasing (`AntiAlias` 0) fills the pixels whose
//!   centre is within the radius; otherwise the ink across a stroke is
//!   `2 × radius` less a loss that depends on the level and the radius
//!   (radius 20: 0.65 / 1.1 / 1.4 for weak / medium / strong; radius 2–3:
//!   1.1 / 1.5 / 1.6) over a ramp of about one pixel, and about `radius`
//!   for strokes under 2 px wide.
//! - Effector flag 0x40 follows the velocity, point value 2 (0..1): the
//!   factor is `1 − (1 − min) × v`, `min` the word after the pressure's.
//!   Flag 0x80 is a random factor per dab down to its minimum (drawn as a
//!   fixed factor); 0x100 does not change straight strokes.
//! - Header 88 objects (freehand strokes) are a cubic spline through
//!   their points; header 92 objects are drawn straight from point to point
//!   and without velocity.
//! - The tip is an ellipse: `ThicknessBase` (times `ThicknessEffector`,
//!   read like the size) is the ratio of its minor to its major axis, and
//!   the major semi-axis is the radius. The minor axis points at
//!   `RotationBase` degrees (y down) when `RotationEffector` has bit 0x01,
//!   at 0 without it, plus the direction of the line with bit 0x40; bit
//!   0x80 turns each dab at random by up to ±`RotationRandom` × 180°,
//!   which is drawn as a round tip 30% of the way from the mean width of
//!   the dabs of that range to the widest. `BrushStyle.StyleFlag` bit 0x40 turns the tip a further
//!   quarter (the angle then names the major axis). A stroke is as wide as the ellipse across the line; the
//!   anti-aliasing loss grows with how long the ellipse is across the line
//!   against along it (× 0.4 to × 1.75).
//!
//! Not drawn: brush tips other than round, textures, rotation following
//! pressure or tilt, spray, watercolour and other brush effects.

use std::collections::HashMap;

use crate::clip::ClipFile;
use crate::database::{Row, select, table_exists, value_as_text};
use crate::error::Result;
use crate::image::RgbaImage;
use crate::model::Layer;
use crate::vector::{VectorData, VectorObject};

/// A pressure curve: its control points, in order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Curve {
    pub points: Vec<(f64, f64)>,
}

impl Curve {
    /// The control points in `data`: all of them, or the first `count`.
    pub fn parse(data: &[u8], count: Option<usize>) -> Self {
        let points: Vec<(f64, f64)> = data
            .as_chunks::<16>()
            .0
            .iter()
            .take(count.unwrap_or(usize::MAX))
            .map(|pair| {
                let x = f64::from_be_bytes(pair[..8].try_into().expect("8"));
                let y = f64::from_be_bytes(pair[8..].try_into().expect("8"));
                (x, y)
            })
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .collect();
        Curve { points }
    }

    /// The curve at `x` in 0..1 (identity when empty).
    pub fn at(&self, x: f64) -> f64 {
        let x = x.clamp(0.0, 1.0);
        let points = &self.points;
        let (Some(first), Some(last)) = (points.first(), points.last()) else { return x };
        if x <= first.0 {
            return first.1;
        }
        if x >= last.0 {
            return if last.0 > 0.0 { last.1 * x / last.0 } else { last.1 };
        }
        if points.len() == 2 {
            return first.1 + (last.1 - first.1) * (x - first.0) / (last.0 - first.0);
        }
        // Quadratic Bézier pieces between the midpoints of the control
        // polygon, starting at the first point and ending at the last.
        let mid = |a: (f64, f64), b: (f64, f64)| ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        let n = points.len();
        for i in 1..n - 1 {
            let start = if i == 1 { points[0] } else { mid(points[i - 1], points[i]) };
            let end = if i == n - 2 { points[n - 1] } else { mid(points[i], points[i + 1]) };
            if x > end.0 && i < n - 2 {
                continue;
            }
            let control = points[i];
            let along = |t: f64, k: fn((f64, f64)) -> f64| {
                (1.0 - t) * (1.0 - t) * k(start) + 2.0 * t * (1.0 - t) * k(control) + t * t * k(end)
            };
            let (mut low, mut high) = (0.0, 1.0);
            for _ in 0..40 {
                let t = (low + high) / 2.0;
                if along(t, |p| p.0) < x {
                    low = t;
                } else {
                    high = t;
                }
            }
            return along((low + high) / 2.0, |p| p.1);
        }
        last.1
    }
}

/// How one brush setting follows the pen: the flags, each followed by its
/// words in this order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Effector {
    /// Flag 0x10: `(minimum, curve)`, following the pressure.
    pub pressure: Option<(f64, Curve)>,
    /// Flag 0x40: the minimum at full speed (point value 2).
    pub velocity: Option<f64>,
    /// Flag 0x80: the minimum of a random factor per dab.
    pub random: Option<f64>,
}

impl Effector {
    const PRESSURE: u32 = 0x10;
    const TILT: u32 = 0x20;
    const VELOCITY: u32 = 0x40;
    const RANDOM: u32 = 0x80;

    pub fn parse(data: &[u8], curves: &HashMap<i64, Curve>) -> Self {
        let word = |i: usize| data.get(i * 4..i * 4 + 4).map(|b| u32::from_be_bytes(b.try_into().expect("4")));
        let real = |i: usize| (word(i).map(f32::from_bits).unwrap_or(0.0) as f64).clamp(0.0, 1.0);
        let Some(flags) = word(0) else { return Effector::default() };
        let mut effector = Effector::default();
        let mut next = 1;
        if flags & Self::PRESSURE != 0 {
            let curve = word(next + 1).and_then(|id| curves.get(&(id as i64)).cloned()).unwrap_or_default();
            effector.pressure = Some((real(next), curve));
            next += 2;
        }
        if flags & Self::TILT != 0 {
            next += 1;
        }
        if flags & Self::VELOCITY != 0 {
            effector.velocity = Some(real(next));
            next += 1;
        }
        if flags & Self::RANDOM != 0 {
            effector.random = Some(real(next));
        }
        effector
    }

    /// The factor (0..1) at `pressure` and `velocity` (point values 0 and 2)
    /// for a size or thickness. Velocity v takes `1 − (1 − min) × v`. A
    /// random factor per dab is drawn as a fixed `1 − (1 − min) × u`: CSP's
    /// stroke is the union of its dabs, so the wider the range the nearer
    /// the stroke is to its widest dabs (u = 0.42 for a range of 0.11, 0.15
    /// for 0.74, 0.125 for 0.8, measured).
    pub fn factor(&self, pressure: f64, velocity: f64) -> f64 {
        self.factor_with(pressure, velocity, |range| (0.467 - 0.428 * range).clamp(0.1, 0.5))
    }

    /// The factor for an opacity: a random opacity per dab is drawn as
    /// `1 − 0.42 × (1 − min)` (measured with minimum 0.51 on straight test
    /// lines and on real strokes).
    pub fn opacity_factor(&self, pressure: f64, velocity: f64) -> f64 {
        self.factor_with(pressure, velocity, |_| 0.42)
    }

    fn factor_with(&self, pressure: f64, velocity: f64, share: impl Fn(f64) -> f64) -> f64 {
        let mut factor = match &self.pressure {
            Some((minimum, curve)) => minimum + (1.0 - minimum) * curve.at(pressure).clamp(0.0, 1.0),
            None => 1.0,
        };
        if let Some(minimum) = self.velocity {
            factor *= 1.0 - (1.0 - minimum) * velocity.clamp(0.0, 1.0);
        }
        if let Some(minimum) = self.random {
            let range = 1.0 - minimum;
            factor *= 1.0 - range * share(range);
        }
        factor
    }
}

/// The parts of a `BrushStyle` row the approximation uses.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Brush {
    pub size: Effector,
    pub opacity: Effector,
    pub tip: Tip,
    /// `AntiAlias`: 0 none … 3 strong.
    pub anti_alias: i64,
}

/// Brush styles of a file by `BrushStyle.MainId`.
pub fn brushes(file: &ClipFile) -> Result<HashMap<i64, Brush>> {
    let mut curves = HashMap::new();
    if table_exists(file.database(), "BrushEffectorGraphData")? {
        for row in select(file.database(), "SELECT * FROM BrushEffectorGraphData", [])? {
            let count = row.opt_int("ControlNumber").and_then(|n| usize::try_from(n).ok());
            curves.insert(row.int("MainId"), Curve::parse(row.blob("ControlPoints").unwrap_or_default(), count));
        }
    }
    let mut brushes = HashMap::new();
    if table_exists(file.database(), "BrushStyle")? {
        for row in select(file.database(), "SELECT * FROM BrushStyle", [])? {
            brushes.insert(row.int("MainId"), brush(&row, &curves));
        }
    }
    Ok(brushes)
}

fn brush(row: &Row, curves: &HashMap<i64, Curve>) -> Brush {
    Brush {
        size: Effector::parse(row.blob("SizeEffector").unwrap_or_default(), curves),
        opacity: Effector::parse(row.blob("OpacityEffector").unwrap_or_default(), curves),
        tip: Tip {
            thickness: row.opt_real("ThicknessBase").unwrap_or(1.0),
            thickness_effector: Effector::parse(row.blob("ThicknessEffector").unwrap_or_default(), curves),
            rotation: row.opt_real("RotationBase").unwrap_or(0.0),
            rotation_flags: row.opt_int("RotationEffector").unwrap_or(0),
            rotation_random: row.opt_real("RotationRandom").unwrap_or(0.0),
            turned: row.opt_int("StyleFlag").unwrap_or(0) & 0x40 != 0,
        },
        anti_alias: row.opt_int("AntiAlias").unwrap_or(1),
    }
}

/// The shape of the brush tip: an ellipse.
#[derive(Debug, Clone, PartialEq)]
pub struct Tip {
    /// `ThicknessBase`: minor over major axis.
    pub thickness: f64,
    pub thickness_effector: Effector,
    /// `RotationBase`, degrees.
    pub rotation: f64,
    /// `RotationEffector` bits.
    pub rotation_flags: i64,
    /// `RotationRandom`, 0..1.
    pub rotation_random: f64,
    /// `StyleFlag` bit 0x40: the tip is turned a quarter, so the angle
    /// names its major axis.
    pub turned: bool,
}

impl Default for Tip {
    fn default() -> Self {
        Tip {
            thickness: 1.0,
            thickness_effector: Effector::default(),
            rotation: 0.0,
            rotation_flags: 0,
            rotation_random: 0.0,
            turned: false,
        }
    }
}

impl Tip {
    const FIXED: i64 = 0x01;
    const DIRECTION: i64 = 0x40;
    const RANDOM: i64 = 0x80;

    fn round(&self) -> bool {
        self.thickness >= 1.0 && self.thickness_effector == Effector::default()
    }

    /// The thickness at `pressure` and `velocity`.
    fn thickness_at(&self, pressure: f64, velocity: f64) -> f64 {
        (self.thickness.min(1.0) * self.thickness_effector.factor(pressure, velocity)).max(0.0)
    }

    /// The direction of the minor axis (radians, y down) on a line going along `u`.
    fn angle(&self, u: (f64, f64)) -> f64 {
        let mut angle = if self.rotation_flags & Self::FIXED != 0 { self.rotation.to_radians() } else { 0.0 };
        if self.rotation_flags & Self::DIRECTION != 0 {
            angle += u.1.atan2(u.0);
        }
        if self.turned {
            angle += std::f64::consts::FRAC_PI_2;
        }
        angle
    }

    /// Whether the tip is drawn as an ellipse.
    fn elliptic(&self) -> bool {
        !self.round()
    }

    /// How wide a stroke of `radius` and `thickness` going along the unit
    /// vector `u` is.
    fn across(&self, radius: f64, thickness: f64, u: (f64, f64)) -> Across {
        if thickness >= 1.0 {
            return Across { half: radius, loss: 1.0, circle: false };
        }
        let angle = self.angle(u);
        // The ellipse's half extent along the unit vector v, its minor axis at `turn`.
        let extent = |turn: f64, v: (f64, f64)| {
            let (sin, cos) = turn.sin_cos();
            let along_minor = v.0 * cos + v.1 * sin;
            let along_major = -v.0 * sin + v.1 * cos;
            ((radius * thickness * along_minor).powi(2) + (radius * along_major).powi(2)).sqrt()
        };
        let normal = (-u.1, u.0);
        // The anti-aliasing loss grows with how thin the tip is along the line.
        let loss = |turn: f64| (extent(turn, normal) / extent(turn, u).max(1e-9)).clamp(0.4, 1.75);
        if self.rotation_flags & Self::RANDOM != 0 && self.rotation_random > 0.0 {
            // Turned at random by up to ±`RotationRandom` × 180° per dab: the
            // dabs overlap, so the stroke is wider than the mean width (CSP,
            // synthetic lines with seeds: 0.3 of the way to the widest).
            let range = self.rotation_random.min(1.0) * std::f64::consts::PI;
            let turns: Vec<f64> = (0..=32).map(|k| angle + range * (k as f64 / 16.0 - 1.0)).collect();
            let mean = |f: &dyn Fn(f64) -> f64| turns.iter().map(|&turn| f(turn)).sum::<f64>() / turns.len() as f64;
            let widest = turns.iter().map(|&turn| extent(turn, normal)).fold(0.0, f64::max);
            let average = mean(&|turn| extent(turn, normal));
            return Across { half: average + 0.3 * (widest - average), loss: mean(&loss), circle: true };
        }
        Across { half: extent(angle, normal), loss: loss(angle), circle: false }
    }
}

/// A tip across a line.
#[derive(Clone, Copy)]
struct Across {
    /// Half the width of the stroke.
    half: f64,
    /// The factor on the anti-aliasing loss.
    loss: f64,
    /// Whether the tip sweeps as a circle of radius `half` (a tip turned
    /// at random).
    circle: bool,
}

/// An ellipse as the map that takes it to the unit circle.
#[derive(Clone, Copy)]
struct Ellipse {
    /// Unit minor and major axes, over their semi-axes.
    minor: (f64, f64),
    major: (f64, f64),
}

impl Ellipse {
    fn new(radius: f64, thickness: f64, angle: f64) -> Self {
        let (sin, cos) = angle.sin_cos();
        let (a, b) = (radius.max(0.3), (radius * thickness).max(0.3));
        Ellipse { minor: (cos / b, sin / b), major: (-sin / a, cos / a) }
    }

    fn map(&self, v: (f64, f64)) -> (f64, f64) {
        (self.minor.0 * v.0 + self.minor.1 * v.1, self.major.0 * v.0 + self.major.1 * v.1)
    }

    /// Signed distance in pixels from `v` (relative to the centre) to the
    /// edge, to first order.
    fn distance(&self, v: (f64, f64)) -> f64 {
        let w = self.map(v);
        let rho = (w.0 * w.0 + w.1 * w.1).sqrt();
        if rho < 1e-9 {
            return -1.0 / self.minor.0.hypot(self.minor.1).max(self.major.0.hypot(self.major.1));
        }
        let (n0, n1) = (w.0 / rho, w.1 / rho);
        let gradient = (n0 * self.minor.0 + n1 * self.major.0).hypot(n0 * self.minor.1 + n1 * self.major.1);
        (rho - 1.0) / gradient
    }
}

/// The vector data of a layer (all its `VectorObjectList` rows, in order).
pub fn layer_vectors(file: &ClipFile, layer: &Layer) -> Result<Vec<VectorData>> {
    if !table_exists(file.database(), "VectorObjectList")? {
        return Ok(Vec::new());
    }
    let rows =
        select(file.database(), "SELECT * FROM VectorObjectList WHERE LayerId = ?1 ORDER BY MainId", [layer.main_id])?;
    Ok(rows
        .iter()
        .filter_map(|row| row.get("VectorData").and_then(value_as_text))
        .filter_map(|id| file.external(&id))
        .filter_map(|payload| VectorData::parse(payload).ok())
        .collect())
}

/// Where vector coordinates land in the target image: `target = (p + offset) × scale`.
#[derive(Debug, Clone, Copy)]
pub struct Placement {
    pub offset: (f64, f64),
    pub scale: f64,
}

/// Draw vector objects onto `image` (straight alpha, normal blending, in order).
pub fn draw_objects(image: &mut RgbaImage, objects: &[VectorObject], brushes: &HashMap<i64, Brush>, at: Placement) {
    let fallback = Brush::default();
    for object in objects {
        let brush = object.brush_styles().iter().find_map(|id| brushes.get(&(*id as i64))).unwrap_or(&fallback);
        draw_object(image, object, brush, at);
    }
}

/// How the edge of a stroke is drawn, by `BrushStyle.AntiAlias`.
#[derive(Clone, Copy)]
struct Edge {
    /// Ink lost across the stroke, in pixels, at the radii of `RADII`
    /// (None: no anti-aliasing).
    loss: Option<[f64; 8]>,
}

impl Edge {
    /// Radii at which the loss was measured (V6 section 7).
    const RADII: [f64; 8] = [1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 10.0, 20.0];

    fn of(anti_alias: i64) -> Self {
        let loss = match anti_alias {
            ..=0 => None,
            1 => Some([1.05, 1.25, 1.13, 1.05, 0.91, 0.91, 0.82, 0.65]),
            2 => Some([1.05, 1.25, 1.45, 1.49, 1.31, 1.18, 1.15, 1.09]),
            _ => Some([1.05, 1.25, 1.45, 1.71, 1.68, 1.47, 1.44, 1.42]),
        };
        Edge { loss }
    }

    /// The ink lost across a stroke of `radius` (linear between the measured radii).
    fn loss_at(table: &[f64; 8], radius: f64) -> f64 {
        let r = radius.clamp(Self::RADII[0], Self::RADII[7]);
        let i = Self::RADII.iter().rposition(|&k| k <= r).unwrap_or(0).min(6);
        let t = (r - Self::RADII[i]) / (Self::RADII[i + 1] - Self::RADII[i]);
        table[i] + t * (table[i + 1] - table[i])
    }

    /// Coverage of a pixel whose centre is `distance` from the spine of a
    /// stroke of `radius`.
    fn coverage(self, radius: f64, distance: f64, loss_factor: f64) -> f64 {
        let Some(table) = self.loss else { return if distance <= radius { 1.0 } else { 0.0 } };
        // The ink across the stroke; thin strokes keep about their radius.
        let ink = (2.0 * radius - Self::loss_at(&table, radius) * loss_factor).max(radius);
        if ink >= 1.0 { (ink / 2.0 - distance + 0.5).clamp(0.0, 1.0) } else { ink * (1.0 - distance).clamp(0.0, 1.0) }
    }
}

#[derive(Clone, Copy)]
struct Stamp {
    x: f64,
    y: f64,
    radius: f64,
    thickness: f64,
    alpha: f64,
    /// Point flag 0x2: the stroke turns sharply here.
    corner: bool,
}

/// The points of a stroke joined by a cubic spline through them (header
/// 88 objects; CSP draws the others with straight lines), cut into pieces
/// that stay within a fifth of a pixel of the curve; the other values
/// change linearly along each span. Measured on CSP's exports (V6 section
/// 7): the tangent at a point is 0.7 × (next − previous), at an open end
/// 0.55 × (neighbour − end), and none at a corner (point flag 0x2).
fn smooth(points: &[Stamp], closed: bool) -> Vec<Stamp> {
    let n = points.len();
    if n < 3 {
        return points.to_vec();
    }
    // A closed stroke repeats its first point at the end.
    let unique = if closed { n - 1 } else { n };
    let tangent = |i: usize| -> (f64, f64) {
        let p = &points[i % unique];
        if p.corner {
            return (0.0, 0.0);
        }
        let (prev, next) = if closed {
            (&points[(i + unique - 1) % unique], &points[(i + 1) % unique])
        } else if i == 0 {
            return (0.55 * (points[1].x - p.x), 0.55 * (points[1].y - p.y));
        } else if i == n - 1 {
            return (0.55 * (p.x - points[n - 2].x), 0.55 * (p.y - points[n - 2].y));
        } else {
            (&points[i - 1], &points[i + 1])
        };
        (0.7 * (next.x - prev.x), 0.7 * (next.y - prev.y))
    };
    let mut out = vec![points[0]];
    for i in 0..n - 1 {
        let (p1, p2) = (&points[i], &points[i + 1]);
        let (t1, t2) = (tangent(i), tangent(i + 1));
        // Bézier control points of the span.
        let b1 = (p1.x + t1.0 / 3.0, p1.y + t1.1 / 3.0);
        let b2 = (p2.x - t2.0 / 3.0, p2.y - t2.1 / 3.0);
        let (cx, cy) = (p2.x - p1.x, p2.y - p1.y);
        let chord = cx.hypot(cy).max(1e-9);
        let off = |b: (f64, f64)| ((b.0 - p1.x) * cy - (b.1 - p1.y) * cx).abs() / chord;
        let pieces = ((off(b1).max(off(b2)) / 0.2).sqrt().ceil() as usize).clamp(1, 64);
        for k in 1..=pieces {
            let t = k as f64 / pieces as f64;
            let u = 1.0 - t;
            let bezier = |a: f64, b: f64, c: f64, d: f64| {
                u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d
            };
            let mix = |a: f64, b: f64| a + t * (b - a);
            out.push(Stamp {
                x: bezier(p1.x, b1.0, b2.0, p2.x),
                y: bezier(p1.y, b1.1, b2.1, p2.y),
                radius: mix(p1.radius, p2.radius),
                thickness: mix(p1.thickness, p2.thickness),
                alpha: mix(p1.alpha, p2.alpha),
                corner: false,
            });
        }
    }
    out
}

fn draw_object(image: &mut RgbaImage, object: &VectorObject, brush: &Brush, at: Placement) {
    let size = object.brush_size().unwrap_or(1.0).max(0.0) * at.scale;
    let opacity = object.opacity().clamp(0.0, 1.0);
    let freehand = object.header.len() == 88;
    let mut stamps: Vec<Stamp> = (0..object.points.len())
        .filter_map(|index| object.point(index))
        .map(|point| {
            // Header 92 objects draw as if at rest (CSP draws them straight too).
            let velocity = if freehand { point.values[2] as f64 } else { 0.0 };
            let pressure = point.values[0] as f64;
            Stamp {
                x: (point.x + at.offset.0) * at.scale,
                y: (point.y + at.offset.1) * at.scale,
                radius: size * brush.size.factor(pressure, velocity),
                thickness: brush.tip.thickness_at(pressure, velocity),
                corner: point.flags & 0x2 != 0,
                alpha: opacity * brush.opacity.opacity_factor(pressure, velocity),
            }
        })
        .collect();
    if stamps.is_empty() {
        return;
    }
    if object.is_closed() && stamps.len() > 2 {
        stamps.push(stamps[0]);
    }
    let stamps = if freehand { smooth(&stamps, object.is_closed() && stamps.len() > 3) } else { stamps };
    // Coverage of the whole object (the union of its segments), then one
    // blend: a pen stroke does not darken where it overlaps itself.
    let margin = stamps.iter().map(|s| s.radius).fold(0.0, f64::max) + 2.0;
    let x0 = (stamps.iter().map(|s| s.x).fold(f64::MAX, f64::min) - margin).floor().max(0.0) as i64;
    let y0 = (stamps.iter().map(|s| s.y).fold(f64::MAX, f64::min) - margin).floor().max(0.0) as i64;
    let x1 = ((stamps.iter().map(|s| s.x).fold(f64::MIN, f64::max) + margin).ceil() as i64).min(image.width as i64);
    let y1 = ((stamps.iter().map(|s| s.y).fold(f64::MIN, f64::max) + margin).ceil() as i64).min(image.height as i64);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let mut coverage = vec![0f32; w * h];
    let edge = Edge::of(brush.anti_alias);
    let elliptic = brush.tip.elliptic();
    let segments: Vec<(&Stamp, &Stamp)> = if stamps.len() == 1 {
        vec![(&stamps[0], &stamps[0])]
    } else {
        stamps.windows(2).map(|p| (&p[0], &p[1])).collect()
    };
    for (a, b) in segments {
        let reach = a.radius.max(b.radius) + 1.0;
        let sx0 = ((a.x.min(b.x) - reach).floor() as i64).max(x0);
        let sy0 = ((a.y.min(b.y) - reach).floor() as i64).max(y0);
        let sx1 = ((a.x.max(b.x) + reach).ceil() as i64).min(x1);
        let sy1 = ((a.y.max(b.y) + reach).ceil() as i64).min(y1);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let length2 = dx * dx + dy * dy;
        // Half widths across this segment at its ends.
        let u = if length2 > 0.0 { (dx / length2.sqrt(), dy / length2.sqrt()) } else { (1.0, 0.0) };
        let (across_a, across_b) =
            (brush.tip.across(a.radius, a.thickness, u), brush.tip.across(b.radius, b.thickness, u));
        let (half_a, half_b) = (across_a.half, across_b.half);
        // An elliptic tip sweeps the ellipse along the segment: the nearest
        // point is found where the ellipse is a circle.
        let angle = brush.tip.angle(u);
        let ellipse = |t: f64| {
            if across_a.circle {
                return Ellipse::new(half_a + t * (half_b - half_a), 1.0, angle);
            }
            let radius = a.radius + t * (b.radius - a.radius);
            Ellipse::new(radius, a.thickness + t * (b.thickness - a.thickness), angle)
        };
        let middle = ellipse(0.5);
        let d = middle.map((dx, dy));
        let mapped2 = d.0 * d.0 + d.1 * d.1;
        for py in sy0..sy1 {
            for px in sx0..sx1 {
                let (qx, qy) = (px as f64 + 0.5, py as f64 + 0.5);
                let (t, distance) = if elliptic {
                    let q = middle.map((qx - a.x, qy - a.y));
                    let t = if mapped2 > 0.0 { ((q.0 * d.0 + q.1 * d.1) / mapped2).clamp(0.0, 1.0) } else { 0.0 };
                    let half = half_a + t * (half_b - half_a);
                    (t, half + ellipse(t).distance((qx - a.x - t * dx, qy - a.y - t * dy)))
                } else {
                    let t = if length2 > 0.0 {
                        (((qx - a.x) * dx + (qy - a.y) * dy) / length2).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    (t, (qx - a.x - t * dx).hypot(qy - a.y - t * dy))
                };
                let radius = half_a + t * (half_b - half_a);
                let loss = across_a.loss + t * (across_b.loss - across_a.loss);
                let alpha = a.alpha + t * (b.alpha - a.alpha);
                let inside = edge.coverage(radius, distance, loss);
                let value = (inside * alpha) as f32;
                let cell = &mut coverage[(py - y0) as usize * w + (px - x0) as usize];
                if value > *cell {
                    *cell = value;
                }
            }
        }
    }
    let color = object.main_color().map(|c| c as f32 / 255.0);
    for y in 0..h {
        for x in 0..w {
            let k = coverage[y * w + x];
            if k <= 0.0 {
                continue;
            }
            let (ix, iy) = ((x0 + x as i64) as u32, (y0 + y as i64) as u32);
            let p = image.pixel(ix, iy);
            let ba = p[3] as f32 / 255.0;
            let oa = k + ba * (1.0 - k);
            let mix = |c: usize| (color[c] * k + p[c] as f32 / 255.0 * ba * (1.0 - k)) / oa.max(1e-6);
            let out = [mix(0), mix(1), mix(2), oa].map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8);
            image.set_pixel(ix, iy, out);
        }
    }
}

/// Draw a vector layer at `scale` into an image of `width × height` (canvas
/// pixels × scale). None when the layer has no vector data.
pub fn draw_layer(file: &ClipFile, layer: &Layer, width: u32, height: u32, scale: f64) -> Result<Option<RgbaImage>> {
    let vectors = layer_vectors(file, layer)?;
    if vectors.is_empty() {
        return Ok(None);
    }
    let brushes = brushes(file)?;
    let mut image = RgbaImage::new(width, height);
    let at = Placement { offset: (layer.offset.0 as f64, layer.offset.1 as f64), scale };
    for data in &vectors {
        draw_objects(&mut image, &data.objects, &brushes, at);
    }
    Ok(Some(image))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn curve(points: &[(f64, f64)]) -> Curve {
        let data: Vec<u8> = points.iter().flat_map(|(x, y)| [x.to_be_bytes(), y.to_be_bytes()].concat()).collect();
        Curve::parse(&data, Some(points.len()))
    }

    #[test]
    fn curve_is_the_quadratic_b_spline_of_its_points() {
        // Factors read off CSP's exports of strokes of known pressure
        // (V6 section 7), to within the measurement.
        let pen = curve(&[(0.0, 0.0), (0.6270833, 0.2636364), (1.0, 1.0)]);
        for (x, csp) in [(0.3, 0.161), (0.5, 0.324), (0.7, 0.537), (0.9, 0.818)] {
            assert!((pen.at(x) - csp).abs() < 0.01, "{x}: {} vs {csp}", pen.at(x));
        }
        let five = curve(&[(0.0, 0.0), (0.2, 0.6), (0.5, 0.3), (0.8, 0.9), (1.0, 1.0)]);
        for (x, csp) in [(0.2, 0.428), (0.4, 0.416), (0.7, 0.69), (0.9, 0.934)] {
            assert!((five.at(x) - csp).abs() < 0.01, "{x}: {} vs {csp}", five.at(x));
        }
        // ControlNumber limits the points read; past the last point the
        // curve is y × x / x_last.
        let data: Vec<u8> = [0.0f64, 0.0, 0.3, 0.0, 0.6, 1.0, 1.0, 1.0].iter().flat_map(|v| v.to_be_bytes()).collect();
        let three = Curve::parse(&data, Some(3));
        assert!((three.at(0.9) - 1.5).abs() < 1e-9);
        assert_eq!(Curve::default().at(0.3), 0.3);
    }

    #[test]
    fn stroke_width_is_twice_the_size_less_the_anti_aliasing() {
        let width = |anti_alias: i64, radius: f64| -> f64 {
            let edge = Edge::of(anti_alias);
            (-40..=40).map(|y| edge.coverage(radius, (y as f64 + 0.5 - 0.3).abs(), 1.0)).sum()
        };
        // CSP: radius 20 → 40, 39.35, 38.91, 38.58 px of ink.
        assert_eq!(width(0, 20.0), 40.0);
        assert!((width(1, 20.0) - 39.35).abs() < 0.1);
        assert!((width(2, 20.0) - 38.91).abs() < 0.1);
        assert!((width(3, 20.0) - 38.58).abs() < 0.2);
        // Thin strokes lose more (CSP radius 3: 4.91–4.99 px; radius 2.5: 3.48 px).
        assert!((width(1, 3.0) - 4.95).abs() < 0.15);
        assert!((width(2, 2.5) - 3.48).abs() < 0.15);
        // A thin stroke keeps about its radius of ink (CSP 0.5 at 0.5).
        assert!((width(2, 0.5) - 0.5).abs() < 0.1);
    }

    #[test]
    fn tip_thickness_and_rotation_set_the_width_across_the_line() {
        let pen = Edge::of(2);
        let width = |tip: &Tip, radius: f64, thickness: f64, degrees: f64| -> f64 {
            let u = (degrees.to_radians().cos(), degrees.to_radians().sin());
            let across = tip.across(radius, thickness, u);
            (-60..=60).map(|y| pen.coverage(across.half, (y as f64 + 0.5 - 0.3).abs(), across.loss)).sum()
        };
        let tip =
            |rotation: f64, rotation_flags: i64| Tip { thickness: 0.4, rotation, rotation_flags, ..Tip::default() };
        // CSP, radius 20, thickness 0.4, lines at 0°, 22.5° … 157.5°.
        let csp = [37.9, 35.5, 29.3, 20.4, 15.5, 20.4, 29.3, 35.6];
        for (k, csp) in csp.iter().enumerate() {
            let degrees = 22.5 * k as f64;
            // Rotation without bit 0x01 is not used.
            assert!((width(&tip(45.0, 0), 20.0, 0.4, degrees) - csp).abs() < 0.5, "{degrees}");
            // Bit 0x01: the minor axis at 45°.
            assert!((width(&tip(45.0, 1), 20.0, 0.4, degrees + 45.0) - csp).abs() < 0.5, "{degrees}");
            // Bit 0x40: turned with the line, so every line is as wide as the 45° one.
            assert!((width(&tip(45.0, 0x43), 20.0, 0.4, degrees) - 29.0).abs() < 0.5, "{degrees}");
        }
        // Thickness following the pressure (0.5 at half pressure).
        let mut curves = HashMap::new();
        curves.insert(1, Curve { points: vec![(0.0, 0.0), (1.0, 1.0)] });
        let effector = [0x11u32, 0, 1].iter().flat_map(|w| w.to_be_bytes()).collect::<Vec<_>>();
        let follows = Tip { thickness_effector: Effector::parse(&effector, &curves), ..Tip::default() };
        let thickness = follows.thickness_at(0.5, 0.0);
        assert!((width(&follows, 20.0, thickness, 0.0) - 37.9).abs() < 0.5);
        assert!((width(&follows, 20.0, thickness, 90.0) - 19.2).abs() < 0.5);
        assert!((width(&follows, 20.0, follows.thickness_at(1.0, 0.0), 90.0) - 38.9).abs() < 0.5);
        // StyleFlag 0x40 turns the tip a quarter: the 0° line is now the thin one.
        let turned = Tip { turned: true, ..follows };
        assert!((width(&turned, 20.0, thickness, 90.0) - 37.9).abs() < 0.5);
        assert!((width(&turned, 20.0, thickness, 0.0) - 19.2).abs() < 0.5);
        // Turned at random (CSP, synthetic lines with seeds, thickness 0.51,
        // radius up to 20): RotationRandom 0.3 on a tip across the line keeps
        // 0.96 of the ink; a fixed tip turned fully at random draws every line
        // alike, at 0.94 of the widest (this model: 0.85 at radius 20, 1.0 at 4).
        let tip = |flags: i64, random: f64| Tip {
            thickness: 0.51,
            rotation: 181.5,
            rotation_flags: flags,
            rotation_random: random,
            ..Tip::default()
        };
        let across = width(&tip(0x43, 0.0), 20.0, 0.51, 0.0);
        assert!((width(&tip(0xc3, 0.3), 20.0, 0.51, 0.0) / across - 0.963).abs() < 0.04);
        let random: Vec<f64> =
            [0.0, 45.0, 90.0].iter().map(|&d| width(&tip(0x81, 1.0), 20.0, 0.51, d) / across).collect();
        assert!(random.iter().all(|w| (w - random[0]).abs() < 0.02 && (w - 0.94).abs() < 0.1), "{random:?}");
    }

    #[test]
    fn freehand_strokes_are_a_spline_through_their_points() {
        let stamp = |x: f64, y: f64| Stamp { x, y, radius: 4.0, thickness: 1.0, alpha: 1.0, corner: false };
        let v = smooth(&[stamp(60.0, 60.0), stamp(340.0, 340.0), stamp(620.0, 60.0)], false);
        // The centre of CSP's stroke through these points (V6 section 7).
        let y_at = |x: f64| {
            let w = v.windows(2).find(|w| w[0].x <= x && x <= w[1].x).unwrap();
            w[0].y + (w[1].y - w[0].y) * (x - w[0].x) / (w[1].x - w[0].x)
        };
        for (x, csp) in [(150.0, 188.1), (200.0, 256.1), (250.0, 305.5), (300.0, 333.2)] {
            assert!((y_at(x) - csp).abs() < 4.0, "{x}: {} vs {csp}", y_at(x));
        }
        // A corner (point flag 0x2) keeps the lines straight.
        let mut corner = stamp(340.0, 340.0);
        corner.corner = true;
        let straight = smooth(&[stamp(60.0, 60.0), corner, stamp(620.0, 60.0)], false);
        assert!(straight.iter().all(|s| (s.y - if s.x <= 340.0 { s.x } else { 680.0 - s.x }).abs() < 1e-6));
    }

    #[test]
    fn effector_reads_pressure_minimum() {
        let mut curves = HashMap::new();
        curves.insert(4, Curve { points: vec![(0.0, 0.0), (1.0, 1.0)] });
        let data = [0x11u32, 0.5f32.to_bits(), 4].iter().flat_map(|w| w.to_be_bytes()).collect::<Vec<_>>();
        let effector = Effector::parse(&data, &curves);
        assert_eq!(effector.factor(0.0, 0.0), 0.5);
        assert_eq!(effector.factor(1.0, 0.0), 1.0);
        assert_eq!(Effector::parse(&1u32.to_be_bytes(), &curves).factor(0.0, 0.0), 1.0);
        // Pressure and velocity (CSP: minimum 0.6 → 0.8 at velocity 0.5,
        // minimum 0.2 → 0.4 at 0.75).
        let both = [0x51u32, 0, 4, 0.6f32.to_bits()].iter().flat_map(|w| w.to_be_bytes()).collect::<Vec<_>>();
        assert!((Effector::parse(&both, &curves).factor(1.0, 0.5) - 0.8).abs() < 1e-6);
        let velocity = [0x41u32, 0.2f32.to_bits()].iter().flat_map(|w| w.to_be_bytes()).collect::<Vec<_>>();
        assert!((Effector::parse(&velocity, &curves).factor(1.0, 0.75) - 0.4).abs() < 1e-6);
        // Random (CSP, real strokes: size minimum 0.89 → 0.954, opacity
        // minimum 0.51 → 0.796).
        let random =
            |minimum: f32| Effector::parse(&[0x81u32, minimum.to_bits()].map(u32::to_be_bytes).concat(), &curves);
        assert!((random(0.89).factor(1.0, 0.0) - 0.954).abs() < 0.005);
        assert!((random(0.51).opacity_factor(1.0, 0.0) - 0.796).abs() < 0.005);
    }
}
