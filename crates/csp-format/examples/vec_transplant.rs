//! Move real strokes into a test file, to see which point values change
//! how CSP draws them.
//!
//! `vec_transplant TEMPLATE SOURCE LAYER OUT [EDITS]`: from the vector
//! layer LAYER of SOURCE (a file whose vector layer also has its pixels
//! stored), take the objects of the most used brush style inside the
//! template-canvas-sized window with the most ink, and put them into the
//! vector layer of TEMPLATE (`vec_tmpl.clip`), with that brush style's
//! settings and size curve. EDITS sets point values on every point, as
//! `INDEX=VALUE` separated by commas (`1=1,10=0`). Also writes
//! `OUT.stored.png`, the stored pixels of the same window, and
//! `OUT.ours.png`, this library's drawing of the moved objects, for
//! comparing with CSP's export. Test files stay with the corpus (they hold
//! real strokes).

use std::collections::BTreeMap;

use csp_format::database::value_as_text;
use csp_format::stroke::{Placement, brushes, draw_objects, layer_vectors};
use csp_format::vector::VectorData;
use csp_format::{ClipFile, LayerKind, Result, RgbaImage};
use rusqlite::types::Value;

fn word(data: &[u8], i: usize) -> Option<u32> {
    data.get(i * 4..i * 4 + 4).map(|b| u32::from_be_bytes(b.try_into().expect("4")))
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (template, source, layer_id, output) = match args.as_slice() {
        [t, s, l, o, ..] => (t, s, l.parse::<i64>().expect("layer id"), o),
        _ => {
            eprintln!("usage: vec_transplant TEMPLATE SOURCE LAYER OUT [EDITS]");
            std::process::exit(2);
        }
    };
    let edits: Vec<(usize, f32)> = args
        .get(4)
        .map(|e| {
            e.split(',')
                .filter_map(|kv| kv.split_once('='))
                .map(|(k, v)| (k.parse().expect("index"), v.parse().expect("value")))
                .collect()
        })
        .unwrap_or_default();
    let src = ClipFile::open(source)?;
    let layer = src.layers.get(layer_id).ok_or_else(|| csp_format::Error::format("no such layer"))?.clone();
    let stored = src
        .mipmap_base(layer.render_mipmap)?
        .filter(|level| level.data.is_some())
        .ok_or_else(|| csp_format::Error::format("no stored pixels"))?
        .to_rgba()?;
    let mut tmpl = ClipFile::open(template)?;
    let (w, h) = (tmpl.canvas.width_px, tmpl.canvas.height_px);
    // The window with the most ink (coarse search).
    let mut best = (0.0, 0u32, 0u32);
    for wy in (0..stored.height.saturating_sub(h).max(1)).step_by(64) {
        for wx in (0..stored.width.saturating_sub(w).max(1)).step_by(64) {
            let mut sum = 0.0;
            for y in (wy..(wy + h).min(stored.height)).step_by(8) {
                for x in (wx..(wx + w).min(stored.width)).step_by(8) {
                    sum += stored.pixel(x, y)[3] as f64;
                }
            }
            if sum > best.0 {
                best = (sum, wx, wy);
            }
        }
    }
    let (wx, wy) = (best.1 as f64, best.2 as f64);
    let origin = layer.pixel_origin();
    let shift = ((layer.offset.0 - origin.0) as f64, (layer.offset.1 - origin.1) as f64);
    let (dx, dy) = (shift.0 - wx, shift.1 - wy);
    let mut objects = Vec::new();
    for data in layer_vectors(&src, &layer)? {
        for object in data.objects {
            let b = object.bbox();
            let inside = b[0] as f64 + dx >= 0.0
                && b[1] as f64 + dy >= 0.0
                && b[2] as f64 + dx <= w as f64
                && b[3] as f64 + dy <= h as f64;
            if inside {
                objects.push(object);
            }
        }
    }
    let src_brushes = brushes(&src)?;
    let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
    for object in &objects {
        if let Some(id) = object.brush_styles().into_iter().find(|id| src_brushes.contains_key(&(*id as i64))) {
            *counts.entry(id).or_default() += 1;
        }
    }
    let style = counts.iter().max_by_key(|(_, n)| **n).map(|(id, _)| *id).unwrap_or(0);
    objects.retain(|o| o.brush_styles().contains(&style));
    // Stored pixels of the window, and our drawing of the kept objects.
    let mut crop = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let (sx, sy) = (x + best.1, y + best.2);
            if sx < stored.width && sy < stored.height {
                crop.set_pixel(x, y, stored.pixel(sx, sy));
            }
        }
    }
    std::fs::write(format!("{output}.stored.png"), crop.to_png()?)?;
    let mut ours = RgbaImage::new(w, h);
    draw_objects(&mut ours, &objects, &src_brushes, Placement { offset: (dx, dy), scale: 1.0 });
    std::fs::write(format!("{output}.ours.png"), ours.to_png()?)?;
    // Move the objects, point them at the template's brush style 1 and
    // apply the edits.
    for object in &mut objects {
        object.transform(1.0, dx, dy);
        let end = object.header.len() - 12;
        for offset in (72..end).step_by(4) {
            if word(&object.header, offset / 4) == Some(style) {
                object.header[offset..offset + 4].copy_from_slice(&1u32.to_be_bytes());
            }
        }
        for raw in &mut object.points {
            for (index, value) in &edits {
                raw[36 + 4 * index..40 + 4 * index].copy_from_slice(&value.to_be_bytes());
            }
        }
    }
    let layer_t = tmpl
        .layers
        .flatten()
        .into_iter()
        .find(|(l, _)| l.kind == LayerKind::Vector)
        .map(|(l, _)| l.main_id)
        .ok_or_else(|| csp_format::Error::format("no vector layer in the template"))?;
    let id: String = tmpl
        .database()
        .query_row("SELECT VectorData FROM VectorObjectList WHERE LayerId = ?1", [layer_t], |row| {
            row.get::<_, Value>(0)
        })
        .ok()
        .and_then(|value| value_as_text(&value))
        .ok_or_else(|| csp_format::Error::format("no vector data"))?;
    let mut data = VectorData::parse(tmpl.external(&id).unwrap_or_default())?;
    println!("window {wx},{wy}; objects {} of brush {style}; edits {edits:?}", objects.len());
    data.objects = objects;
    tmpl.set_external(&id, data.to_bytes());
    // The brush style's tip and effector settings, with its size curve as
    // curve 1. Only plain columns are copied: settings that point at other
    // tables (textures, patterns) made CSP close when it opened the file.
    const COLUMNS: [&str; 11] = [
        "SizeEffector",
        "OpacityEffector",
        "ThicknessBase",
        "ThicknessEffector",
        "RotationBase",
        "RotationEffector",
        "RotationRandom",
        "AntiAlias",
        "Hardness",
        "IntervalBase",
        "AutoIntervalType",
    ];
    let mut size_curve = None;
    let mut faithful = true;
    for name in COLUMNS {
        let sql = format!("SELECT \"{name}\" FROM BrushStyle WHERE MainId = ?1");
        let Ok(mut value) = src.database().query_row(&sql, [style], |row| row.get::<_, Value>(0)) else {
            eprintln!("column {name} not in the source");
            continue;
        };
        if let Value::Blob(blob) = &mut value
            && word(blob, 0).is_some_and(|flags| flags & 0x10 != 0)
        {
            let curve = word(blob, 2).unwrap_or(0);
            if name == "SizeEffector" {
                size_curve = Some(curve);
            }
            if size_curve.is_some_and(|c| c == curve) {
                blob[8..12].copy_from_slice(&1u32.to_be_bytes());
            } else {
                // Only one curve is moved: this setting loses the pressure.
                faithful = false;
                *blob = 1u32.to_be_bytes().to_vec();
            }
        }
        let sql = format!("UPDATE BrushStyle SET \"{name}\" = ?1");
        if tmpl.database().execute(&sql, [value]).is_err() {
            eprintln!("column {name} not in the template");
        }
    }
    println!("settings copied faithfully: {faithful}");
    // `VT_SIZE_EFFECTOR=hex` replaces the size effector (to see what its
    // flags do on real strokes).
    if let Ok(hex) = std::env::var("VT_SIZE_EFFECTOR") {
        let blob: Vec<u8> =
            (0..hex.len() / 2).filter_map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()).collect();
        tmpl.database().execute("UPDATE BrushStyle SET SizeEffector = ?1", [Value::Blob(blob)])?;
        println!("size effector {hex}");
    }
    if let Some(curve) = size_curve {
        let (number, points): (i64, Vec<u8>) = src.database().query_row(
            "SELECT ControlNumber, ControlPoints FROM BrushEffectorGraphData WHERE MainId = ?1",
            [curve],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        tmpl.database().execute(
            "UPDATE BrushEffectorGraphData SET ControlNumber = ?1, ControlPoints = ?2",
            (number, Value::Blob(points)),
        )?;
    }
    tmpl.update_preview()?;
    tmpl.renew_file_id();
    tmpl.save(output)
}
