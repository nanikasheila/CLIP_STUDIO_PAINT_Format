//! `cspf asset …`: CSP's files other than canvases (sub tools, auto
//! actions, colour sets, gradient sets, material folders, settings
//! databases). Requirements: `docs/assets/requirements.md` (A13).

use std::io::Write;
use std::path::{Path, PathBuf};

use csp_assets::action::ActionSet;
use csp_assets::colorset::{Color, ColorSet};
use csp_assets::gradient::GradientSet;
use csp_assets::kind::{Family, detect};
use csp_assets::material::{self, Catalog, Element, InfoList, MaterialFolder};
use csp_assets::serial::Document;
use csp_assets::shortcut::Shortcuts;
use csp_assets::subtool::{self, SubToolDb};
use csp_assets::{c2f, tar};
use csp_format::{Error, Result};
use rusqlite::types::Value as SqlValue;
use serde_json::{Value, json};

pub const USAGE: &str = "\
cspf asset — sub tools, auto actions, colour and gradient sets, materials, settings

USAGE:
  cspf asset kind FILE...           format of each file (family, purpose)
  cspf asset inspect FILE           summary as JSON (any supported format)
  cspf asset subtools FILE          tool / group / sub tool tree (.sut, EditImageTool.todb, .bfps)
  cspf asset settings FILE SUBTOOL [--initial]
                                    a sub tool's settings as JSON (SUBTOOL: name or group/name)
  cspf asset sut-extract FILE SUBTOOL OUT.sut [--materials DIR]...
                                    write one sub tool as a .sut, packing the materials it
                                    uses found under DIR (CSP's material folders)
  cspf asset sut-set FILE SUBTOOL OUT [--name NAME] [COLUMN=VALUE]...
                                    change settings (VALUE: integer, real with '.', s:TEXT,
                                    x:HEX bytes, null) and/or the name; writes a copy
  cspf asset dump FILE              auto action / gradient set / other serial file as JSON
  cspf asset undump JSON OUT        the JSON of `dump` back to the file (byte for byte)
  cspf asset actions FILE           auto actions and their commands as text (.las / .laf)
  cspf asset colorset FILE          colour set as JSON (.cls)
  cspf asset colorset-new OUT NAME COLOR...
                                    a colour set (COLOR: RRGGBB or RRGGBBAA, optional =NAME)
  cspf asset gradients FILE         gradient set as JSON (.sgs)
  cspf asset shortcuts FILE [--tools EditImageTool.todb]
                                    shortcut keys as text: command, then keys (.khc, .spws);
                                    tool keys are named from the tool DB when given
  cspf asset catalog DIR            material folders under DIR as JSON (no decryption)
  cspf asset corpus ROOT OUT.jsonl  read every file under ROOT and check round trips
";

pub fn run(args: &[String]) -> Result<bool> {
    let Some(command) = args.first() else {
        print!("{USAGE}");
        return Ok(false);
    };
    let rest = &args[1..];
    match command.as_str() {
        "kind" => kind(rest),
        "inspect" => one(rest).and_then(|f| inspect(&f)),
        "subtools" => one(rest).and_then(|f| subtools(&f)),
        "settings" => settings(rest),
        "sut-extract" => sut_extract(rest),
        "sut-set" => sut_set(rest),
        "dump" => one(rest).and_then(|f| dump(&f)),
        "undump" => undump(rest),
        "actions" => one(rest).and_then(|f| actions(&f)),
        "colorset" => one(rest).and_then(|f| colorset(&f)),
        "shortcuts" => shortcuts(rest),
        "colorset-new" => colorset_new(rest),
        "gradients" => one(rest).and_then(|f| gradients(&f)),
        "catalog" => one(rest).and_then(|d| catalog(&d)),
        "corpus" => corpus(rest),
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            Ok(true)
        }
        other => Err(Error::format(format!("unknown asset command {other:?}\n\n{USAGE}"))),
    }
}

fn usage_error(what: &str) -> Error {
    Error::format(format!("{what}\n\n{USAGE}"))
}

fn one(args: &[String]) -> Result<PathBuf> {
    match args {
        [file] => Ok(PathBuf::from(file)),
        _ => Err(usage_error("expected one path")),
    }
}

fn print_json(value: &Value) {
    println!("{}", serde_json::to_string_pretty(value).expect("JSON"));
}

