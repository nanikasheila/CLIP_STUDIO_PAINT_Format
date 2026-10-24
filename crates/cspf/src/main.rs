//! `cspf`: command line tool for CLIP STUDIO PAINT files.
//!
//! Input files are only read; outputs go to the paths given.

use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use csp_format::verify::{CheckOptions, check_clip, check_cmc};
use csp_format::{ClipFile, Result};
use serde_json::{Value, json};

const USAGE: &str = "\
cspf — CLIP STUDIO PAINT file tool

USAGE:
  cspf inspect FILE                 structure summary as JSON (.clip / .cmc)
  cspf layers FILE                  layer tree as text
  cspf schema FILE                  CSP's schema registry (ParamScheme) as JSON
  cspf check FILE...                self-check (.clip: container, database, pixels, vectors,
                                    text, references; .cmc: page list and page files)
  cspf extract FILE OUT_DIR         stored layer pixels and masks as PNG
  cspf preview FILE OUT.png         the preview CSP saved in the file
  cspf corpus ROOT OUT.jsonl [--seconds N] [--no-reencode] [--shard K/N] [--list PATHS.txt]
                                    check every .clip / .cmc under ROOT (resumable)
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first() else {
        eprint!("{USAGE}");
        return ExitCode::from(2);
    };
    let rest = &args[1..];
    let result = match command.as_str() {
        "inspect" => one_file(rest).and_then(inspect),
        "layers" => one_file(rest).and_then(layers),
        "schema" => one_file(rest).and_then(schema),
        "check" => check(rest),
        "extract" => two(rest).and_then(|(file, out)| extract(&file, &out)),
        "preview" => two(rest).and_then(|(file, out)| preview(&file, &out)),
        "corpus" => corpus(rest),
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            Ok(true)
        }
        other => {
            eprintln!("Unknown command {other:?}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(1)
        }
    }
}

fn one_file(args: &[String]) -> Result<PathBuf> {
    match args {
        [file] => Ok(PathBuf::from(file)),
        _ => Err(csp_format::Error::format(format!("expected one file\n\n{USAGE}"))),
    }
}

fn two(args: &[String]) -> Result<(PathBuf, PathBuf)> {
    match args {
        [a, b] => Ok((PathBuf::from(a), PathBuf::from(b))),
        _ => Err(csp_format::Error::format(format!("expected two paths\n\n{USAGE}"))),
    }
}

fn print_json(value: &Value) {
    println!("{}", serde_json::to_string_pretty(value).expect("JSON"));
}

fn is_cmc(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("cmc"))
}

fn inspect(path: PathBuf) -> Result<bool> {
    if is_cmc(&path) {
        let file = csp_format::CmcFile::open(&path)?;
        let pages: Vec<Value> = file
            .pages()?
            .iter()
            .map(|node| json!({"id": node.main_id, "file": node.relative_path().map(|p| p.display().to_string()), "spread": node.spread}))
            .collect();
        print_json(&json!({"file": path.display().to_string(), "project": file.project, "pages": pages}));
        return Ok(true);
    }
    let file = ClipFile::open(&path)?;
    let layers: Vec<Value> = file
        .layers
        .flatten()
        .into_iter()
        .map(|(layer, depth)| {
            json!({
                "depth": depth,
                "id": layer.main_id,
                "name": layer.name,
                "kind": layer.kind.name(),
                "type": layer.layer_type,
                "visible": layer.visible,
                "opacity": layer.opacity,
                "blend": layer.blend,
                "clipping": layer.clipping,
                "mask": layer.has_mask(),
                "offset": [layer.offset.0, layer.offset.1],
            })
        })
        .collect();
    print_json(&json!({
        "file": path.display().to_string(),
        "project": file.project,
        "canvas": file.canvas,
        "chunks": file.container().chunks.len(),
        "layers": layers,
    }));
    Ok(true)
}

fn layers(path: PathBuf) -> Result<bool> {
    let file = ClipFile::open(&path)?;
    for (layer, depth) in file.layers.flatten() {
        println!(
            "{}{} [{}] {}{}{} opacity={} blend={}",
            "  ".repeat(depth),
            layer.name,
            layer.kind.name(),
            if layer.visible { "" } else { "hidden " },
            if layer.clipping { "clip " } else { "" },
            if layer.has_mask() { "mask " } else { "" },
            layer.opacity,
            layer.blend
        );
    }
    Ok(true)
}

fn schema(path: PathBuf) -> Result<bool> {
    let data = std::fs::read(&path)?;
    let connection = if csp_format::container::Container::sniff(&data) {
        csp_format::database::open_bytes(csp_format::container::Container::parse(data)?.sqlite()?)?
    } else {
        csp_format::database::open_bytes(&data)?
    };
    let schema = csp_format::schema::Schema::read(&connection)?;
    print_json(&serde_json::to_value(&schema).expect("JSON"));
    Ok(true)
}

fn check(args: &[String]) -> Result<bool> {
    let mut all_ok = true;
    for path in args {
        let data = std::fs::read(path)?;
        let (ok, report) = if is_cmc(Path::new(path)) {
            let report = check_cmc(data, Path::new(path).parent());
            (report.ok(), serde_json::to_value(&report).expect("JSON"))
        } else {
            let report = check_clip(data, CheckOptions::default());
            (report.ok(), serde_json::to_value(&report).expect("JSON"))
        };
        all_ok &= ok;
        print_json(&json!({"file": path, "ok": ok, "report": report}));
    }
    Ok(all_ok)
}

