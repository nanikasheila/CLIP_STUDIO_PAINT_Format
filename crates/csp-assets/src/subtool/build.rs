//! Writing a `.sut` from settings: one sub tool, made in another program or
//! changed there, for CSP to read with サブツールの読み込み.
//!
//! The file has the tables CSP writes: `Manager` (its root is the sub
//! tool), `Node` (the sub tool, linked to nothing), `Variant` (its settings
//! and initial settings) and `MaterialFile` (the materials it uses). Only
//! the columns given are written; CSP fills the others with its defaults
//! when it reads the file (W3), and declares each column with the type it
//! uses itself, so a value is stored as CSP stores it whatever number type it
//! came as.

use rusqlite::Connection;
use rusqlite::types::Value;

use csp_format::database::{quote, serialize};

use super::MaterialFile;
use super::columns::VARIANT_COLUMN_TYPES;
use crate::error::{Error, Result};

/// One row as (column, value) pairs, in the order to write them.
pub type Cells = Vec<(String, Value)>;

/// What [`build_sut`] writes.
#[derive(Debug, Clone, Default)]
pub struct SutParts {
    /// `Manager` values (`ToolType`, `Version`, ...). `RootUuid`,
    /// `CurrentNodeUuid` and `MaxVariantID` are set from the node and the
    /// settings.
    pub manager: Cells,
    /// `Node` values: `NodeUuid`, `NodeName`, `NodeInputOp`, `NodeOutputOp`,
    /// `NodeVariantID`, `NodeInitVariantID` and the like. Links to other
    /// nodes are written as "none".
    pub node: Cells,
    /// `Variant` rows, each with its `VariantID`: the settings and the
    /// initial settings the node points at.
    pub variants: Vec<Cells>,
    /// Materials to pack (brush tips, paper textures), by the path the
    /// settings' material lists name.
    pub materials: Vec<MaterialFile>,
}

/// `Manager` columns of CSP's `.sut` files and their types.
const MANAGER_COLUMNS: &[(&str, &str)] = &[
    ("ToolType", "INTEGER"),
    ("Version", "INTEGER"),
    ("RootUuid", "BLOB"),
    ("CurrentNodeUuid", "BLOB"),
    ("MaxVariantID", "INTEGER"),
    ("CommonVariantID", "INTEGER"),
    ("ObjectNodeUuid", "BLOB"),
    ("SavedCount", "INTEGER"),
    ("PressureGraph", ""),
    ("PressureGraphInitialized", "INTEGER"),
    ("UsePressureFile", "INTEGER"),
];

/// `Node` columns of CSP's `.sut` files and their types.
const NODE_COLUMNS: &[(&str, &str)] = &[
    ("NodeUuid", "BLOB"),
    ("NodeName", "TEXT"),
    ("NodeShortCutKey", "INTEGER"),
    ("NodeIcon", "INTEGER"),
    ("NodeIconColor", "INTEGER"),
    ("NodeHidden", "INTEGER"),
    ("NodeInstalledState", "INTEGER"),
    ("NodeInstalledVersion", "INTEGER"),
    ("NodeLock", "INTEGER"),
    ("NodeInputOp", "INTEGER"),
    ("NodeOutputOp", "INTEGER"),
    ("NodeRangeOp", "INTEGER"),
    ("NodeNextUuid", "BLOB"),
    ("NodeFirstChildUuid", "BLOB"),
    ("NodeSelectedUuid", "BLOB"),
    ("NodeVariantID", "INTEGER"),
    ("NodeInitVariantID", "INTEGER"),
    ("NodeCustomIcon", ""),
    ("NodeHideByGrade", "INTEGER"),
    ("NodeDefaultIdentifier", "INTEGER"),
];

/// The SQLite type CSP declares a `Variant` column with (empty when the
/// column is not known).
pub fn variant_column_type(column: &str) -> &'static str {
    VARIANT_COLUMN_TYPES.binary_search_by(|(name, _)| (*name).cmp(column)).map_or("", |i| VARIANT_COLUMN_TYPES[i].1)
}

fn known_type(table: &[(&str, &'static str)], column: &str) -> &'static str {
    table.iter().find(|(name, _)| *name == column).map_or("", |(_, kind)| kind)
}

fn get<'a>(cells: &'a Cells, column: &str) -> Option<&'a Value> {
    cells.iter().find(|(name, _)| name == column).map(|(_, value)| value)
}

/// Column names in order: `first`, then the others as they come.
fn column_order<'a>(first: impl IntoIterator<Item = &'a str>, rows: &'a [&'a Cells]) -> Vec<String> {
    let mut names: Vec<String> = first.into_iter().map(String::from).collect();
    for row in rows {
        for (name, _) in row.iter() {
            if name != "_PW_ID" && !names.contains(name) {
                names.push(name.clone());
            }
        }
    }
    names
}

