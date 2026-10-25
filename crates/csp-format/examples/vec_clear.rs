//! Clear the stored pixels of a vector layer, to see whether (and how) CSP
//! draws the layer again from its vector data when it opens the file.
//!
//! `vec_clear FILE LAYER OUT [SQL...]`: each SQL statement is run on the
//! file's database first (to change a brush setting or curve).

use csp_format::{ClipFile, Result, RgbaImage};

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
    let blank = RgbaImage::new(clip.canvas.width_px, clip.canvas.height_px);
    clip.set_layer_pixels(layer, &blank, (0, 0))?;
    clip.update_preview()?;
    clip.renew_file_id();
    clip.save(output)
}
