//! Diagnostic: for each layer with a mask, the mask's stored blocks and the
//! distribution of its values (structure only).
//!
//! Usage: mask_stats FILE.clip
use csp_format::ClipFile;

fn main() -> csp_format::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let file = ClipFile::open(&args[0])?;
    for (layer, _) in file.layers.flatten() {
        let Some(mipmap) = file.mipmap(layer.mask_mipmap)? else { continue };
        let Some(level) = mipmap.levels.first() else { continue };
        let Some(offscreen) = file.offscreen(level.offscreen)? else { continue };
        let blocks = offscreen.data.as_ref().map(|d| d.blocks.len()).unwrap_or(0);
        let mut stored = 0;
        offscreen.for_each_block(|_, _| {
            stored += 1;
            Ok(())
        })?;
        let alpha = offscreen.to_alpha()?;
        let mut histogram = [0usize; 4];
        for &v in &alpha.pixels {
            histogram[match v {
                0 => 0,
                255 => 3,
                1..=127 => 1,
                _ => 2,
            }] += 1;
        }
        println!(
            "layer {} {} mask_enabled {} {}x{} order {:#x} blocks {blocks} stored {stored} init {:?} values 0:{} low:{} high:{} 255:{}",
            layer.main_id,
            layer.kind.name(),
            layer.mask_enabled,
            offscreen.parameter.width,
            offscreen.parameter.height,
            offscreen.parameter.channel_order,
            offscreen.attribute.init_color().map(|i| (i.do_initialize, i.alpha)),
            histogram[0],
            histogram[1],
            histogram[2],
            histogram[3]
        );
    }
    Ok(())
}
