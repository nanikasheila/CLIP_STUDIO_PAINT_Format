//! Draw the vector layers of a file from their data with this library (the
//! stored pixels are not used), for comparing with CSP's export.
//!
//! `vec_draw FILE OUT.png`

use csp_format::stroke::{Placement, brushes, draw_objects, layer_vectors};
use csp_format::{ClipFile, LayerKind, Result, RgbaImage};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [file, output] = args.as_slice() else {
        eprintln!("usage: vec_draw FILE OUT.png");
        std::process::exit(2);
    };
    let clip = ClipFile::open(file)?;
    let (w, h) = (clip.canvas.width_px, clip.canvas.height_px);
    let brushes = brushes(&clip)?;
    let mut out = RgbaImage::new(w, h);
    for (layer, _) in clip.layers.flatten() {
        if layer.kind != LayerKind::Vector || !layer.visible {
            continue;
        }
        let at = Placement { offset: (layer.offset.0 as f64, layer.offset.1 as f64), scale: 1.0 };
        for data in layer_vectors(&clip, layer)? {
            draw_objects(&mut out, &data.objects, &brushes, at);
        }
    }
    std::fs::write(output, out.to_png()?)?;
    Ok(())
}
