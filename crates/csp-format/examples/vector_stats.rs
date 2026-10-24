//! Summarise the vector objects of .clip files: header layouts, flags,
//! brush sizes and the range of every per-point value, plus a few sample
//! objects (numbers only). Used to work out how strokes are drawn.
//!
//! Usage: vector_stats SAMPLES FILE.clip...

use std::collections::BTreeMap;

use csp_format::database::value_as_text;
use csp_format::vector::VectorData;
use csp_format::{ClipFile, Result};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let samples: usize = args.first().and_then(|v| v.parse().ok()).unwrap_or(3);
    let mut headers: BTreeMap<(usize, usize, u32), usize> = BTreeMap::new();
    let mut ranges = [(f32::MAX, f32::MIN, 0f64, 0usize); 13];
    let mut flags: BTreeMap<u32, usize> = BTreeMap::new();
    let mut sizes: BTreeMap<i64, usize> = BTreeMap::new();
    let mut printed = 0;
    for path in &args[1..] {
        let Ok(file) = ClipFile::open(path) else { continue };
        let Ok(mut statement) = file.database().prepare("SELECT LayerId, VectorData FROM VectorObjectList") else {
            continue;
        };
        let rows: Vec<(i64, Option<String>)> = statement
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, value_as_text(&row.get(1)?))))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default();
        for (layer, id) in rows {
            let Some(payload) = id.as_deref().and_then(|id| file.external(id)) else { continue };
            let Ok(data) = VectorData::parse(payload) else { continue };
            for object in &data.objects {
                *headers.entry((object.header_size(), object.point_size, object.flags())).or_default() += 1;
                *sizes.entry(object.brush_size().unwrap_or(-1.0).round() as i64).or_default() += 1;
                for index in 0..object.points.len() {
                    let point = object.point(index).expect("point");
                    *flags.entry(point.flags).or_default() += 1;
                    for (range, value) in ranges.iter_mut().zip(point.values) {
                        if value.is_finite() {
                            range.0 = range.0.min(value);
                            range.1 = range.1.max(value);
                            range.2 += value as f64;
                            range.3 += 1;
                        }
                    }
                }
                if printed < samples && object.points.len() >= 4 {
                    printed += 1;
                    println!(
                        "object layer {layer} header {} flags {:#x} bbox {:?} colour {:?}/{:?} opacity {} size {:?} styles {:?} header-hex {}",
                        object.header_size(),
                        object.flags(),
                        object.bbox(),
                        object.main_color(),
                        object.sub_color(),
                        object.opacity(),
                        object.brush_size(),
                        object.brush_styles(),
                        object.header[64..].iter().map(|b| format!("{b:02x}")).collect::<String>()
                    );
                    for index in 0..object.points.len().min(6) {
                        let p = object.point(index).expect("point");
                        println!(
                            "  pt {:.2},{:.2} bbox {:?} flags {:#x} values {:?}",
                            p.x,
                            p.y,
                            p.bbox,
                            p.flags,
                            p.values.map(|v| (v * 1000.0).round() / 1000.0)
                        );
                    }
                }
            }
        }
    }
    println!("header/point size/flags: {headers:?}");
    println!("point flags: {flags:?}");
    println!("brush sizes: {sizes:?}");
    for (i, (lo, hi, sum, n)) in ranges.iter().enumerate() {
        println!("value {i} (+{}): {lo} .. {hi}, mean {:.3}", 36 + 4 * i, sum / (*n).max(1) as f64);
    }
    Ok(())
}
