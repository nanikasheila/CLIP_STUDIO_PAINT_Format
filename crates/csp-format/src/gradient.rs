//! Gradient layers (`Layer.GradationFillInfo`, R10): CSP saves no pixels
//! for them and draws the gradient when it opens the file.
//!
//! Big endian, the same section layout as `LayerEffectInfo`: `u32 total,
//! u32 2`, then sections of `u32 n, UTF-16BE name (n units), payload`:
//!
//! ```text
//! GradationData
//!   u32 size (bytes after this field)
//!   u32 header size (16 or 24, counted from here)  u32 stop size (28)
//!   u32 stop count  u32 16  [u32 ?, u32 ? when the header is 24 bytes]
//!   stop × count:
//!     u32 R, G, B, A (16-bit value repeated in both halves)
//!     u32 flags (1 main colour, 2 sub colour, 0x10 curved to the next stop)
//!     u32 position (0..=0x8000)   u32 points in the curve (flag 0x10)
//!   curve × (stops with flag 0x10): f64 x, y pairs (0..1)
//! GradationSetting
//!   u32 ?  u32 shape (0 linear, 1 circular)  u32 ?
//!   f64 ellipse width, height, rotation (degrees; circular shapes)
//!   f64 start x, y, end x, y (canvas pixels)
//! GradationSettingAdd0001 (not used here)
//! ```
//!
//! Colours between stops are interpolated in straight (not premultiplied)
//! RGBA, along the curve when the stop has one. Outside the start and end
//! the end colours continue.

use crate::error::{Error, Result};
use crate::image::RgbaImage;

#[derive(Debug, Clone, PartialEq)]
pub struct Stop {
    /// Straight RGBA, 0..1.
    pub color: [f64; 4],
    pub flags: u32,
    /// 0..1 along the gradient.
    pub position: f64,
    /// Control points (x, y) shaping the change to the next stop.
    pub curve: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Linear,
    Circular,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    pub stops: Vec<Stop>,
    pub shape: Shape,
    /// Ellipse width and height (pixels) and rotation (degrees).
    pub ellipse: [f64; 3],
    pub start: (f64, f64),
    pub end: (f64, f64),
}

fn word(data: &[u8], offset: usize) -> Result<u32> {
    data.get(offset..offset + 4)
        .map(|b| u32::from_be_bytes(b.try_into().expect("4")))
        .ok_or_else(|| Error::format("GradationFillInfo is cut short"))
}

fn real(data: &[u8], offset: usize) -> Result<f64> {
    data.get(offset..offset + 8)
        .map(|b| f64::from_be_bytes(b.try_into().expect("8")))
        .ok_or_else(|| Error::format("GradationFillInfo is cut short"))
}

/// The sections (name, payload) of a GradationFillInfo blob. Payload
/// sizes differ by section and version, so a section runs to the next
/// name (`u32 n` and n UTF-16 units spelling `Gradation…`).
fn sections(data: &[u8]) -> Vec<(String, &[u8])> {
    let total = word(data, 0).map(|t| t as usize).unwrap_or(0).min(data.len());
    let name_at = |at: usize| -> Option<(String, usize)> {
        let n = word(data, at).ok()? as usize;
        let units: Vec<u16> =
            data.get(at + 4..at + 4 + 2 * n)?.chunks_exact(2).map(|u| u16::from_be_bytes([u[0], u[1]])).collect();
        let name = String::from_utf16(&units).ok()?;
        (n >= 3 && name.starts_with("Gradation") && name.chars().all(|c| c.is_ascii_alphanumeric()))
            .then_some((name, at + 4 + 2 * n))
    };
    let mut names = Vec::new();
    let mut at = 8;
    while at + 4 <= total {
        match name_at(at) {
            Some((name, end)) => {
                names.push((at, name, end));
                at = end;
            }
            None => at += 2,
        }
    }
    names
        .iter()
        .enumerate()
        .map(|(i, (_, name, start))| {
            let end = names.get(i + 1).map(|next| next.0).unwrap_or(total);
            (name.clone(), &data[*start..end])
        })
        .collect()
}

