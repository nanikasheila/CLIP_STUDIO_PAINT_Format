//! Validation (V7): per layer, what a reader sees, as one JSON line per
//! file, for comparing the Python reader (`tools/python_reader_check.py`)
//! with this crate. Pixels are reduced to CRC-32s; no names, text or pixels
//! are printed.
//!
//! Usage: layer_digest [--text-only] FILE.clip...
//!
//! `--text-only` gives only the layer ids, kinds and text (fast).
use csp_format::database::{select, table_exists, value_as_text};
use csp_format::vector::VectorData;
use csp_format::{ClipFile, RgbaImage};
use serde_json::{Value, json};

fn crc(bytes: &[u8]) -> u32 {
    let mut crc = flate2::Crc::new();
    crc.update(bytes);
    crc.sum()
}

fn image(found: Option<(RgbaImage, (i64, i64))>) -> Value {
    match found {
        Some((image, (x, y))) => json!([image.width, image.height, x, y, crc(&image.pixels)]),
        None => Value::Null,
    }
}

fn digest(path: &str, text_only: bool) -> csp_format::Result<Value> {
    let file = ClipFile::open(path)?;
    let has_vectors = table_exists(file.database(), "VectorObjectList")?;
    let mut layers = Vec::new();
    for (layer, depth) in file.layers.flatten() {
        let text: Vec<u32> = if matches!(layer.kind.name(), "text" | "story_info") {
            csp_format::text::read_blocks(&layer.row)?.iter().map(|block| crc(block.text.as_bytes())).collect()
        } else {
            Vec::new()
        };
        if text_only {
            layers.push(json!({"id": layer.main_id, "kind": layer.kind.name(), "text": text}));
            continue;
        }
        let mask = file.layer_mask(layer)?.map(|(mask, at)| {
            // The Python reader gives masks as opaque gray RGBA.
            let rgba: Vec<u8> = mask.pixels.iter().flat_map(|v| [*v, *v, *v, 255]).collect();
            json!([mask.width, mask.height, at.0, at.1, crc(&rgba)])
        });
        let (mut objects, mut points) = (0, 0);
        if has_vectors {
            let sql = "SELECT VectorData FROM VectorObjectList WHERE LayerId = ?1 ORDER BY MainId";
            for row in select(file.database(), sql, [layer.main_id])? {
                let Some(id) = row.values().first().and_then(value_as_text) else { continue };
                let Some(payload) = file.external(&id) else { continue };
                for object in VectorData::parse(payload)?.objects {
                    objects += 1;
                    points += object.points.len();
                }
            }
        }
        layers.push(json!({
            "id": layer.main_id,
            "depth": depth,
            "kind": layer.kind.name(),
            "name": crc(layer.name.as_bytes()),
            "visible": layer.visible,
            "opacity": layer.opacity_fraction(),
            "pixels": image(file.layer_pixels(layer)?),
            "mask": mask,
            "thumbnail": file.layer_thumbnail(layer)?.map(|t| crc(&t.pixels)),
            "vector": [objects, points],
            "text": text,
        }));
    }
    let preview = file.preview()?.map(|p| crc(&p.pixels));
    Ok(json!({
        "canvas": [file.canvas.width_px, file.canvas.height_px],
        "preview": preview,
        "layers": layers,
    }))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let text_only = args.iter().any(|arg| arg == "--text-only");
    for path in args.iter().filter(|arg| *arg != "--text-only") {
        let value = digest(path, text_only).unwrap_or_else(|error| json!({"error": error.to_string()}));
        println!("{}", serde_json::to_string(&value).expect("JSON"));
    }
}
