//! Print the parsed gradient of every gradient layer (numbers only).
//!
//! Usage: gradient_dump FILE.clip

use csp_format::gradient::Gradient;
use csp_format::{ClipFile, LayerKind, Result};

fn main() -> Result<()> {
    let path = std::env::args().nth(1).expect("usage: gradient_dump FILE.clip");
    let file = ClipFile::open(path)?;
    println!("canvas {}x{}", file.canvas.width_px, file.canvas.height_px);
    for (layer, _) in file.layers.flatten() {
        if layer.kind != LayerKind::Gradient {
            continue;
        }
        let data = layer.row.blob("GradationFillInfo").unwrap_or_default();
        println!(
            "layer {} visible {} opacity {} blend {} clip {} mask {} offset {:?} colour {:?}/{:?}",
            layer.main_id,
            layer.visible,
            layer.opacity,
            layer.blend,
            layer.clipping,
            layer.mask_mipmap,
            layer.offset,
            layer.color("DrawColorMain"),
            layer.color("DrawColorSub")
        );
        match Gradient::parse(data) {
            Ok(g) => {
                println!("  shape {:?} ellipse {:?} start {:?} end {:?}", g.shape, g.ellipse, g.start, g.end);
                for stop in &g.stops {
                    println!(
                        "  stop {:.4} flags {:#x} rgba {:?} curve {:?}",
                        stop.position,
                        stop.flags,
                        stop.color.map(|v| (v * 255.0).round()),
                        stop.curve
                    );
                }
            }
            Err(error) => println!("  parse error {error}"),
        }
        let words: Vec<String> = data.chunks(4).map(|w| w.iter().map(|b| format!("{b:02x}")).collect()).collect();
        println!("  raw {}", words.join(" "));
    }
    Ok(())
}