fn to_json(value: impl serde::Serialize) -> Value {
    serde_json::to_value(value).expect("JSON")
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn kind(args: &[String]) -> Result<bool> {
    if args.is_empty() {
        return Err(usage_error("expected files"));
    }
    let mut all = true;
    for file in args {
        let path = Path::new(file);
        let data = std::fs::read(path)?;
        let detected = detect(&file_name(path), &data);
        all &= detected.consistent;
        let (pattern, purpose) = detected.format.map_or(("?", "?"), |f| (f.pattern, f.purpose));
        let family = to_json(detected.family);
        let note = if detected.consistent { "" } else { "\t(content does not match the name)" };
        println!("{}\t{pattern}\t{purpose}\t{file}{note}", family.as_str().unwrap_or("?"));
    }
    Ok(all)
}

/// Tables, columns and row counts of any SQLite file (A2).
fn sqlite_summary(data: &[u8]) -> Result<Value> {
    let connection = csp_format::database::open_bytes(data)?;
    let mut statement = connection.prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")?;
    let names: Vec<String> = statement.query_map([], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
    let mut tables = serde_json::Map::new();
    for name in names {
        let columns = csp_format::database::columns(&connection, &name)?;
        let rows: i64 = connection.query_row(
            &format!("SELECT count(*) FROM {}", csp_format::database::quote(&name)),
            [],
            |row| row.get(0),
        )?;
        tables.insert(name, json!({ "rows": rows, "columns": columns.len() }));
    }
    Ok(Value::Object(tables))
}

fn subtool_summary(db: &SubToolDb) -> Value {
    let (blobs, failures) = subtool::check_blobs(db);
    json!({
        "kind": to_json(db.kind),
        "nodes": db.nodes.len(),
        "subTools": db.sub_tools().len(),
        "variants": db.variants.len(),
        "tree": to_json(db.check_tree()),
        "blobs": blobs,
        "blobFailures": failures.len(),
        "materials": db.materials.iter().map(|m| json!({
            "originalPath": m.original_path,
            "catalogPath": m.catalog_path,
            "installFolder": m.install_folder,
            "files": m.entries().map(|e| e.iter().map(|x| json!({"name": x.name, "bytes": x.data.len()})).collect::<Vec<_>>()).unwrap_or_default(),
        })).collect::<Vec<_>>(),
    })
}

fn inspect(path: &Path) -> Result<bool> {
    let data = std::fs::read(path)?;
    let name = file_name(path);
    let detected = detect(&name, &data);
    let mut out = json!({
        "family": to_json(detected.family),
        "format": detected.format.map(|f| json!({"pattern": f.pattern, "purpose": f.purpose, "support": to_json(f.support)})),
        "bytes": data.len(),
    });
    let detail = match detected.family {
        Family::Sqlite => match SubToolDb::from_bytes(&data) {
            Ok(db) => subtool_summary(&db),
            Err(_) => {
                let mut detail = json!({ "tables": sqlite_summary(&data)? });
                if let Ok(keys) = Shortcuts::from_bytes(&data) {
                    detail["shortcuts"] = json!({"commands": keys.commands.len(), "unassigned": keys.unassigned.len(), "tools": keys.tools.len()});
                }
                detail
            }
        },
        Family::Serial => {
            let document = Document::parse(&data)?;
            if let Ok(set) = ActionSet::parse(&data) {
                json!({ "actionSet": set.name(), "actions": set.actions().iter().map(|a| json!({"name": a.name, "items": a.items.len()})).collect::<Vec<_>>() })
            } else if let Ok(set) = GradientSet::from_document(&document) {
                json!({ "gradientGroups": set.groups.iter().map(|g| json!({"name": g.name, "gradients": g.gradients.len()})).collect::<Vec<_>>() })
            } else {
                json!({ "typename": document.root.typename(), "keys": document.root.body.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>() })
            }
        }
        Family::ColorSet => {
            let set = ColorSet::parse(&data)?;
            json!({ "name": set.name, "colors": set.colors.len() })
        }
        Family::C2f => to_json(c2f::summarize(&data)?),
        Family::Xml => {
            if let Ok(catalog) = Catalog::parse(&data) {
                to_json(catalog)
            } else if let Ok(list) = InfoList::parse(&data) {
                to_json(list)
            } else {
                let root = Element::parse(&data)?;
                json!({ "root": root.name, "children": root.children.len() })
            }
        }
        Family::Tar => {
            json!({ "files": tar::read(&data)?.iter().map(|e| json!({"name": e.name, "bytes": e.data.len()})).collect::<Vec<_>>() })
        }
        Family::CatalogCache => json!({ "strings": material::cac_strings(&data) }),
        Family::Unknown if name.eq_ignore_ascii_case("InstallPathToCatalogId.dat") => {
            to_json(material::InstallMap::parse(&data)?)
        }
        _ => Value::Null,
    };
    if !detail.is_null() {
        out["content"] = detail;
    }
    print_json(&out);
    Ok(true)
}

fn open_db(path: &Path) -> Result<SubToolDb> {
    SubToolDb::open(path)
}

fn find(db: &SubToolDb, name: &str) -> Result<usize> {
    db.find(name).map(|e| e.node).ok_or_else(|| Error::format(format!("no sub tool named {name:?}")))
}

fn subtools(path: &Path) -> Result<bool> {
    let db = open_db(path)?;
    for entry in db.tree() {
        let node = &db.nodes[entry.node];
        let label = if node.name.is_empty() && entry.depth == 0 { "(root)" } else { &node.name };
        let kind = to_json(node.kind());
        let ids = if node.variant_id != 0 {
            format!("  [settings {} / initial {}]", node.variant_id, node.init_variant_id)
        } else {
            String::new()
        };
        println!("{}{label}  <{}>{ids}", "  ".repeat(entry.depth), kind.as_str().unwrap_or("?"));
    }
    let check = db.check_tree();
    if !check.closed() {
        eprintln!("tree problems: {}", serde_json::to_string(&check).expect("JSON"));
    }
    Ok(check.closed())
}

fn settings(args: &[String]) -> Result<bool> {
    let (file, name, initial) = match args {
        [file, name] => (file, name, false),
        [file, name, flag] if flag == "--initial" => (file, name, true),
        _ => return Err(usage_error("expected FILE SUBTOOL [--initial]")),
    };
    let db = open_db(Path::new(file))?;
    let node = &db.nodes[find(&db, name)?];
    let id = if initial { node.init_variant_id } else { node.variant_id };
    let settings = db.variant_json(id).ok_or_else(|| Error::format(format!("settings row {id} is missing")))?;
    print_json(&json!({ "name": node.name, "kind": to_json(node.kind()), "variantId": id, "settings": settings }));
    Ok(true)
}

fn sut_extract(args: &[String]) -> Result<bool> {
    let [file, name, out, options @ ..] = args else {
        return Err(usage_error("expected FILE SUBTOOL OUT.sut"));
    };
    let mut roots = Vec::new();
    let mut options = options.iter();
    while let Some(option) = options.next() {
        match (option.as_str(), options.next()) {
            ("--materials", Some(dir)) => roots.push(PathBuf::from(dir)),
            _ => return Err(usage_error(&format!("unknown option {option}"))),
        }
    }
    let db = open_db(Path::new(file))?;
    let node = find(&db, name)?;
    let index = material::index_by_uuid(&roots);
    let missing = std::cell::RefCell::new(Vec::new());
    let bytes = db.extract_sut(node, &|reference| {
        let found = material::resolve_ref(&index, &reference.path)
            .and_then(|dir| material::sut_tar(&material::DirSource(dir)).ok().flatten());
        if found.is_none() {
            missing.borrow_mut().push(reference.name.clone().unwrap_or_else(|| reference.path.clone()));
        }
        found
    })?;
    std::fs::write(out, bytes)?;
    let missing = missing.into_inner();
    for name in &missing {
        eprintln!("material not found (not packed): {name}");
    }
    Ok(missing.is_empty())
}

fn parse_value(text: &str) -> Result<SqlValue> {
    if text == "null" {
        Ok(SqlValue::Null)
    } else if let Some(rest) = text.strip_prefix("s:") {
        Ok(SqlValue::Text(rest.to_owned()))
    } else if let Some(rest) = text.strip_prefix("x:") {
        let hex: String = rest.chars().filter(|c| !c.is_whitespace()).collect();
        if !hex.len().is_multiple_of(2) {
            return Err(Error::format("x: needs an even number of hex digits"));
        }
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
            .collect::<std::result::Result<Vec<u8>, _>>()
            .map(SqlValue::Blob)
            .map_err(|_| Error::format(format!("bad hex {rest:?}")))
    } else if text.contains('.') || text.contains('e') {
        text.parse().map(SqlValue::Real).map_err(|_| Error::format(format!("bad number {text:?}")))
    } else {
        text.parse()
            .map(SqlValue::Integer)
            .map_err(|_| Error::format(format!("bad value {text:?} (use s:TEXT for text)")))
    }
}

fn sut_set(args: &[String]) -> Result<bool> {
    let [file, name, out, changes @ ..] = args else {
        return Err(usage_error("expected FILE SUBTOOL OUT [--name NAME] [COLUMN=VALUE]..."));
    };
    let mut db = open_db(Path::new(file))?;
    let node = find(&db, name)?;
    let variant = db.nodes[node].variant_id;
    let mut changes = changes.iter();
    while let Some(change) = changes.next() {
        if change == "--name" {
            let new_name = changes.next().ok_or_else(|| usage_error("--name needs a value"))?;
            db.rename(node, new_name)?;
        } else if let Some((column, value)) = change.split_once('=') {
            db.set_value(variant, column, parse_value(value)?)?;
        } else {
            return Err(usage_error(&format!("expected COLUMN=VALUE, got {change:?}")));
        }
    }
    std::fs::write(out, db.to_bytes()?)?;
    Ok(true)
}

fn dump(path: &Path) -> Result<bool> {
    print_json(&Document::parse(&std::fs::read(path)?)?.to_json());
    Ok(true)
}

fn undump(args: &[String]) -> Result<bool> {
    let [json, out] = args else { return Err(usage_error("expected JSON OUT")) };
    let value: Value =
        serde_json::from_slice(&std::fs::read(json)?).map_err(|e| Error::format(format!("{json}: {e}")))?;
    std::fs::write(out, Document::from_json(&value)?.to_bytes())?;
    Ok(true)
}

fn actions(path: &Path) -> Result<bool> {
    let set = ActionSet::parse(&std::fs::read(path)?)?;
    println!("{}", set.name());
    for action in set.actions() {
        println!("  {}", action.name);
        for item in action.items {
            let mark = if item.enabled { "x" } else { " " };
            let dialog = if item.show_settings { "  (dialog)" } else { "" };
            println!("    [{mark}] {}{dialog}", item.kind);
        }
    }
    Ok(true)
}

fn shortcuts(args: &[String]) -> Result<bool> {
    let (path, tools) = match args {
        [path] => (path, None),
        [path, flag, tools] if flag == "--tools" => (path, Some(SubToolDb::open(tools)?)),
        _ => return Err(usage_error("expected FILE [--tools EditImageTool.todb]")),
    };
    let names: std::collections::HashMap<String, String> =
        tools.iter().flat_map(|db| db.nodes.iter().map(|n| (n.uuid_hex(), n.name.clone()))).collect();
    let shortcuts = Shortcuts::from_bytes(&std::fs::read(path)?)?;
    let mut done = std::collections::HashSet::new();
    for s in &shortcuts.commands {
        if done.insert(s.command.as_str()) {
            println!("{}\t{}", s.command, shortcuts.keys(&s.command).join("  "));
        }
    }
    for tool in &shortcuts.tools {
        let name = names.get(&tool.tool).map_or(tool.tool.as_str(), String::as_str);
        println!("tool:{name}\t{}", tool.key);
    }
    Ok(true)
}

fn colorset(path: &Path) -> Result<bool> {
    let set = ColorSet::parse(&std::fs::read(path)?)?;
    print_json(&to_json(&set));
    Ok(true)
}

fn parse_color(text: &str) -> Result<Color> {
    let (hex, name) = match text.split_once('=') {
        Some((hex, name)) => (hex, Some(name)),
        None => (text, None),
    };
    let hex = hex.trim_start_matches('#');
    let byte =
        |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| Error::format(format!("bad colour {text:?}")));
    let rgba = match hex.len() {
        6 => [byte(0)?, byte(2)?, byte(4)?, 255],
        8 => [byte(0)?, byte(2)?, byte(4)?, byte(6)?],
        _ => return Err(Error::format(format!("bad colour {text:?} (RRGGBB or RRGGBBAA)"))),
    };
    Ok(Color::new(rgba, name))
}

fn colorset_new(args: &[String]) -> Result<bool> {
    let [out, name, colors @ ..] = args else { return Err(usage_error("expected OUT NAME COLOR...")) };
    let colors = colors.iter().map(|c| parse_color(c)).collect::<Result<Vec<_>>>()?;
    std::fs::write(out, ColorSet::new(name, colors).to_bytes())?;
    Ok(true)
}

fn gradients(path: &Path) -> Result<bool> {
    print_json(&to_json(GradientSet::parse(&std::fs::read(path)?)?));
    Ok(true)
}

fn catalog(dir: &Path) -> Result<bool> {
    let folders: Vec<Value> = material::scan(dir)
        .iter()
        .map(|folder| match MaterialFolder::open(folder) {
            Ok(m) => json!({ "folder": folder.strip_prefix(dir).unwrap_or(folder).to_string_lossy(), "material": to_json(m) }),
            Err(e) => json!({ "folder": folder.strip_prefix(dir).unwrap_or(folder).to_string_lossy(), "error": e.to_string() }),
        })
        .collect();
    print_json(&Value::Array(folders));
    Ok(true)
}

/// Read one file the way its family needs and check what can be checked.
/// Returns (checks passed, details).
fn check_file(name: &str, data: &[u8]) -> (bool, Value) {
    let detected = detect(name, data);
    let result: Result<Value> = (|| {
        Ok(match detected.family {
            Family::Sqlite => match SubToolDb::from_bytes(data) {
                Ok(db) => {
                    let summary = subtool_summary(&db);
                    let mut tars = 0;
                    for m in &db.materials {
                        let again = tar::write_at(&m.entries()?, tar::mtime(&m.file_data).unwrap_or(0))?;
                        if again != m.file_data {
                            return Err(Error::format("material tar does not round-trip"));
                        }
                        tars += 1;
                    }
                    let ok = db.check_tree().closed() && summary["blobFailures"] == 0;
                    json!({ "ok": ok, "subtool": summary, "materialTars": tars })
                }
                Err(_) => json!({ "ok": true, "tables": sqlite_summary(data)?.as_object().map_or(0, |t| t.len()) }),
            },
            Family::Serial => {
                let document = Document::parse(data)?;
                let bytes_ok = document.to_bytes() == data;
                let json_ok = Document::from_json(&document.to_json())?.to_bytes() == data;
                let mut typed = Value::Null;
                if let Ok(set) = GradientSet::from_document(&document) {
                    typed = json!({ "gradients": set.groups.iter().map(|g| g.gradients.len()).sum::<usize>() });
                } else if let Ok(set) = ActionSet::parse(data) {
                    typed = json!({ "actions": set.actions().len() });
                }
                json!({ "ok": bytes_ok && json_ok, "bytes": bytes_ok, "json": json_ok, "typed": typed })
            }
            Family::ColorSet => {
                let set = ColorSet::parse(data)?;
                json!({ "ok": set.to_bytes() == data, "colors": set.colors.len() })
            }
            Family::C2f => {
                let chunks = c2f::parse(data)?;
                let crc = chunks.iter().all(|c| c.crc_ok);
                json!({ "ok": crc && c2f::write(&chunks) == data, "chunks": chunks.len(), "crc": crc })
            }
            Family::Xml => {
                let typed = if name == "catalog.xml" {
                    Catalog::parse(data).map(|c| json!({ "items": c.items.len() }))?
                } else if name == "info.xml" || name == "layerData.xml" {
                    InfoList::parse(data).map(|l| json!({ "infos": l.infos.len() }))?
                } else {
                    json!({ "root": Element::parse(data)?.name })
                };
                json!({ "ok": true, "typed": typed })
            }
            Family::Tar => {
                let entries = tar::read(data)?;
                json!({ "ok": true, "files": entries.len(), "csp": tar::write_at(&entries, tar::mtime(data).unwrap_or(0))? == data })
            }
            Family::CatalogCache => json!({ "ok": !material::cac_strings(data).is_empty() }),
            Family::Unknown if name.eq_ignore_ascii_case("InstallPathToCatalogId.dat") => {
                let map = material::InstallMap::parse(data)?;
                json!({ "ok": map.to_bytes() == data, "entries": map.entries.len() })
            }
            _ => json!({ "ok": true, "skipped": true }),
        })
    })();
    let mut details = match result {
        Ok(details) => details,
        Err(error) => json!({ "ok": false, "error": error.to_string() }),
    };
    let ok = details["ok"] == true && detected.consistent;
    details["family"] = to_json(detected.family);
    details["format"] = json!(detected.format.map(|f| f.pattern));
    details["consistent"] = json!(detected.consistent);
    (ok, details)
}

fn corpus(args: &[String]) -> Result<bool> {
    let [root, out] = args else { return Err(usage_error("expected ROOT OUT.jsonl")) };
    let root = Path::new(root);
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut writer = std::io::BufWriter::new(std::fs::File::create(out)?);
    let (mut passed, mut failed) = (0, 0);
    let mut by_family: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    for path in &files {
        let data = std::fs::read(path)?;
        let (ok, mut details) = check_file(&file_name(path), &data);
        details["path"] = json!(path.strip_prefix(root).unwrap_or(path).to_string_lossy());
        writeln!(writer, "{}", serde_json::to_string(&details).expect("JSON"))?;
        let family = by_family.entry(details["family"].as_str().unwrap_or("?").to_owned()).or_default();
        if ok {
            passed += 1;
            family.0 += 1;
        } else {
            failed += 1;
            family.1 += 1;
        }
    }
    writer.flush()?;
    for (family, (ok, bad)) in &by_family {
        println!("{family:<14} {ok:>6} ok {bad:>4} failed");
    }
    println!("{passed} ok, {failed} failed");
    Ok(failed == 0)
}
