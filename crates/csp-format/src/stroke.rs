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
//! - `BrushEffectorGraphData.ControlPoints`: `f64 x, f64 y` pairs, a curve
//!   from pressure (x) to the size or opacity factor (y).
//! - Point value 0 (`+36`, f32) is the pressure; size and opacity at a
//!   point are `base × (min + (1 − min) × curve(pressure))`.
//!
//! Not drawn: brush tips other than round, textures, thickness and
//! rotation of the tip, spray, watercolour and other brush effects.

use std::collections::HashMap;

use crate::clip::ClipFile;
use crate::database::{Row, select, table_exists, value_as_text};
use crate::error::Result;
use crate::image::RgbaImage;
use crate::model::Layer;
use crate::vector::{VectorData, VectorObject};

/// A pressure curve: control points sorted by x.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Curve {
    pub points: Vec<(f64, f64)>,
}

impl Curve {
    pub fn parse(data: &[u8]) -> Self {
        let mut points: Vec<(f64, f64)> = data
            .as_chunks::<16>()
            .0
            .iter()
            .map(|pair| {
                let x = f64::from_be_bytes(pair[..8].try_into().expect("8"));
                let y = f64::from_be_bytes(pair[8..].try_into().expect("8"));
                (x, y)
            })
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .collect();
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        Curve { points }
    }

    /// The curve at `x` (linear between control points; identity when empty).
    pub fn at(&self, x: f64) -> f64 {
        let x = x.clamp(0.0, 1.0);
        let Some(first) = self.points.first() else { return x };
        if x <= first.0 {
            return first.1;
        }
        for pair in self.points.windows(2) {
            let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
            if x <= x1 {
                return if x1 > x0 { y0 + (y1 - y0) * (x - x0) / (x1 - x0) } else { y1 };
            }
        }
        self.points.last().map(|p| p.1).unwrap_or(x)
    }
}

/// How one brush setting follows the pen pressure.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Effector {
    /// `(minimum, curve)` when the setting follows the pressure.
    pub pressure: Option<(f64, Curve)>,
}

impl Effector {
    const PRESSURE: u32 = 0x10;

    pub fn parse(data: &[u8], curves: &HashMap<i64, Curve>) -> Self {
        let word = |i: usize| data.get(i * 4..i * 4 + 4).map(|b| u32::from_be_bytes(b.try_into().expect("4")));
        let Some(flags) = word(0) else { return Effector::default() };
        if flags & Self::PRESSURE == 0 {
            return Effector::default();
        }
        let minimum = word(1).map(f32::from_bits).unwrap_or(0.0) as f64;
        let curve = word(2).and_then(|id| curves.get(&(id as i64)).cloned()).unwrap_or_default();
        Effector { pressure: Some((minimum.clamp(0.0, 1.0), curve)) }
    }

    /// The factor (0..1) at `pressure`.
    pub fn factor(&self, pressure: f64) -> f64 {
        match &self.pressure {
            Some((minimum, curve)) => minimum + (1.0 - minimum) * curve.at(pressure).clamp(0.0, 1.0),
            None => 1.0,
        }
    }
}

/// The parts of a `BrushStyle` row the approximation uses.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Brush {
    pub size: Effector,
    pub opacity: Effector,
    /// `AntiAlias`: 0 none … 3 strong.
    pub anti_alias: i64,
}

/// Brush styles of a file by `BrushStyle.MainId`.
pub fn brushes(file: &ClipFile) -> Result<HashMap<i64, Brush>> {
    let mut curves = HashMap::new();
    if table_exists(file.database(), "BrushEffectorGraphData")? {
        for row in select(file.database(), "SELECT * FROM BrushEffectorGraphData", [])? {
            curves.insert(row.int("MainId"), Curve::parse(row.blob("ControlPoints").unwrap_or_default()));
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
        anti_alias: row.opt_int("AntiAlias").unwrap_or(1),
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
        let brush = object.brush_styles().first().and_then(|id| brushes.get(&(*id as i64))).unwrap_or(&fallback);
        draw_object(image, object, brush, at);
    }
}

struct Stamp {
    x: f64,
    y: f64,
    radius: f64,
    alpha: f64,
}

fn draw_object(image: &mut RgbaImage, object: &VectorObject, brush: &Brush, at: Placement) {
    let size = object.brush_size().unwrap_or(1.0).max(0.0) * at.scale;
    let opacity = object.opacity().clamp(0.0, 1.0);
    let mut stamps: Vec<Stamp> = (0..object.points.len())
        .filter_map(|index| object.point(index))
        .map(|point| {
            let pressure = point.values[0] as f64;
            Stamp {
                x: (point.x + at.offset.0) * at.scale,
                y: (point.y + at.offset.1) * at.scale,
                radius: size * brush.size.factor(pressure) / 2.0,
                alpha: opacity * brush.opacity.factor(pressure),
            }
        })
        .collect();
    if stamps.is_empty() {
        return;
    }
    if object.is_closed() && stamps.len() > 2 {
        let first = &stamps[0];
        stamps.push(Stamp { x: first.x, y: first.y, radius: first.radius, alpha: first.alpha });
    }
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
    let soft = if brush.anti_alias <= 0 { 0.0 } else { 1.0 };
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
        for py in sy0..sy1 {
            for px in sx0..sx1 {
                let (qx, qy) = (px as f64 + 0.5, py as f64 + 0.5);
                let t =
                    if length2 > 0.0 { (((qx - a.x) * dx + (qy - a.y) * dy) / length2).clamp(0.0, 1.0) } else { 0.0 };
                let (cx, cy) = (a.x + t * dx, a.y + t * dy);
                let distance = ((qx - cx).powi(2) + (qy - cy).powi(2)).sqrt();
                let radius = a.radius + t * (b.radius - a.radius);
                let alpha = a.alpha + t * (b.alpha - a.alpha);
                let inside = if soft > 0.0 {
                    (radius - distance + 0.5).clamp(0.0, 1.0)
                } else if distance <= radius.max(0.5) {
                    1.0
                } else {
                    0.0
                };
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

    #[test]
    fn curve_interpolates() {
        let mut data = Vec::new();
        for (x, y) in [(0.0f64, 0.0f64), (0.5, 0.25), (1.0, 1.0)] {
            data.extend(x.to_be_bytes());
            data.extend(y.to_be_bytes());
        }
        let curve = Curve::parse(&data);
        assert_eq!(curve.at(0.25), 0.125);
        assert_eq!(curve.at(0.75), 0.625);
        assert_eq!(Curve::default().at(0.3), 0.3);
    }

    #[test]
    fn effector_reads_pressure_minimum() {
        let mut curves = HashMap::new();
        curves.insert(4, Curve { points: vec![(0.0, 0.0), (1.0, 1.0)] });
        let data = [0x11u32, 0.5f32.to_bits(), 4].iter().flat_map(|w| w.to_be_bytes()).collect::<Vec<_>>();
        let effector = Effector::parse(&data, &curves);
        assert_eq!(effector.factor(0.0), 0.5);
        assert_eq!(effector.factor(1.0), 1.0);
        assert_eq!(Effector::parse(&1u32.to_be_bytes(), &curves).factor(0.0), 1.0);
    }
}
