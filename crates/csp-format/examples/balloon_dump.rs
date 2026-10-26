//! Numbers of the text layers that have balloons: per layer its offset,
//! each text block's records 42 (bbox), 63 (size), 64 (quad), the size of
//! its rendered image, and each balloon object's header words, bbox and
//! first points. No text, names or pixels are printed.
//!
//! Usage: balloon_dump FILE.clip [MAX_LAYERS]  (FRAMES=1: frame folders instead)

use csp_format::binary::be_u32_at;
use csp_format::stroke::layer_vectors;
use csp_format::text::read_blocks;
use csp_format::{ClipFile, LayerKind, Result};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let file = ClipFile::open(&args[0])?;
    let max: usize = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(3);
    let mut shown = 0;
    for (layer, _) in file.layers.flatten() {
        let want = if std::env::var("FRAMES").is_ok() { LayerKind::FrameFolder } else { LayerKind::Text };
        if layer.kind != want {
            continue;
        }
        let vectors = layer_vectors(&file, layer)?;
        if vectors.is_empty() {
            continue;
        }
        shown += 1;
        if shown > max {
            break;
        }
        println!(
            "layer {} offset {:?} visible {} render_mipmap {} mask {} {:?} {:?} canvas {}x{}",
            layer.main_id,
            layer.offset,
            layer.visible,
            layer.render_mipmap,
            layer.mask_mipmap,
            layer.mask_offset,
            layer.mask_render_offset,
            file.canvas.width_px,
            file.canvas.height_px
        );
        for (i, block) in read_blocks(&layer.row)?.iter().enumerate() {
            let a = &block.attributes;
            let image = match a.render_offscreen() {
                Some(id) if id > 0 => file.offscreen(id as i64)?.map(|o| o.to_rgba().map(|i| (i.width, i.height))),
                _ => None,
            };
            let ids: Vec<u32> = a.records.iter().map(|r| r.id).collect();
            println!(
                "  block {i} chars {} bbox {:?} size {:?} quad {:?} image {:?} ids {:?}",
                block.text.chars().count(),
                a.bbox(),
                a.block_size(),
                a.quad(),
                image.map(|r| r.ok()),
                ids
            );
            if std::env::var("RECORDS").is_ok() {
                for r in &a.records {
                    let hex: String = r.data.iter().take(48).map(|b| format!("{b:02x}")).collect();
                    println!("    rec {} len {} {}", r.id, r.data.len(), hex);
                }
            }
        }
        for data in &vectors {
            for (j, object) in data.objects.iter().enumerate() {
                let words: Vec<String> = (20..object.header.len())
                    .step_by(4)
                    .map(|o| format!("{:08x}", be_u32_at(&object.header, o)))
                    .collect();
                let points: Vec<String> =
                    object.coordinates().iter().take(12).map(|(x, y)| format!("({x:.1},{y:.1})")).collect();
                println!(
                    "  balloon {j} h{} f{:#x} bbox {:?} main {:?} sub {:?} op {} n {} size {:?} words {} pts {}",
                    object.header_size(),
                    object.flags(),
                    object.bbox(),
                    object.main_color(),
                    object.sub_color(),
                    object.opacity(),
                    object.points.len(),
                    object.brush_size(),
                    words.join(" "),
                    points.join(" ")
                );
                if let Some(p) = object.point(0) {
                    println!("    p0 flags {:#x} values {:?}", p.flags, p.values);
                }
            }
        }
    }
    Ok(())
}
