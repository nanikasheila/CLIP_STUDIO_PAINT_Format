//! Diagnostic: write every stored mipmap level of every layer (folders and
//! the root included) as PNG, to see which caches CSP keeps.
use csp_format::ClipFile;

fn main() -> csp_format::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let file = ClipFile::open(&args[0])?;
    for (layer, depth) in file.layers.flatten() {
        let Some(mipmap) = file.mipmap(layer.render_mipmap)? else { continue };
        for (index, level) in mipmap.levels.iter().enumerate() {
            let Some(offscreen) = file.offscreen(level.offscreen)? else { continue };
            let image = offscreen.to_rgba()?;
            let (mut sum, mut opaque) = ([0u64; 4], 0u64);
            for pixel in image.pixels.chunks(4) {
                for c in 0..4 {
                    sum[c] += pixel[c] as u64;
                }
                opaque += (pixel[3] > 0) as u64;
            }
            let n = (image.width * image.height).max(1) as u64;
            println!(
                "{:depth$}layer {} {:?} level {index} {}x{} mean {:?} covered {}",
                "",
                layer.main_id,
                layer.kind,
                image.width,
                image.height,
                sum.map(|s| s / n),
                opaque,
            );
            if args.len() > 1 {
                image
                    .to_png()
                    .map(|png| std::fs::write(format!("{}/l{}_{index}.png", args[1], layer.main_id), png))??;
            }
        }
    }
    Ok(())
}
