//! Shortcut keys: `Shortcut/default.khc` (ショートカットキー設定) and the
//! copy each workspace keeps (`.spws`).
//!
//! SQLite. `shortcutmenu` (`workspaceshortcutmenu` in a workspace) has one
//! row per command and key: `menucommandtype` (`basiccommand` for undo,
//! copy and the like, `advancedcommand` for menu commands), `menucommand`
//! (the command id, e.g. `filesaveas`, or a UUID for some commands),
//! `shortcut` (the key name, `NULL` as text when none) and `modifier`
//! (bits: 1 Alt, 2 Shift, 4 Ctrl). A command with several keys has several
//! rows. `workspaceshortcuttool` maps a key to a tool by the tool node's
//! UUID (`Node.NodeUuid` of the tool DB).

use rusqlite::Connection;
use serde::Serialize;

use csp_format::database::{open_bytes, select_all, table_exists};

use crate::error::{Error, Result};

pub const ALT: i64 = 1;
pub const SHIFT: i64 = 2;
pub const CTRL: i64 = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Shortcut {
    /// `basiccommand` or `advancedcommand`.
    pub kind: String,
    pub command: String,
    /// Key name as CSP stores it (`S`, `F2`, `DELETE`, `NUM+` …).
    pub key: String,
    pub modifiers: i64,
}

impl Shortcut {
    /// The key as CSP shows it, e.g. `Ctrl+Shift+S`.
    pub fn label(&self) -> String {
        let mut out = String::new();
        for (bit, name) in [(CTRL, "Ctrl+"), (SHIFT, "Shift+"), (ALT, "Alt+")] {
            if self.modifiers & bit != 0 {
                out.push_str(name);
            }
        }
        out.push_str(&self.key);
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolShortcut {
    /// The tool node's UUID as hex.
    pub tool: String,
    pub key: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Shortcuts {
    /// Commands with a key (rows without one are left out).
    pub commands: Vec<Shortcut>,
    /// Commands listed without a key.
    pub unassigned: Vec<String>,
    pub tools: Vec<ToolShortcut>,
}

fn no_key(key: &str) -> bool {
    key.is_empty() || key == "NULL"
}

impl Shortcuts {
    pub fn from_bytes(data: &[u8]) -> Result<Shortcuts> {
        if !crate::subtool::looks_like(data) {
            return Err(Error::format("not a shortcut file (not SQLite)"));
        }
        Shortcuts::read(&open_bytes(data)?)
    }

    pub fn read(connection: &Connection) -> Result<Shortcuts> {
        let menu = ["shortcutmenu", "workspaceshortcutmenu"]
            .into_iter()
            .find(|t| table_exists(connection, t).unwrap_or(false))
            .ok_or_else(|| Error::format("no shortcutmenu table (not a .khc or .spws file)"))?;
        let mut out = Shortcuts::default();
        for row in select_all(connection, menu)? {
            let command = row.text("menucommand").unwrap_or_default();
            let key = row.text("shortcut").unwrap_or_default();
            if no_key(&key) {
                out.unassigned.push(command);
                continue;
            }
            out.commands.push(Shortcut {
                kind: row.text("menucommandtype").unwrap_or_default(),
                command,
                key,
                modifiers: row.int("modifier"),
            });
        }
        for row in select_all(connection, "workspaceshortcuttool")? {
            let key = row.text("shortcut").unwrap_or_default();
            if no_key(&key) {
                continue;
            }
            let tool = crate::serial::hex(row.blob("uuid").unwrap_or_default());
            out.tools.push(ToolShortcut { tool, key });
        }
        Ok(out)
    }

    /// The keys of one command.
    pub fn keys(&self, command: &str) -> Vec<String> {
        self.commands.iter().filter(|s| s.command == command).map(Shortcut::label).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use csp_format::database::serialize;

    fn khc() -> Vec<u8> {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE shortcutinfo(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT);
             CREATE TABLE shortcutmenu(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, menucommandtype TEXT,
               menucommand TEXT, shortcut TEXT, modifier INTEGER);
             INSERT INTO shortcutmenu(menucommandtype, menucommand, shortcut, modifier) VALUES
               ('advancedcommand', 'filesaveas', 'S', 6), ('advancedcommand', 'filesaveaspng', 'NULL', 0),
               ('basiccommand', 'undo', 'Z', 4), ('basiccommand', 'undo', 'Z', 0),
               ('advancedcommand', 'editpaint', 'DELETE', 1);",
        )
        .unwrap();
        serialize(&c).unwrap()
    }

    #[test]
    fn reads_commands_and_keys() {
        let s = Shortcuts::from_bytes(&khc()).unwrap();
        assert_eq!(s.keys("filesaveas"), ["Ctrl+Shift+S"]);
        assert_eq!(s.keys("undo"), ["Ctrl+Z", "Z"]);
        assert_eq!(s.keys("editpaint"), ["Alt+DELETE"]);
        assert_eq!(s.unassigned, ["filesaveaspng"]);
        assert!(s.tools.is_empty());
    }

    #[test]
    fn rejects_other_databases() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE other(a);").unwrap();
        assert!(Shortcuts::from_bytes(&serialize(&c).unwrap()).is_err());
        assert!(Shortcuts::from_bytes(b"SLCC").is_err());
    }
}
