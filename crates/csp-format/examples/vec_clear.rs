//! Clear the stored pixels of a vector layer, to see whether (and how) CSP
//! draws the layer again from its vector data when it opens the file.
//!
//! `vec_clear FILE LAYER OUT [SQL...]`: each SQL statement is run on the
//! file's database first (to change a brush setting or curve). With
//! `VC_SEED` set, every point of the layer gets a random seed in point value
//! 11 (CSP's per-point seed for random brush effects; synthetic strokes have 0).

use csp_format::database::value_as_text;
use csp_format::vector::VectorData;
use csp_format::{ClipFile, Result, RgbaImage};
use rusqlite::types::Value;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [file, layer, output, statements @ ..] = args.as_slice() else {
        eprintln!("usage: vec_clear FILE LAYER OUT [SQL...]");
        std::process::exit(2);
    };
    let mut clip = ClipFile::open(file)?;
    for sql in statements {
        let changed = clip.database().execute(sql, [])?;
        println!("{changed} rows changed");
    }
    let layer: i64 = layer.parse().expect("layer id");
    if std::env::var_os("VC_SEED").is_some() {
        seed_points(&mut clip, layer)?;
    }
    let blank = RgbaImage::new(clip.canvas.width_px, clip.canvas.height_px);
    clip.set_layer_pixels(layer, &blank, (0, 0))?;
    clip.update_preview()?;
    clip.renew_file_id();
    clip.save(output)
}

/// Writes a random seed (xorshift) into point value 11 of every point.
fn seed_points(clip: &mut ClipFile, layer: i64) -> Result<()> {
    let id: String = clip
        .database()
        .query_row("SELECT VectorData FROM VectorObjectList WHERE LayerId = ?1", [layer], |row| row.get::<_, Value>(0))
        .ok()
        .and_then(|value| value_as_text(&value))
        .ok_or_else(|| csp_format::Error::format("no vector data"))?;
    let mut data = VectorData::parse(clip.external(&id).unwrap_or_default())?;
    let mut state = 0x9e37_79b9u32;
    let mut count = 0;
    for object in &mut data.objects {
        for raw in &mut object.points {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            raw[80..84].copy_from_slice(&state.to_be_bytes());
            count += 1;
        }
    }
    clip.set_external(&id, data.to_bytes());
    println!("{count} points seeded");
    Ok(())
}
