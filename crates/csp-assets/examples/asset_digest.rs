//! W5: what the Rust crate reads from one file, as JSON, for comparison
//! with the Python reader (`tools/validation/assets_python_check.py`).
//!
//!     asset_digest FILE

use csp_assets::colorset::ColorSet;
use csp_assets::kind::{Family, family};
use csp_assets::material::{self, Catalog, InfoList};
use csp_assets::serial::Document;
use csp_assets::subtool::SubToolDb;
use serde_json::{Value, json};

fn digest(name: &str, data: &[u8]) -> csp_assets::Result<Value> {
    Ok(match family(data) {
        Family::Sqlite => {
            let db = SubToolDb::from_bytes(data)?;
            let tree: Vec<Value> = db
                .tree()
                .iter()
                .map(|e| {
                    let node = &db.nodes[e.node];
                    json!([e.depth, e.path, node.name, node.kind(), node.variant_id, node.init_variant_id])
                })
                .collect();
            let settings: serde_json::Map<String, Value> =
                db.variants.keys().map(|id| (id.to_string(), db.variant_json(*id).unwrap_or(Value::Null))).collect();
            json!({ "kind": db.kind, "tree": tree, "settings": settings })
        }
        Family::Serial => Document::parse(data)?.to_json(),
        Family::ColorSet => serde_json::to_value(ColorSet::parse(data)?).expect("JSON"),
        Family::Xml if name == "catalog.xml" => serde_json::to_value(Catalog::parse(data)?).expect("JSON"),
        Family::Xml if name == "info.xml" || name == "layerData.xml" => {
            serde_json::to_value(InfoList::parse(data)?).expect("JSON")
        }
        Family::C2f => serde_json::to_value(csp_assets::c2f::summarize(data)?).expect("JSON"),
        Family::CatalogCache => json!(material::cac_strings(data)),
        _ => Value::Null,
    })
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: asset_digest FILE");
    let data = std::fs::read(&path).expect("read");
    let name = std::path::Path::new(&path).file_name().unwrap().to_string_lossy().into_owned();
    let out = match digest(&name, &data) {
        Ok(value) => json!({ "ok": true, "digest": value }),
        Err(error) => json!({ "ok": false, "error": error.to_string() }),
    };
    println!("{}", serde_json::to_string(&out).expect("JSON"));
}
