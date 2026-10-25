//! Test files for the width of vector strokes.
//!
//! `vec_test TEMPLATE OUT ROUND`: from `vec_tmpl.clip` (a CSP-made file
//! with one vector layer holding a G-pen stroke), replace the strokes with
//! a grid of straight horizontal lines of known pressure and brush size,
//! and give the one brush style the size effector of ROUND. CSP draws
//! vector layers from their data when it opens a file, so its merged-image
//! export shows how each setting is drawn.
//!
//! Grid: 11 rows (pressure 0.05, 0.1, 0.2 … 1.0) × 4 columns. Columns 0–2
//! have brush sizes 5, 10 and 20 at that pressure; column 3 has size 10,
//! pressure 1 and the row's value as point value 1. Lines run 260 px with
//! a point every 4 px. Point value 10 is set to 0: CSP skips a segment
//! shorter than it (it looks like the distance to the first dab), and
//! value 2 is kept (CSP draws nothing where it is 0).
//!
//! Rounds (`SizeEffector` flags, then the words that follow):
//! `a` as saved (0x11, the G-pen curve), `b` 0x11 with a linear curve,
//! `c` 0x11 with minimum 0.5 and a linear curve, `d` 0x01 (no pressure),
//! `e` 0x51 (0.6) linear, `f` 0x91 (1.0) linear, `g` 0x101 (1.0), `h` 0x41
//! (0.6), `i` 0x11 with the five-point curve (0, 0) (0.2, 0.6) (0.5, 0.3)
//! (0.8, 0.9) (1, 1), `j` 0x11 with (0, 0) (0.3, 0) (0.6, 1) (1, 1);
//! `k`, `l`, `m` round `b` with `AntiAlias` 0, 1 and 3 (the template's is 2).

use csp_format::database::value_as_text;
use csp_format::vector::{VectorData, VectorObject};
use csp_format::{ClipFile, LayerKind, Result};
use rusqlite::types::Value;

pub const PRESSURES: [f32; 11] = [0.05, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];
const SIZES: [f64; 3] = [5.0, 10.0, 20.0];
const LENGTH: f64 = 260.0;
const STEP: f64 = 4.0;

/// Brush tip settings of the tip rounds.
#[derive(Debug)]
struct Tip {
    thickness: f64,
    rotation: f64,
    rotation_effector: i64,
    size_effector: Vec<u32>,
    thickness_effector: Vec<u32>,
}

/// Rounds `ta` … `tj`: thickness 0.4 with rotation 0° / 45° and
/// `RotationEffector` 0, 1, 3, 67, 195, 35, 19, 131; `ti` thickness 1
/// following the pressure (linear curve) and a size without pressure.
fn tip(round: &str) -> Option<Tip> {
    let fixed = |rotation: f64, rotation_effector: i64| Tip {
        thickness: 0.4,
        rotation,
        rotation_effector,
        size_effector: vec![0x11, 0, 1],
        thickness_effector: vec![1],
    };
    Some(match round {
        "ta" => fixed(0.0, 0),
        "tb" => fixed(45.0, 0),
        "tc" => fixed(45.0, 1),
        "td" => fixed(45.0, 3),
        "te" => fixed(45.0, 67),
        "tf" => fixed(45.0, 195),
        "tg" => fixed(45.0, 35),
        "th" => fixed(45.0, 19),
        "tj" => fixed(45.0, 131),
        "ti" => Tip {
            thickness: 1.0,
            rotation: 0.0,
            rotation_effector: 0,
            size_effector: vec![1],
            thickness_effector: vec![0x11, 0, 1],
        },
        _ => return None,
    })
}

