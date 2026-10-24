//! Copy a file with every raster layer except the bottom one at the given
//! opacity (blend-mode tests at partial opacity).
//!
//! Usage: set_opacity IN.clip OUT.clip OPACITY(0..1)
use csp_format::{ClipFile, LayerKind, Result};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output, opacity] = args.as_slice() else {
        eprintln!("usage: set_opacity IN.clip OUT.clip OPACITY");
        std::process::exit(2);
    };
    let opacity: f64 = opacity.parse().map_err(|_| csp_format::Error::format("bad opacity"))?;
    let mut file = ClipFile::open(input)?;
    let rasters: Vec<i64> = file
        .layers
        .flatten()
        .into_iter()
        .filter(|(layer, _)| layer.kind == LayerKind::Raster)
        .map(|(layer, _)| layer.main_id)
        .collect();
    for id in rasters.iter().skip(1) {
        file.set_layer_opacity(*id, opacity)?;
    }
    file.update_preview()?;
    file.renew_file_id();
    file.save(output)
}