fn extract(path: &Path, out: &Path) -> Result<bool> {
    let file = ClipFile::open(path)?;
    std::fs::create_dir_all(out)?;
    let mut count = 0;
    for (layer, _) in file.layers.flatten() {
        if let Some((image, origin)) = file.layer_pixels(layer)? {
            let name =
                out.join(format!("layer{:04}_{}_{}x{}.png", layer.main_id, layer.kind.name(), origin.0, origin.1));
            std::fs::write(name, image.to_png()?)?;
            count += 1;
        }
        if let Some((mask, origin)) = file.layer_mask(layer)? {
            let name = out.join(format!("layer{:04}_mask_{}x{}.png", layer.main_id, origin.0, origin.1));
            std::fs::write(name, mask.to_png()?)?;
            count += 1;
        }
    }
    eprintln!("wrote {count} images");
    Ok(true)
}

fn preview(path: &Path, out: &Path) -> Result<bool> {
    let file = ClipFile::open(path)?;
    match file.preview()? {
        Some(image) => {
            std::fs::write(out, image.to_png()?)?;
            Ok(true)
        }
        None => {
            eprintln!("no preview stored");
            Ok(false)
        }
    }
}

/// Stable short key of a path relative to the corpus root (FNV-1a).
fn key_of(relative: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in relative.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn walk(root: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            walk(&path, found);
        } else if kind.is_file()
            && path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("clip") || ext.eq_ignore_ascii_case("cmc"))
        {
            found.push(path);
        }
    }
}

fn corpus(args: &[String]) -> Result<bool> {
    let mut positional = Vec::new();
    let mut seconds = 0.0;
    let mut options = CheckOptions::default();
    let mut shard: (usize, usize) = (0, 1);
    let mut list: Option<String> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--seconds" => seconds = iter.next().and_then(|s| s.parse().ok()).unwrap_or(0.0),
            "--no-reencode" => options.reencode_pixels = false,
            "--list" => list = iter.next().cloned(),
            "--shard" => {
                let text = iter.next().cloned().unwrap_or_default();
                let (k, n) = text.split_once('/').unwrap_or(("0", "1"));
                shard = (k.parse().unwrap_or(0), n.parse().unwrap_or(1));
            }
            _ => positional.push(arg.clone()),
        }
    }
    let [root, out] = positional.as_slice() else {
        return Err(csp_format::Error::format(format!("expected ROOT OUT.jsonl\n\n{USAGE}")));
    };
    let root = PathBuf::from(root);
    let mut done = HashSet::new();
    if let Ok(existing) = std::fs::File::open(out) {
        for line in std::io::BufReader::new(existing).lines().map_while(std::result::Result::ok) {
            if let Ok(value) = serde_json::from_str::<Value>(&line)
                && let Some(key) = value.get("key").and_then(Value::as_str)
            {
                done.insert(key.to_owned());
            }
        }
    }
    let mut files = Vec::new();
    match &list {
        // A list of paths (relative to ROOT, one per line) avoids walking slow folders.
        Some(list) => {
            for line in std::fs::read_to_string(list)?.lines().map(str::trim).filter(|line| !line.is_empty()) {
                files.push(root.join(line.trim_start_matches("./")));
            }
        }
        None => walk(&root, &mut files),
    }
    files.sort();
    let mut output = std::fs::OpenOptions::new().create(true).append(true).open(out)?;
    // A run that was killed may have left half a line; start on a new one.
    if std::fs::read(out).is_ok_and(|bytes| bytes.last().is_some_and(|last| *last != b'\n')) {
        writeln!(output)?;
    }
    let started = Instant::now();
    let (mut checked, mut failed, mut remaining) = (0, 0, 0);
    for (index, path) in files.iter().enumerate() {
        if index % shard.1.max(1) != shard.0 {
            continue;
        }
        let relative = path.strip_prefix(&root).unwrap_or(path).to_string_lossy().replace('\\', "/");
        let key = key_of(&relative);
        if done.contains(&key) {
            continue;
        }
        if seconds > 0.0 && started.elapsed().as_secs_f64() > seconds {
            remaining += 1;
            continue;
        }
        let file_started = Instant::now();
        let (size, ok, report) = match std::fs::read(path) {
            Ok(data) if is_cmc(path) => {
                let size = data.len();
                let report = check_cmc(data, path.parent());
                (size, report.ok(), serde_json::to_value(&report).expect("JSON"))
            }
            Ok(data) => {
                let size = data.len();
                let report = check_clip(data, options);
                (size, report.ok(), serde_json::to_value(&report).expect("JSON"))
            }
            Err(error) => (0, false, json!({"errors": [format!("read: {error}")]})),
        };
        failed += (!ok) as usize;
        checked += 1;
        let line = json!({
            "key": key,
            "path": relative,
            "size": size,
            "ok": ok,
            "seconds": file_started.elapsed().as_secs_f64(),
            "report": report,
        });
        writeln!(output, "{}", serde_json::to_string(&line).expect("JSON"))?;
        output.flush()?;
    }
    eprintln!("checked {checked} files ({failed} not ok), {} already done, {remaining} remaining", done.len());
    Ok(true)
}
