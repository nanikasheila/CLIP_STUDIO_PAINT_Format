//! CSP's own schema registry inside every .clip / .cmc database.
//!
//! - `ParamScheme`: one row per column CSP knows (`TableName`, `LabelName`),
//!   with a data type code, an owner element kind and, for id columns, the
//!   table the id points into (`LinkTable`). It lists every table of the
//!   saving CSP version, also tables the file does not contain.
//! - `ElemScheme`: one row per table with its element kind (`ElemType`) and
//!   the highest `MainId` handed out so far (`MaxIndex`).
//! - `ExternalTableAndColumnName`: the columns whose values are external
//!   chunk ids (pixel blocks, vector data, 3D data, time-lapse data).
//!
//! See `docs/spec/02-database.md` for what the codes mean.

use std::collections::{BTreeMap, HashMap};

use rusqlite::Connection;

use crate::database::{self, select_all};
use crate::error::Result;

/// `ParamScheme.DataType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
#[non_exhaustive]
pub enum DataType {
    Integer,
    Real,
    Text,
    Blob,
    Unknown(i64),
}

impl DataType {
    pub fn from_code(code: i64) -> Self {
        match code {
            1 => DataType::Integer,
            2 => DataType::Real,
            3 => DataType::Text,
            4 => DataType::Blob,
            other => DataType::Unknown(other),
        }
    }

    pub fn code(&self) -> i64 {
        match self {
            DataType::Integer => 1,
            DataType::Real => 2,
            DataType::Text => 3,
            DataType::Blob => 4,
            DataType::Unknown(code) => *code,
        }
    }

    /// The SQLite declared type CSP uses for new columns of this type.
    pub fn sql_type(&self) -> &'static str {
        match self {
            DataType::Integer => "INTEGER",
            DataType::Real => "REAL",
            DataType::Text => "TEXT",
            DataType::Blob => "BLOB",
            DataType::Unknown(_) => "",
        }
    }
}

/// Element kinds (`ElemScheme.ElemType`, `ParamScheme.OwnerType`). An
/// owner column holds the `MainId` of the element of that kind that owns
/// the row (for example `Offscreen.LayerId`, owner kind 4 = layer).
pub fn element_kind_name(code: i64) -> &'static str {
    match code {
        0 => "none",
        1 => "project",
        2 => "canvas",
        4 => "layer",
        8 => "layer_object",
        1024 => "bank",
        2048 => "item",
        4096 => "track",
        _ => "unknown",
    }
}

/// One `ParamScheme` row.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Param {
    pub table: String,
    pub column: String,
    pub data_type: DataType,
    pub flag: i64,
    pub owner_type: i64,
    pub lock_type: i64,
    pub lock_specified: i64,
    pub alternative_lock_type: Option<i64>,
    pub alternative_lock_specified: Option<i64>,
    /// Table an id column links to (`MainId` there); None for plain values.
    pub link_table: Option<String>,
}

/// One `ElemScheme` row.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Elem {
    pub table: String,
    pub elem_type: i64,
    pub max_index: i64,
}

/// The registry of one database.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Schema {
    pub params: Vec<Param>,
    pub elems: Vec<Elem>,
    pub external_columns: Vec<(String, String)>,
}

impl Schema {
    pub fn read(connection: &Connection) -> Result<Self> {
        let params = select_all(connection, "ParamScheme")?
            .iter()
            .map(|row| Param {
                table: row.text("TableName").unwrap_or_default(),
                column: row.text("LabelName").unwrap_or_default(),
                data_type: DataType::from_code(row.int("DataType")),
                flag: row.int("Flag"),
                owner_type: row.int("OwnerType"),
                lock_type: row.int("LockType"),
                lock_specified: row.int("LockSpecified"),
                alternative_lock_type: row.opt_int("AlternativeLockType"),
                alternative_lock_specified: row.opt_int("AlternativeLockSpecified"),
                link_table: row.text("LinkTable").filter(|text| !text.is_empty()),
            })
            .collect();
        let elems = select_all(connection, "ElemScheme")?
            .iter()
            .map(|row| Elem {
                table: row.text("TableName").unwrap_or_default(),
                elem_type: row.int("ElemType"),
                max_index: row.int("MaxIndex"),
            })
            .collect();
        let external_columns = select_all(connection, "ExternalTableAndColumnName")?
            .iter()
            .map(|row| (row.text("TableName").unwrap_or_default(), row.text("ColumnName").unwrap_or_default()))
            .collect();
        Ok(Schema { params, elems, external_columns })
    }

    pub fn param(&self, table: &str, column: &str) -> Option<&Param> {
        self.params.iter().find(|param| param.table == table && param.column == column)
    }

    pub fn table_params(&self, table: &str) -> impl Iterator<Item = &Param> {
        self.params.iter().filter(move |param| param.table == table)
    }

    pub fn elem(&self, table: &str) -> Option<&Elem> {
        self.elems.iter().find(|elem| elem.table == table)
    }

    pub fn is_external(&self, table: &str, column: &str) -> bool {
        self.external_columns.iter().any(|(t, c)| t == table && c == column)
    }

    /// Id links: (table, column) → linked table.
    pub fn links(&self) -> BTreeMap<(String, String), String> {
        self.params
            .iter()
            .filter_map(|param| Some(((param.table.clone(), param.column.clone()), param.link_table.clone()?)))
            .collect()
    }

    /// Tables in registry order.
    pub fn table_names(&self) -> Vec<String> {
        let mut seen = HashMap::new();
        let mut names = Vec::new();
        for param in &self.params {
            if seen.insert(param.table.clone(), ()).is_none() {
                names.push(param.table.clone());
            }
        }
        names
    }
}

/// Hand out the next `MainId` of `table`, as CSP does: one more than both
/// `ElemScheme.MaxIndex` and the highest id in the table, recorded back
/// into `ElemScheme`.
pub fn allocate_main_id(connection: &Connection, table: &str) -> Result<i64> {
    let recorded: i64 = connection
        .query_row("SELECT MaxIndex FROM ElemScheme WHERE TableName = ?1", [table], |row| row.get::<_, Option<i64>>(0))
        .ok()
        .flatten()
        .unwrap_or(0);
    let used: i64 = if database::table_exists(connection, table)? {
        connection.query_row(
            &format!("SELECT COALESCE(MAX(MainId), 0) FROM {}", database::quote(table)),
            [],
            |row| row.get(0),
        )?
    } else {
        0
    };
    let next = recorded.max(used) + 1;
    let updated = connection.execute("UPDATE ElemScheme SET MaxIndex = ?1 WHERE TableName = ?2", (next, table))?;
    if updated == 0 && database::table_exists(connection, "ElemScheme")? {
        connection
            .execute("INSERT INTO ElemScheme(TableName, ElemType, MaxIndex) VALUES (?1, 0, ?2)", (table, next))?;
    }
    Ok(next)
}
