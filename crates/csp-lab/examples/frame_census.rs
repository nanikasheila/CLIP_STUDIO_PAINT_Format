//! Census of frame folders (コマ枠フォルダー) and masks over a corpus: whether
//! a visible folder's mask and frame-line pixels are stored, and how many
//! frame objects it has; and how many other masked layers have no stored
//! mask pixels. Aggregated counts as JSON (no names or paths).
//!
//! Usage: frame_census ROOT LIST.txt

use std::collections::BTreeMap;

use csp_format::stroke::layer_vectors;
use csp_format::{ClipFile, LayerKind, Result};

fn stored(file: &ClipFile, mipmap: i64) -> &'static str {
    if mipmap == 0 {
        return "none";
    }
    match file.mipmap(mipmap) {
        Ok(Some(m)) => {
            let any =
                m.levels.iter().any(|l| file.offscreen(l.offscreen).ok().flatten().is_some_and(|o| o.data.is_some()));
            if any { "stored" } else { "empty" }
        }
        _ => "missing",
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [root, list] = args.as_slice() else { panic!("usage: frame_census ROOT LIST.txt") };
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for line in std::fs::read_to_string(list)?.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let Ok(file) = ClipFile::open(format!("{root}/{}", line.trim_start_matches("./"))) else { continue };
        let mut file_keys = std::collections::BTreeSet::new();
        for (layer, _) in file.layers.flatten() {
            if !layer.visible {
                continue;
            }
            let mask = if layer.mask_enabled { stored(&file, layer.mask_mipmap) } else { "off" };
            if layer.kind == LayerKind::FrameFolder {
                let lines = stored(&file, layer.row.int("ComicFrameLineMipmap"));
                let objects = layer_vectors(&file, layer)?.iter().map(|d| d.objects.len()).sum::<usize>();
                let key = format!("frame mask={mask} lines={lines} objects={}", if objects > 0 { "yes" } else { "no" });
                *counts.entry(key.clone()).or_default() += 1;
                file_keys.insert(key);
            } else if layer.mask_enabled && mask != "stored" {
                let key = format!("{} mask={mask}", layer.kind.name());
                *counts.entry(key.clone()).or_default() += 1;
                file_keys.insert(key);
            }
        }
        for key in file_keys {
            *counts.entry(format!("files: {key}")).or_default() += 1;
        }
    }
    println!("{}", serde_json::to_string_pretty(&counts).expect("JSON"));
    Ok(())
}