fn create(connection: &Connection, table: &str, names: &[String], kind: impl Fn(&str) -> &'static str) -> Result<()> {
    let mut declared = vec!["_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT".to_owned()];
    for name in names {
        let kind = kind(name);
        declared.push(if kind.is_empty() {
            format!("{} NULL DEFAULT NULL", quote(name))
        } else {
            format!("{} {kind} DEFAULT NULL", quote(name))
        });
    }
    connection.execute_batch(&format!("CREATE TABLE {}({});", quote(table), declared.join(", ")))?;
    Ok(())
}

fn insert(connection: &Connection, table: &str, names: &[String], cells: &Cells) -> Result<()> {
    let values: Vec<Value> = names.iter().map(|name| get(cells, name).cloned().unwrap_or(Value::Null)).collect();
    let placeholders: Vec<String> = (1..=names.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "INSERT INTO {} ({}) VALUES ({})",
        quote(table),
        names.iter().map(|n| quote(n)).collect::<Vec<_>>().join(", "),
        placeholders.join(", ")
    );
    connection.execute(&sql, rusqlite::params_from_iter(values.iter()))?;
    Ok(())
}

/// Write one sub tool as a `.sut`.
///
/// The node needs a `NodeUuid` (16 bytes) and a `NodeVariantID` that one of
/// the variants has; a missing `NodeInitVariantID` points at the settings.
pub fn build_sut(parts: &SutParts) -> Result<Vec<u8>> {
    let uuid = match get(&parts.node, "NodeUuid") {
        Some(Value::Blob(uuid)) if uuid.len() == 16 && !super::is_none(uuid) => uuid.clone(),
        _ => return Err(Error::format("the node needs a 16-byte NodeUuid")),
    };
    let variant_id = |cells: &Cells| match get(cells, "VariantID") {
        Some(Value::Integer(id)) => Some(*id),
        _ => None,
    };
    let ids: Vec<i64> = parts.variants.iter().filter_map(variant_id).collect();
    if ids.len() != parts.variants.len() {
        return Err(Error::format("every settings row needs an integer VariantID"));
    }
    let settings = match get(&parts.node, "NodeVariantID") {
        Some(Value::Integer(id)) if ids.contains(id) => *id,
        _ => return Err(Error::format("NodeVariantID must name one of the settings rows")),
    };

    let mut node = parts.node.clone();
    let set = |cells: &mut Cells, column: &str, value: Value| match cells.iter_mut().find(|(n, _)| n == column) {
        Some(cell) => cell.1 = value,
        None => cells.push((column.to_owned(), value)),
    };
    for link in ["NodeNextUuid", "NodeFirstChildUuid", "NodeSelectedUuid"] {
        set(&mut node, link, Value::Blob(vec![0]));
    }
    if !matches!(get(&node, "NodeInitVariantID"), Some(Value::Integer(id)) if ids.contains(id)) {
        set(&mut node, "NodeInitVariantID", Value::Integer(settings));
    }
    let mut manager = parts.manager.clone();
    set(&mut manager, "RootUuid", Value::Blob(uuid));
    set(&mut manager, "CurrentNodeUuid", Value::Blob(vec![0]));
    set(&mut manager, "MaxVariantID", Value::Integer(ids.iter().copied().max().unwrap_or(0)));

    let out = Connection::open_in_memory()?;
    let manager_names = column_order(MANAGER_COLUMNS.iter().map(|(n, _)| *n), &[&manager]);
    create(&out, "Manager", &manager_names, |name| known_type(MANAGER_COLUMNS, name))?;
    insert(&out, "Manager", &manager_names, &manager)?;
    let node_names = column_order(NODE_COLUMNS.iter().map(|(n, _)| *n), &[&node]);
    create(&out, "Node", &node_names, |name| known_type(NODE_COLUMNS, name))?;
    insert(&out, "Node", &node_names, &node)?;
    let rows: Vec<&Cells> = parts.variants.iter().collect();
    let variant_names = column_order(["VariantID", "VariantShowSeparator", "VariantShowParam"], &rows);
    create(&out, "Variant", &variant_names, variant_column_type)?;
    for row in &parts.variants {
        insert(&out, "Variant", &variant_names, row)?;
    }
    out.execute_batch(
        "CREATE TABLE MaterialFile(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, InstallFolder INTEGER DEFAULT NULL, OriginalPath TEXT DEFAULT NULL, OldMaterial INTEGER DEFAULT NULL, FileData BLOB DEFAULT NULL, CatalogPath TEXT DEFAULT NULL, MaterialUuid TEXT DEFAULT NULL);",
    )?;
    for material in &parts.materials {
        out.execute(
            "INSERT INTO MaterialFile(InstallFolder, OriginalPath, OldMaterial, FileData, CatalogPath, MaterialUuid) VALUES (?1, ?2, 0, ?3, ?4, '')",
            rusqlite::params![
                material.install_folder,
                material.original_path.trim_end_matches('\0'),
                material.file_data,
                material.catalog_path
            ],
        )?;
    }
    serialize(&out)
}
