//! Copies of a CSP-saved file that pretend to come from another CSP version,
//! to see how CSP reacts (docs/validation/V9-versions.md).
//!
//! - `base`: renamed top layer only (control)
//! - `unk_set`: an unknown Layer column, registered like CSP's `Has1_0_N`
//!   markers (LockType 4, LockSpecified 2), set to 1 on the top layer
//! - `unk_alt`: as `unk_set` with AlternativeLockType 1 / Specified 0
//! - `unk_null`: the unknown column registered and present but NULL
//! - `unk_unreg`: the unknown column set to 1 but not registered
//! - `unk_plain`: an unknown column registered with lock type 0, set to 1
//! - `unk_row`: an unknown column registered with lock type 1, set to 1
//! - `older`: the ParamScheme rows that only CSP 5.x writes removed, as if
//!   an older version saved the file
//!
//! Usage: version_probe IN.clip OUT_PREFIX
use csp_format::database::SqlValue as Value;
use csp_format::{ClipFile, LayerKind, Result};

const UNKNOWN: &str = "Has1_0_99";

/// Labels first registered by the 5.0 and 5.1 generations.
const NEWEST: &[(&str, &str)] = &[
    ("BrushStyle", "BlurKind"),
    ("BrushStyle", "MipmapIndexToPlot"),
    ("CanvasItem", "ItemVersion"),
    ("Layer", "Has1_0_16"),
    ("Layer", "Has1_0_17"),
    ("Layer", "Has1_0_18"),
    ("Layer", "Has1_0_19"),
    ("Layer", "LayerEffectHas1_0_19"),
    ("Layer", "LayerEffectEdgeNewRender"),
    ("Project", "DefaultCoverSimulateColorProfile"),
    ("Project", "DefaultCoverSrcColorProfile"),
    ("Project", "DefaultSimulatePageColorProfile"),
    ("Project", "DefaultSrcPageColorProfile"),
    ("BrushStyle", "WaterEdgeStrength"),
    ("BrushStyle", "WaterEdgeStyle"),
    ("BrushStyle", "WaterEdgeVividness"),
];

fn top_raster(file: &ClipFile) -> i64 {
    file.layers
        .flatten()
        .into_iter()
        .rev()
        .find(|(layer, _)| layer.kind == LayerKind::Raster)
        .map(|(layer, _)| layer.main_id)
        .expect("a raster layer")
}

fn register(file: &ClipFile, lock: (i64, i64), alternative: Option<(i64, i64)>) -> Result<()> {
    let (alt_type, alt_specified) = match alternative {
        Some((t, s)) => (Value::Integer(t), Value::Integer(s)),
        None => (Value::Null, Value::Null),
    };
    file.database().execute(
        "INSERT INTO ParamScheme(TableName, LabelName, DataType, Flag, OwnerType, LockType, LockSpecified, \
         AlternativeLockType, AlternativeLockSpecified, LinkTable) VALUES ('Layer', ?1, 1, 0, 0, ?2, ?3, ?4, ?5, '')",
        (UNKNOWN, lock.0, lock.1, alt_type, alt_specified),
    )?;
    Ok(())
}

fn add_column(file: &ClipFile, layer: i64, value: Option<i64>) -> Result<()> {
    let db = file.database();
    db.execute(&format!("ALTER TABLE Layer ADD COLUMN {UNKNOWN} INTEGER DEFAULT NULL"), [])?;
    if let Some(value) = value {
        db.execute(&format!("UPDATE Layer SET {UNKNOWN} = ?1 WHERE MainId = ?2"), (value, layer))?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, prefix] = args.as_slice() else {
        eprintln!("usage: version_probe IN.clip OUT_PREFIX");
        std::process::exit(2);
    };
    for variant in ["base", "unk_set", "unk_alt", "unk_null", "unk_unreg", "unk_plain", "unk_row", "older"] {
        let mut file = ClipFile::open(input)?;
        let layer = top_raster(&file);
        file.set_layer_name(layer, variant)?;
        match variant {
            "unk_set" => {
                register(&file, (4, 2), None)?;
                add_column(&file, layer, Some(1))?;
            }
            "unk_alt" => {
                register(&file, (4, 2), Some((1, 0)))?;
                add_column(&file, layer, Some(1))?;
            }
            "unk_null" => {
                register(&file, (4, 2), None)?;
                add_column(&file, layer, None)?;
            }
            "unk_unreg" => add_column(&file, layer, Some(1))?,
            "unk_plain" => {
                register(&file, (0, 0), None)?;
                add_column(&file, layer, Some(1))?;
            }
            "unk_row" => {
                register(&file, (1, 0), None)?;
                add_column(&file, layer, Some(1))?;
            }
            "older" => {
                for (table, label) in NEWEST {
                    file.database()
                        .execute("DELETE FROM ParamScheme WHERE TableName = ?1 AND LabelName = ?2", (table, label))?;
                }
            }
            _ => {}
        }
        file.renew_file_id();
        file.save(format!("{prefix}{variant}.clip"))?;
    }
    Ok(())
}