impl Gradient {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let sections = sections(data);
        let find = |name: &str| {
            sections
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, payload)| *payload)
                .ok_or_else(|| Error::format(format!("GradationFillInfo has no {name}")))
        };
        let stops_data = find("GradationData")?;
        let header = word(stops_data, 4)? as usize;
        let stop_size = word(stops_data, 8)? as usize;
        let count = word(stops_data, 12)? as usize;
        if stop_size < 28 || count == 0 || count > 256 {
            return Err(Error::format(format!("GradationData: {count} stops of {stop_size} bytes")));
        }
        let mut stops = Vec::with_capacity(count);
        let mut curve_offset = 4 + header + count * stop_size;
        for index in 0..count {
            let at = 4 + header + index * stop_size;
            let channel = |i: usize| -> Result<f64> { Ok((word(stops_data, at + 4 * i)? >> 16) as f64 / 65535.0) };
            let color = [channel(0)?, channel(1)?, channel(2)?, channel(3)?];
            let flags = word(stops_data, at + 16)?;
            let position = word(stops_data, at + 20)? as f64 / 32768.0;
            let mut curve = Vec::new();
            if flags & 0x10 != 0 {
                let points = (word(stops_data, at + 24)? as usize).min(64);
                for p in 0..points {
                    curve
                        .push((real(stops_data, curve_offset + 16 * p)?, real(stops_data, curve_offset + 16 * p + 8)?));
                }
                curve_offset += 16 * points;
            }
            stops.push(Stop { color, flags, position: position.clamp(0.0, 1.0), curve });
        }
        stops.sort_by(|a, b| a.position.total_cmp(&b.position));
        let setting = find("GradationSetting")?;
        let shape = if word(setting, 4)? == 1 { Shape::Circular } else { Shape::Linear };
        let r = |i: usize| real(setting, 12 + 8 * i);
        Ok(Gradient { stops, shape, ellipse: [r(0)?, r(1)?, r(2)?], start: (r(3)?, r(4)?), end: (r(5)?, r(6)?) })
    }

    /// Position (0..1, clamped) of the canvas point `(x, y)`.
    pub fn position(&self, x: f64, y: f64) -> f64 {
        let t = match self.shape {
            Shape::Linear => {
                let (dx, dy) = (self.end.0 - self.start.0, self.end.1 - self.start.1);
                let length2 = dx * dx + dy * dy;
                if length2 <= 0.0 { 0.0 } else { ((x - self.start.0) * dx + (y - self.start.1) * dy) / length2 }
            }
            Shape::Circular => {
                let angle = self.ellipse[2].to_radians();
                let (px, py) = (x - self.start.0, y - self.start.1);
                let u = px * angle.cos() + py * angle.sin();
                let v = -px * angle.sin() + py * angle.cos();
                let (rx, ry) = (self.ellipse[0] / 2.0, self.ellipse[1] / 2.0);
                if rx <= 0.0 || ry <= 0.0 { 0.0 } else { ((u / rx).powi(2) + (v / ry).powi(2)).sqrt() }
            }
        };
        t.clamp(0.0, 1.0)
    }

    /// Straight RGBA (0..1) at position `t`.
    pub fn color(&self, t: f64) -> [f64; 4] {
        let first = &self.stops[0];
        if t <= first.position {
            return first.color;
        }
        for pair in self.stops.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            if t <= b.position {
                let span = b.position - a.position;
                let mut k = if span > 0.0 { (t - a.position) / span } else { 1.0 };
                if a.curve.len() >= 2 {
                    k = curve_at(&a.curve, k);
                }
                let out: [f64; 4] = std::array::from_fn(|c| a.color[c] + (b.color[c] - a.color[c]) * k);
                return out;
            }
        }
        self.stops.last().map(|s| s.color).unwrap_or([0.0; 4])
    }

    /// The gradient at `scale` into a `width × height` image (pixel centres).
    pub fn draw(&self, width: u32, height: u32, scale: f64) -> RgbaImage {
        let mut image = RgbaImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let t = self.position((x as f64 + 0.5) / scale, (y as f64 + 0.5) / scale);
                let c = self.color(t).map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8);
                image.set_pixel(x, y, c);
            }
        }
        image
    }
}

/// Piecewise-linear curve through control points (x ascending).
fn curve_at(points: &[(f64, f64)], x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    for pair in points.windows(2) {
        let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
        if x <= x1 {
            return if x1 > x0 { y0 + (y1 - y0) * (x - x0) / (x1 - x0) } else { y1 };
        }
    }
    points.last().map(|p| p.1).unwrap_or(x).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(text: &str) -> Vec<u8> {
        let mut out = (text.len() as u32).to_be_bytes().to_vec();
        for unit in text.encode_utf16() {
            out.extend(unit.to_be_bytes());
        }
        out
    }

    /// A two-stop linear gradient from opaque black (x = 0) to transparent
    /// white (x = 100), shaped like the files CSP writes.
    fn sample() -> Vec<u8> {
        let words = |values: &[u32]| values.iter().flat_map(|w| w.to_be_bytes()).collect::<Vec<u8>>();
        let mut data = words(&[0, 2]);
        data.extend(name("GradationData"));
        data.extend(words(&[72, 16, 28, 2, 16]));
        data.extend(words(&[0, 0, 0, 0xffff_ffff, 0, 0, 0]));
        data.extend(words(&[0xffff_ffff, 0xffff_ffff, 0xffff_ffff, 0, 0, 0x8000, 0]));
        data.extend(name("GradationSetting"));
        data.extend(words(&[0, 0, 1]));
        for value in [100.0f64, 100.0, 0.0, 0.0, 0.0, 100.0, 0.0] {
            data.extend(value.to_be_bytes());
        }
        data.extend(name("GradationSettingAdd0001"));
        data.extend(words(&[0x14, 0, 0, 0, 0, 1]));
        let total = data.len() as u32;
        data[..4].copy_from_slice(&total.to_be_bytes());
        data
    }

    #[test]
    fn parses_and_interpolates() {
        let gradient = Gradient::parse(&sample()).unwrap();
        assert_eq!(gradient.shape, Shape::Linear);
        assert_eq!(gradient.stops.len(), 2);
        assert_eq!(gradient.end, (100.0, 0.0));
        assert_eq!(gradient.position(50.0, 7.0), 0.5);
        assert_eq!(gradient.position(-5.0, 0.0), 0.0);
        let mid = gradient.color(0.5);
        assert!((mid[0] - 0.5).abs() < 1e-3 && (mid[3] - 0.5).abs() < 1e-3, "{mid:?}");
    }

    #[test]
    fn curves_follow_stops() {
        assert_eq!(curve_at(&[(0.0, 0.0), (0.5, 0.1), (1.0, 1.0)], 0.25), 0.05);
    }
}
