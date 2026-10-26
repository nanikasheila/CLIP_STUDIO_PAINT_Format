//! Census of vector lists over a corpus: for every `VectorObjectList` row,
//! which layer column points at it (stroke, fill, balloon, other), the
//! layer's kind, and the objects' header sizes, flags, colours and
//! closedness. Prints aggregated counts as JSON (no names or paths).
//!
//! Usage: vector_lists ROOT LIST.txt [MAX_FILES]

use std::collections::BTreeMap;

use csp_format::database::{select, table_exists};
use csp_format::{ClipFile, LayerKind, Result};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [root, list, rest @ ..] = args.as_slice() else { panic!("usage: vector_lists ROOT LIST.txt [MAX]") };
    let max: usize = rest.first().and_then(|v| v.parse().ok()).unwrap_or(usize::MAX);
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut add = |key: String, n: usize| *counts.entry(key).or_default() += n;
    for line in std::fs::read_to_string(list)?.lines().map(str::trim).filter(|l| !l.is_empty()).take(max) {
        let path = format!("{root}/{}", line.trim_start_matches("./"));
        let Ok(file) = ClipFile::open(&path) else { continue };
        if !table_exists(file.database(), "VectorObjectList")? {
            continue;
        }
        for (layer, _) in file.layers.flatten() {
            let rows = select(
                file.database(),
                "SELECT MainId, VectorData FROM VectorObjectList WHERE LayerId = ?1 ORDER BY MainId",
                [layer.main_id],
            )?;
            for row in rows {
                let id = row.int("MainId");
                let role = [
                    ("stroke", "VectorNormalStrokeIndex"),
                    ("fill", "VectorNormalFillIndex"),
                    ("balloon", "VectorNormalBalloonIndex"),
                ]
                .iter()
                .find(|(_, column)| layer.row.opt_int(column) == Some(id))
                .map_or("other", |(name, _)| name);
                let kind = layer.kind.name();
                let Some(payload) = row
                    .get("VectorData")
                    .and_then(csp_format::database::value_as_text)
                    .and_then(|id| file.external(&id).map(<[u8]>::to_vec))
                else {
                    add(format!("{kind}/{role}/no-data"), 1);
                    continue;
                };
                let Ok(data) = csp_format::vector::VectorData::parse(&payload) else {
                    add(format!("{kind}/{role}/bad"), 1);
                    continue;
                };
                add(format!("{kind}/{role}/lists"), 1);
                for object in &data.objects {
                    add(
                        format!(
                            "{kind}/{role}/h{} p{} f{:#x} sub{} main{} w72 {:#x} size0 {}",
                            object.header_size(),
                            object.point_size,
                            object.flags(),
                            if object.sub_color() == [255, 255, 255] { "W".to_owned() } else { "x".to_owned() },
                            match object.main_color() {
                                [0, 0, 0] => "K",
                                [255, 255, 255] => "W",
                                _ => "x",
                            },
                            csp_format::binary::be_u32_at(&object.header, 72),
                            object.brush_size() == Some(0.0),
                        ),
                        1,
                    );
                }
                if layer.kind == LayerKind::Text {
                    add(format!("text/{role}/objects"), data.objects.len());
                }
            }
        }
    }
    println!("{}", serde_json::to_string_pretty(&counts).expect("JSON"));
    Ok(())
}