/// Turn an object by `angle` (radians, y down) around `centre`; point and
/// object boxes are rebuilt with `reach`.
fn rotate(object: &mut VectorObject, centre: (f64, f64), angle: f64, reach: f64) {
    let (sin, cos) = angle.sin_cos();
    let coords: Vec<(f64, f64)> = object
        .coordinates()
        .iter()
        .map(|(x, y)| {
            let (dx, dy) = (x - centre.0, y - centre.1);
            (centre.0 + dx * cos - dy * sin, centre.1 + dx * sin + dy * cos)
        })
        .collect();
    let mut all = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for (i, raw) in object.points.iter_mut().enumerate() {
        let (x, y) = coords[i];
        let (nx, ny) = coords.get(i + 1).copied().unwrap_or((x, y));
        put(raw, 0, &x.to_be_bytes());
        put(raw, 8, &y.to_be_bytes());
        let bbox = [x.min(nx) - reach, y.min(ny) - reach, x.max(nx) + reach, y.max(ny) + reach];
        for (k, v) in bbox.iter().enumerate() {
            let v = if k < 2 { v.floor() } else { v.ceil() };
            put(raw, 16 + 4 * k, &(v as i32).to_be_bytes());
        }
        all = [all[0].min(bbox[0]), all[1].min(bbox[1]), all[2].max(bbox[2]), all[3].max(bbox[3])];
    }
    object.set_bbox([all[0].floor() as i32, all[1].floor() as i32, all[2].ceil() as i32, all[3].ceil() as i32]);
}

fn put(data: &mut [u8], offset: usize, bytes: &[u8]) {
    data[offset..offset + bytes.len()].copy_from_slice(bytes);
}

/// The `SizeEffector` blob and the curve's control points (x, y pairs)
/// when the curve is replaced (`ControlNumber` follows; rounds `i` and `j`
/// were exported before it did, so CSP read their first three points).
fn effector(round: &str) -> Option<(Vec<u8>, Option<Vec<f64>>)> {
    let linear = Some(vec![0.0, 0.0, 0.5, 0.5, 1.0, 1.0]);
    let (words, curve): (Vec<u32>, Option<Vec<f64>>) = match round {
        "a" | "z" => return None,
        _ if round.starts_with('t') => return None,
        "b" => (vec![0x11, 0, 1], linear),
        "c" => (vec![0x11, 0.5f32.to_bits(), 1], linear),
        "d" => (vec![0x01], None),
        "e" => (vec![0x51, 0, 1, 0.6f32.to_bits()], linear),
        "f" => (vec![0x91, 0, 1, 1f32.to_bits()], linear),
        "g" => (vec![0x101, 1f32.to_bits()], None),
        "h" => (vec![0x41, 0.6f32.to_bits()], None),
        "i" => (vec![0x11, 0, 1], Some(vec![0.0, 0.0, 0.2, 0.6, 0.5, 0.3, 0.8, 0.9, 1.0, 1.0])),
        "j" => (vec![0x11, 0, 1], Some(vec![0.0, 0.0, 0.3, 0.0, 0.6, 1.0, 1.0, 1.0])),
        "k" | "l" | "m" => (vec![0x11, 0, 1], linear),
        _ => panic!("unknown round {round}"),
    };
    Some((words.iter().flat_map(|w| w.to_be_bytes()).collect(), curve))
}

/// Pieces of the template's stroke that new lines are built from.
struct Parts<'a> {
    prototype: &'a VectorObject,
    first: &'a [u8],
    middle: &'a [u8],
    last: &'a [u8],
}

