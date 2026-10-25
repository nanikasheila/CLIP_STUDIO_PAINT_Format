//! Diagnostic: per layer, the offsets and the size of each stored mipmap
//! level of the pixels and the mask, with init colours. Structure only.
//!
//! Usage: layer_geometry FILE.clip
use csp_format::ClipFile;

fn main() -> csp_format::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let file = ClipFile::open(&args[0])?;
    for (layer, depth) in file.layers.flatten() {
        println!(
            "{}id {} {} type {} offset {:?} render {:?} mask_offset {:?} mask_render {:?} mask_enabled {}",
            "  ".repeat(depth),
            layer.main_id,
            layer.kind.name(),
            layer.layer_type,
            layer.offset,
            layer.render_offset,
            layer.mask_offset,
            layer.mask_render_offset,
            layer.mask_enabled
        );
        for (label, id) in [("pixels", layer.render_mipmap), ("mask", layer.mask_mipmap)] {
            let Some(mipmap) = file.mipmap(id)? else { continue };
            for level in &mipmap.levels {
                let Some(offscreen) = file.offscreen(level.offscreen)? else { continue };
                let p = offscreen.parameter;
                let init = offscreen.attribute.init_color();
                println!(
                    "{}  {label} {}% {}x{} channels {} stored {} init {:?}",
                    "  ".repeat(depth),
                    level.scale,
                    p.width,
                    p.height,
                    p.image_channels,
                    offscreen.data.is_some(),
                    init.map(|i| (i.do_initialize, i.alpha, i.image_values.clone()))
                );
            }
        }
    }
    Ok(())
}