impl Parts<'_> {
    /// A horizontal line from (x0, y), a point every `step` px, the
    /// middle point's values unless `edit(index, count, raw)` changes them.
    fn line(
        &self,
        x0: f64,
        y: f64,
        length: f64,
        step: f64,
        size: f64,
        edit: impl Fn(usize, usize, &mut Vec<u8>),
    ) -> VectorObject {
        let count = (length / step).round() as usize + 1;
        let mut object = self.prototype.clone();
        object.set_brush_size(size);
        let reach = size + 2.0;
        object.points = (0..count)
            .map(|i| {
                let mut raw = match i {
                    0 => self.first.to_vec(),
                    _ if i == count - 1 => self.last.to_vec(),
                    _ => self.middle.to_vec(),
                };
                let x = x0 + i as f64 * step;
                put(&mut raw, 0, &x.to_be_bytes());
                put(&mut raw, 8, &y.to_be_bytes());
                let next = if i + 1 < count { x + step } else { x };
                let bbox = [(x - reach).floor(), (y - reach).floor(), (next + reach).ceil(), (y + reach).ceil()];
                for (k, v) in bbox.iter().enumerate() {
                    put(&mut raw, 16 + 4 * k, &(*v as i32).to_be_bytes());
                }
                edit(i, count, &mut raw);
                raw
            })
            .collect();
        put(&mut object.header, 16, &(count as u32).to_be_bytes());
        object.set_bbox([
            (x0 - reach).floor() as i32,
            (y - reach).floor() as i32,
            (x0 + length + reach).ceil() as i32,
            (y + reach).ceil() as i32,
        ]);
        object
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output, round] = args.as_slice() else {
        eprintln!("usage: vec_test TEMPLATE OUT ROUND");
        std::process::exit(2);
    };
    let mut file = ClipFile::open(input)?;
    let layer = file
        .layers
        .flatten()
        .into_iter()
        .find(|(layer, _)| layer.kind == LayerKind::Vector)
        .map(|(layer, _)| layer.main_id)
        .ok_or_else(|| csp_format::Error::format("no vector layer"))?;
    let id: String = file
        .database()
        .query_row("SELECT VectorData FROM VectorObjectList WHERE LayerId = ?1", [layer], |row| row.get::<_, Value>(0))
        .ok()
        .and_then(|value| value_as_text(&value))
        .ok_or_else(|| csp_format::Error::format("no vector data"))?;
    let mut data = VectorData::parse(file.external(&id).unwrap_or_default())?;
    let prototype = data.objects.first().cloned().ok_or_else(|| csp_format::Error::format("no stroke"))?;
    let (first, middle, last) =
        (prototype.points[0].clone(), prototype.points[1].clone(), prototype.points.last().cloned().expect("points"));
    let (cell_w, cell_h) = (file.canvas.width_px as f64 / 4.0, file.canvas.height_px as f64 / 12.0);
    let parts = Parts { prototype: &prototype, first: &first, middle: &middle, last: &last };
    let mut objects = Vec::new();
    if let Some(tip) = tip(round) {
        // Tip rounds: rows (size 20, pressure 1), (20, 0.5), (10, 1),
        // (10, 0.5); columns: lines of 100 px at 0°, 22.5° … 157.5° (y down).
        let (cw, ch) = (file.canvas.width_px as f64 / 8.0, file.canvas.height_px as f64 / 4.0);
        for (row, (size, pressure)) in [(20.0, 1.0f32), (20.0, 0.5), (10.0, 1.0), (10.0, 0.5)].into_iter().enumerate() {
            for column in 0..8 {
                let angle = (column as f64 * 22.5).to_radians();
                let (cx, cy) = ((column as f64 + 0.5) * cw, (row as f64 + 0.5) * ch);
                let mut object = parts.line(cx - 50.0, cy, 100.0, STEP, size, |_, _, raw| {
                    put(raw, 36, &pressure.to_be_bytes());
                    put(raw, 76, &0f32.to_be_bytes());
                });
                rotate(&mut object, (cx, cy), angle, size + 2.0);
                objects.push(object);
            }
        }
        println!("tip {tip:?}");
        file.database().execute(
            "UPDATE BrushStyle SET SizeEffector = ?1, ThicknessBase = ?2, RotationBase = ?3, RotationEffector = ?4, ThicknessEffector = ?5",
            (
                Value::Blob(tip.size_effector.iter().flat_map(|w| w.to_be_bytes()).collect()),
                tip.thickness,
                tip.rotation,
                tip.rotation_effector,
                Value::Blob(tip.thickness_effector.iter().flat_map(|w| w.to_be_bytes()).collect()),
            ),
        )?;
        let points: Vec<u8> = [0.0f64, 0.0, 0.5, 0.5, 1.0, 1.0].iter().flat_map(|v| v.to_be_bytes()).collect();
        file.database().execute(
            "UPDATE BrushEffectorGraphData SET ControlPoints = ?1, ControlNumber = 3",
            [Value::Blob(points)],
        )?;
    } else if round == "z" {
        // Rows of differently built lines (size 10, pressure 1), to see
        // which point fields CSP needs.
        let y = |row: usize| (row as f64 + 1.0) * cell_h;
        let mut copy = prototype.clone();
        let (px, py) = (copy.coordinates()[0].0, copy.coordinates()[0].1);
        copy.transform(1.0, 40.0 - px, y(0) - py);
        copy.set_brush_size(10.0);
        objects.push(copy);
        let source: Vec<_> = (0..prototype.points.len()).filter_map(|i| prototype.point(i)).collect();
        objects.push(parts.line(40.0, y(1), 1000.0, 4.0, 10.0, |i, n, raw| {
            // Values interpolated along the prototype by position.
            let t = i as f64 / (n - 1) as f64 * (source.len() - 1) as f64;
            let (k, f) = (t.floor() as usize, t - t.floor());
            let next = (k + 1).min(source.len() - 1);
            for v in 0..13 {
                let value = source[k].values[v] as f64 * (1.0 - f) + source[next].values[v] as f64 * f;
                put(raw, 36 + 4 * v, &(value as f32).to_be_bytes());
            }
        }));
        objects.push(parts.line(40.0, y(2), 1000.0, 20.0, 10.0, |_, _, _| {}));
        objects.push(parts.line(40.0, y(3), 1000.0, 4.0, 10.0, |_, _, raw| put(raw, 76, &0f32.to_be_bytes())));
        objects.push(parts.line(40.0, y(4), 1000.0, 4.0, 10.0, |_, _, raw| put(raw, 44, &0f32.to_be_bytes())));
        objects.push(parts.line(40.0, y(5), 1000.0, 40.0, 10.0, |_, _, _| {}));
        let mut zero_tail = parts.line(40.0, y(6), 1000.0, 4.0, 10.0, |_, _, _| {});
        let len = zero_tail.header.len();
        put(&mut zero_tail.header, len - 4, &0u32.to_be_bytes());
        objects.push(zero_tail);
        objects.push(parts.line(40.0, y(7), 1000.0, 100.0, 10.0, |_, _, _| {}));
        objects.push(parts.line(40.0, y(8), 1000.0, 4.0, 10.0, |i, n, raw| {
            // Velocity-like value 2: rising then falling.
            let t = i as f64 / (n - 1) as f64;
            put(raw, 44, &((0.5 * (std::f64::consts::PI * t).sin()) as f32).to_be_bytes());
        }));
        println!("rows: copy, interpolated, step 20, value10 0, value2 0, step 40, tail 0, step 100, value2 sine");
    } else {
        for (row, &pressure) in PRESSURES.iter().enumerate() {
            for column in 0..4 {
                let (size, pressure, value1) = match SIZES.get(column) {
                    Some(&size) => (size, pressure, None),
                    None => (10.0, 1.0, Some(pressure)),
                };
                let x0 = column as f64 * cell_w + (cell_w - LENGTH) / 2.0;
                let y = (row as f64 + 1.0) * cell_h;
                objects.push(parts.line(x0, y, LENGTH, STEP, size, |_, _, raw| {
                    put(raw, 36, &pressure.to_be_bytes());
                    put(raw, 76, &0f32.to_be_bytes());
                    if let Some(value1) = value1 {
                        put(raw, 40, &value1.to_be_bytes());
                    }
                }));
                println!("row {row} column {column}: size {size} pressure {pressure} value1 {value1:?}");
            }
        }
    }
    data.objects = objects;
    file.set_external(&id, data.to_bytes());
    if let Some((blob, curve)) = effector(round) {
        file.database().execute("UPDATE BrushStyle SET SizeEffector = ?1", [Value::Blob(blob)])?;
        if let Some(curve) = curve {
            let points: Vec<u8> = curve.iter().flat_map(|v| v.to_be_bytes()).collect();
            file.database().execute(
                "UPDATE BrushEffectorGraphData SET ControlPoints = ?1, ControlNumber = ?2",
                (Value::Blob(points), (curve.len() / 2) as i64),
            )?;
        }
    }
    // Rounds k, l and m: round b with anti-aliasing none, weak and strong.
    if let Some(level) = ["k", "l", "m"].iter().position(|r| r == round) {
        let level = [0, 1, 3][level];
        file.database().execute("UPDATE BrushStyle SET AntiAlias = ?1", [level])?;
    }
    file.update_preview()?;
    file.renew_file_id();
    file.save(output)
}
