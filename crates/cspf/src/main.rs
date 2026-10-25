//! `cspf`: command line tool for CLIP STUDIO PAINT files.
//!
//! Input files are only read; outputs go to the paths given.

mod asset;

use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use csp_format::edit::NewLayer;
use csp_format::render::BlendMode;
use csp_format::verify::{CheckOptions, check_clip, check_cmc};
use csp_format::{ClipFile, Result};
use serde_json::{Value, json};

const USAGE: &str = "\
cspf — CLIP STUDIO PAINT file tool

USAGE:
  cspf inspect FILE                 structure summary as JSON (.clip / .cmc)
  cspf layers FILE                  layer tree as text
  cspf schema FILE                  CSP's schema registry (ParamScheme) as JSON
  cspf version FILE...              which CSP generation saved each file, and what locks it
  cspf check FILE...                self-check (.clip: container, database, pixels, vectors,
                                    text, references; .cmc: page list and page files)
  cspf verify FILE...               the same as check
  cspf roundtrip FILE OUT           save FILE unchanged to OUT and confirm the bytes are equal
  cspf new OUT.clip [--size WxH] [--dpi D] [--paper RRGGBB|none] [--image PNG]
                                    a new file: paper and one raster layer (with PNG's pixels)
  cspf new-work DIR NAME [--trim WxH] [--bleed MM] [--dpi D] [--pages N] [--spreads]
                   [--bind right|left] [--start left|right] [--title T] [--author A]
                                    a new comic work: DIR/NAME.cmc and one page file per page
                                    or spread (sizes in mm; default B5 trim, 5 mm bleed, 600 dpi,
                                    4 pages, right binding, first page on the left)
  cspf add-layer FILE PNG OUT [--name NAME] [--at X,Y] [--parent ID]
                                    add a raster layer on top holding PNG at canvas X,Y
                                    (in folder ID; default: the root folder)
  cspf add-folder FILE OUT [--name NAME] [--parent ID]
                                    add an empty folder on top
  cspf set FILE ID OUT [--name NAME] [--opacity 0-100] [--blend MODE] [--visible yes|no]
                       [--clip yes|no]
                                    change a layer's attributes (MODE: a LayerComposite number)
  cspf remove-layer FILE ID OUT     remove a layer (a folder with its contents)
  cspf extract FILE OUT_DIR         stored layer pixels, masks and thumbnails as PNG
  cspf preview FILE OUT.png         the preview CSP saved in the file
  cspf render FILE OUT.png [--scale S]
                                    composite the layers (default: the preview's size)
  cspf compare FILE...              render at the preview's size and report PSNR (dB)
  cspf anim FILE                    timelines, tracks, keys, labels and onion skin as JSON
  cspf frames FILE OUT_DIR [--timeline N] [--from F] [--to F] [--camera yes|no] [--area frame|canvas]
                                    render animation frames as NNNN.png (N: timeline number from 1,
                                    default the current one; F: frame numbers as CSP shows them,
                                    default the playback range; area: the reference frame, as
                                    CSP's image-sequence export, or the whole canvas)
  cspf anim-set FILE OUT [--timeline N] [--fps F] [--range FIRST-LAST] [--cels FOLDER_ID F:NAME,...]
                                    change a timeline's frame rate or playback range (frames as
                                    CSP shows them), or replace an animation folder's cel keys
  cspf corpus ROOT OUT.jsonl [--seconds N] [--no-reencode] [--shard K/N] [--list PATHS.txt] [--render]
                                    check every .clip / .cmc under ROOT (resumable);
                                    --render compares renders with the stored previews instead
  cspf asset COMMAND ...            sub tools, auto actions, colour / gradient sets, materials
                                    (`cspf asset help`)
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
        "version" => version(rest),
        "check" | "verify" => check(rest),
        "roundtrip" => two(rest).and_then(|(file, out)| roundtrip(&file, &out)),
        "new" => new_file(rest),
        "new-work" => new_work(rest),
        "add-layer" => add_layer(rest),
        "add-folder" => add_folder(rest),
        "set" => set_attributes(rest),
        "remove-layer" => remove_layer(rest),
        "extract" => two(rest).and_then(|(file, out)| extract(&file, &out)),
        "preview" => two(rest).and_then(|(file, out)| preview(&file, &out)),
        "corpus" => corpus(rest),
        "render" => render(rest),
        "compare" => compare(rest),
        "anim" => one_file(rest).and_then(anim),
        "frames" => frames(rest),
        "anim-set" => anim_set(rest),
        "asset" => asset::run(rest),
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

fn version(args: &[String]) -> Result<bool> {
    if args.is_empty() {
        return Err(bad("expected FILE..."));
    }
    for file in args {
        let data = std::fs::read(file)?;
        let info = if csp_format::container::Container::sniff(&data) {
            let container = csp_format::container::Container::parse(data)?;
            let image = container.sqlite()?;
            csp_format::VersionInfo::read(&csp_format::database::open_bytes(image)?, Some(image))?
        } else {
            csp_format::VersionInfo::read(&csp_format::database::open_bytes(&data)?, Some(&data))?
        };
        print_json(&json!({"file": file, "version": info}));
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

fn roundtrip(path: &Path, out: &Path) -> Result<bool> {
    let original = std::fs::read(path)?;
    let file = ClipFile::from_bytes(original.clone())?;
    let saved = file.to_bytes()?;
    std::fs::write(out, &saved)?;
    let same = saved == original;
    print_json(&json!({"file": path, "out": out, "bytes": saved.len(), "identical": same}));
    Ok(same)
}

/// Options `--name value` and the remaining positional arguments.
fn options<'a>(args: &'a [String], names: &[&str]) -> (Vec<(&'a str, &'a str)>, Vec<&'a str>) {
    let (mut found, mut positional) = (Vec::new(), Vec::new());
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if names.contains(&arg.as_str()) {
            found.push((arg.as_str(), iter.next().map(String::as_str).unwrap_or("")));
        } else {
            positional.push(arg.as_str());
        }
    }
    (found, positional)
}

fn bad(message: impl std::fmt::Display) -> csp_format::Error {
    csp_format::Error::format(format!("{message}\n\n{USAGE}"))
}

fn read_png(path: &str) -> Result<csp_format::RgbaImage> {
    csp_format::image::decode_png(&std::fs::read(path)?)
}

fn new_file(args: &[String]) -> Result<bool> {
    let (found, positional) = options(args, &["--size", "--dpi", "--paper", "--image"]);
    let [out] = positional.as_slice() else { return Err(bad("expected OUT.clip")) };
    let mut canvas = csp_format::NewCanvas::default();
    let mut image = None;
    for (name, value) in found {
        match name {
            "--size" => {
                let (w, h) = value.split_once('x').ok_or_else(|| bad("--size is WxH"))?;
                canvas.width = w.parse().map_err(|_| bad("--size is WxH"))?;
                canvas.height = h.parse().map_err(|_| bad("--size is WxH"))?;
            }
            "--dpi" => canvas.resolution = value.parse().map_err(|_| bad("--dpi is a number"))?,
            "--paper" if value == "none" => canvas.paper = None,
            "--paper" => {
                let rgb =
                    u32::from_str_radix(value.trim_start_matches('#'), 16).map_err(|_| bad("--paper is RRGGBB"))?;
                canvas.paper = Some([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]);
            }
            _ => image = Some(read_png(value)?),
        }
    }
    let mut file = ClipFile::create(&canvas)?;
    if let Some(image) = image {
        let raster = file.canvas.row.int("CanvasCurrentLayer");
        file.set_layer_pixels(raster, &image, (0, 0))?;
        file.update_preview()?;
    }
    file.save(out)?;
    print_json(
        &json!({"file": out, "width": canvas.width, "height": canvas.height, "layers": file.layers.layers.len()}),
    );
    Ok(true)
}

fn new_work(args: &[String]) -> Result<bool> {
    use csp_format::work::{Binding, NewWork, StartSide};
    let (found, positional) =
        options(args, &["--trim", "--bleed", "--dpi", "--pages", "--bind", "--start", "--title", "--author"]);
    let mut work = NewWork::default();
    let (flags, positional): (Vec<&str>, Vec<&str>) = positional.into_iter().partition(|arg| *arg == "--spreads");
    work.spreads = !flags.is_empty();
    let [dir, name] = positional.as_slice() else { return Err(bad("expected DIR NAME")) };
    let number = |value: &str, what: &str| value.parse::<f64>().map_err(|_| bad(format!("{what} is a number")));
    for (option, value) in found {
        match option {
            "--trim" => {
                let (w, h) = value.split_once('x').ok_or_else(|| bad("--trim is WxH (mm)"))?;
                (work.trim_width, work.trim_height) = (number(w, "--trim")?, number(h, "--trim")?);
            }
            "--bleed" => work.bleed = number(value, "--bleed")?,
            "--dpi" => work.resolution = number(value, "--dpi")?,
            "--pages" => work.pages = value.parse().map_err(|_| bad("--pages is a count"))?,
            "--bind" => {
                work.binding = match value {
                    "right" => Binding::Right,
                    "left" => Binding::Left,
                    _ => return Err(bad("--bind is right or left")),
                }
            }
            "--start" => {
                work.start = match value {
                    "left" => StartSide::Left,
                    "right" => StartSide::Right,
                    _ => return Err(bad("--start is left or right")),
                }
            }
            "--title" => work.title = value.to_owned(),
            _ => work.author = value.to_owned(),
        }
    }
    let cmc = work.write(Path::new(dir), name)?;
    let slots: Vec<Value> = work
        .layout()
        .iter()
        .map(|slot| json!({"position": slot.position, "spread": slot.spread, "left": slot.left}))
        .collect();
    print_json(&json!({"file": cmc, "pages": slots}));
    Ok(true)
}

fn add_layer(args: &[String]) -> Result<bool> {
    let (found, positional) = options(args, &["--name", "--at", "--parent"]);
    let [file, png, out] = positional.as_slice() else { return Err(bad("expected FILE PNG OUT")) };
    let mut clip = ClipFile::open(file)?;
    let mut name = "レイヤー".to_owned();
    let mut at = (0, 0);
    let mut parent = clip.canvas.root_folder;
    for (option, value) in found {
        match option {
            "--name" => name = value.to_owned(),
            "--parent" => parent = value.parse().map_err(|_| bad("--parent is a folder id (see inspect)"))?,
            _ => {
                let (x, y) = value.split_once(',').ok_or_else(|| bad("--at is X,Y"))?;
                at = (x.parse().map_err(|_| bad("--at is X,Y"))?, y.parse().map_err(|_| bad("--at is X,Y"))?);
            }
        }
    }
    let id = clip.add_layer(parent, NewLayer::Raster, &name)?;
    clip.set_layer_pixels(id, &read_png(png)?, at)?;
    finish(clip, out, json!({"file": out, "layer": id}))
}

fn add_folder(args: &[String]) -> Result<bool> {
    let (found, positional) = options(args, &["--name", "--parent"]);
    let [file, out] = positional.as_slice() else { return Err(bad("expected FILE OUT")) };
    let mut clip = ClipFile::open(file)?;
    let mut name = "フォルダー".to_owned();
    let mut parent = clip.canvas.root_folder;
    for (option, value) in found {
        match option {
            "--name" => name = value.to_owned(),
            _ => parent = value.parse().map_err(|_| bad("--parent is a folder id (see inspect)"))?,
        }
    }
    let id = clip.add_layer(parent, NewLayer::Folder, &name)?;
    finish(clip, out, json!({"file": out, "folder": id}))
}

fn set_attributes(args: &[String]) -> Result<bool> {
    let (found, positional) = options(args, &["--name", "--opacity", "--blend", "--visible", "--clip"]);
    let [file, id, out] = positional.as_slice() else { return Err(bad("expected FILE ID OUT")) };
    let id: i64 = id.parse().map_err(|_| bad("ID is a layer id (see inspect)"))?;
    let yes = |value: &str| match value {
        "yes" | "on" | "true" | "1" => Ok(true),
        "no" | "off" | "false" | "0" => Ok(false),
        _ => Err(bad("expected yes or no")),
    };
    let mut clip = ClipFile::open(file)?;
    for (option, value) in found {
        match option {
            "--name" => clip.set_layer_name(id, value)?,
            "--opacity" => {
                let percent: f64 = value.parse().map_err(|_| bad("--opacity is 0-100"))?;
                clip.set_layer_opacity(id, percent / 100.0)?;
            }
            "--blend" => {
                let mode = value.parse().ok().and_then(BlendMode::from_code);
                clip.set_layer_blend(id, mode.ok_or_else(|| bad("--blend is a LayerComposite number"))?)?;
            }
            "--visible" => clip.set_layer_visible(id, yes(value)?)?,
            _ => clip.set_layer_clipping(id, yes(value)?)?,
        }
    }
    finish(clip, out, json!({"file": out, "layer": id}))
}

/// Refresh the preview, give the file a new id and save it.
fn finish(mut clip: ClipFile, out: &str, report: Value) -> Result<bool> {
    clip.update_preview()?;
    clip.renew_file_id();
    clip.save(out)?;
    print_json(&report);
    Ok(true)
}

fn remove_layer(args: &[String]) -> Result<bool> {
    let [file, id, out] = args else { return Err(bad("expected FILE ID OUT")) };
    let id: i64 = id.parse().map_err(|_| bad("ID is a layer id (see inspect)"))?;
    let mut clip = ClipFile::open(file)?;
    clip.remove_layer(id)?;
    finish(clip, out, json!({"file": out, "removed": id}))
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
        if let Some(thumbnail) = file.layer_thumbnail(layer)? {
            std::fs::write(out.join(format!("layer{:04}_thumbnail.png", layer.main_id)), thumbnail.to_png()?)?;
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

fn render(args: &[String]) -> Result<bool> {
    let mut positional = Vec::new();
    let mut scale = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--scale" => scale = iter.next().and_then(|s| s.parse::<f64>().ok()),
            _ => positional.push(arg.clone()),
        }
    }
    let (file, out) = two(&positional)?;
    let clip = ClipFile::open(&file)?;
    let (image, report) = match scale {
        Some(scale) => csp_format::render::render(&clip, csp_format::render::RenderOptions { scale, size: None })?,
        None => match csp_format::render::render_like_preview(&clip)? {
            Some((image, report, _)) => (image, report),
            None => csp_format::render::render(&clip, Default::default())?,
        },
    };
    std::fs::write(out, image.to_png()?)?;
    print_json(&serde_json::to_value(&report).expect("JSON"));
    Ok(true)
}

fn compare(args: &[String]) -> Result<bool> {
    for path in args {
        let clip = ClipFile::open(path)?;
        let result = match csp_format::render::render_like_preview(&clip)? {
            Some((image, report, preview)) => {
                json!({"file": path, "psnr": csp_format::render::psnr(&image, &preview), "report": report})
            }
            None => json!({"file": path, "psnr": null}),
        };
        println!("{}", serde_json::to_string(&result).expect("JSON"));
    }
    Ok(true)
}

fn anim(path: PathBuf) -> Result<bool> {
    let file = ClipFile::open(&path)?;
    match file.animation()? {
        Some(animation) => print_json(&serde_json::to_value(&animation).expect("JSON")),
        None => print_json(&json!(null)),
    }
    Ok(true)
}

/// The timeline index (0-based) for `--timeline N` (from 1), or the current one.
fn timeline_index(animation: &csp_format::animation::Animation, value: Option<&str>) -> Result<usize> {
    match value {
        None => Ok(animation.current),
        Some(text) => {
            let n: usize = text.parse().map_err(|_| bad("--timeline is a number from 1"))?;
            if n == 0 || n > animation.timelines.len() {
                return Err(bad(format!("--timeline: the file has {} timelines", animation.timelines.len())));
            }
            Ok(n - 1)
        }
    }
}

fn frames(args: &[String]) -> Result<bool> {
    let (found, positional) = options(args, &["--timeline", "--from", "--to", "--camera", "--area"]);
    let [file, out] = positional.as_slice() else { return Err(bad("expected FILE OUT_DIR")) };
    let get = |name: &str| found.iter().find(|(n, _)| *n == name).map(|(_, v)| *v);
    let clip = ClipFile::open(file)?;
    let animation = clip.animation()?.ok_or_else(|| bad("the file has no timeline"))?;
    let index = timeline_index(&animation, get("--timeline"))?;
    let timeline = &animation.timelines[index];
    let number = |name: &str, default: i64| -> Result<i64> {
        get(name).map_or(Ok(default), |v| v.parse().map_err(|_| bad(format!("{name} is a frame number"))))
    };
    let first = number("--from", timeline.start_frame as i64 + 1)?;
    let last = number("--to", timeline.end_frame as i64)?;
    let camera = !matches!(get("--camera"), Some("no" | "off" | "false" | "0"));
    let canvas_area = get("--area") == Some("canvas");
    std::fs::create_dir_all(out)?;
    let mut written = 0;
    let mut unknown = std::collections::BTreeSet::new();
    for (n, frame) in (first..=last).enumerate() {
        let mut state = animation.frame_state(&clip, index, (frame - 1) as f64)?;
        if !camera {
            state.cameras.clear();
        }
        unknown.extend(state.unknown_interp.iter().cloned());
        let (image, _) = csp_format::render::render_frame(&clip, &state, Default::default())?;
        let image = if canvas_area {
            image
        } else {
            let area = animation.frame;
            let mut cropped = csp_format::RgbaImage::new(area.width.round() as u32, area.height.round() as u32);
            cropped.paste(&image, -(area.x.round() as i64), -(area.y.round() as i64));
            cropped
        };
        std::fs::write(Path::new(out).join(format!("{n:04}.png")), image.to_png()?)?;
        written += 1;
    }
    print_json(&json!({
        "timeline": timeline.name, "frames": written, "first": first, "last": last,
        "camera": camera, "unknown_interpolation": unknown,
    }));
    Ok(true)
}

fn anim_set(args: &[String]) -> Result<bool> {
    let (found, positional) = options(args, &["--timeline", "--fps", "--range", "--cels"]);
    let get = |name: &str| found.iter().find(|(n, _)| *n == name).map(|(_, v)| *v);
    let mut positional = positional.into_iter();
    let (Some(file), Some(out)) = (positional.next(), positional.next()) else {
        return Err(bad("expected FILE OUT"));
    };
    let mut clip = ClipFile::open(file)?;
    let animation = clip.animation()?.ok_or_else(|| bad("the file has no timeline"))?;
    let index = timeline_index(&animation, get("--timeline"))?;
    let timeline_id = animation.timelines[index].main_id;
    if let Some(value) = get("--fps") {
        clip.set_frame_rate(timeline_id, value.parse().map_err(|_| bad("--fps is a number"))?)?;
    }
    if let Some(value) = get("--range") {
        let (a, b) = value.split_once('-').ok_or_else(|| bad("--range is FIRST-LAST"))?;
        let first: i64 = a.parse().map_err(|_| bad("--range is FIRST-LAST"))?;
        let last: i64 = b.parse().map_err(|_| bad("--range is FIRST-LAST"))?;
        clip.set_frame_range(timeline_id, first - 1, last)?;
    }
    if let Some(folder) = get("--cels") {
        let folder: i64 = folder.parse().map_err(|_| bad("--cels FOLDER_ID F:NAME,..."))?;
        let list = positional.next().ok_or_else(|| bad("--cels FOLDER_ID F:NAME,..."))?;
        let mut keys = Vec::new();
        for item in list.split(',') {
            let (frame, name) = item.split_once(':').ok_or_else(|| bad("cel keys are F:NAME"))?;
            let frame: i64 = frame.parse().map_err(|_| bad("cel keys are F:NAME"))?;
            keys.push((frame - 1, name.to_owned()));
        }
        clip.set_cel_keys(timeline_id, folder, &keys)?;
    }
    clip.reload()?;
    finish(clip, out, json!({"file": out, "timeline": timeline_id}))
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
    let mut render_mode = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--seconds" => seconds = iter.next().and_then(|s| s.parse().ok()).unwrap_or(0.0),
            "--no-reencode" => options.reencode_pixels = false,
            "--render" => render_mode = true,
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
            Ok(_) if render_mode && is_cmc(path) => continue,
            Ok(data) if render_mode => {
                let size = data.len();
                let result = ClipFile::from_bytes(data).and_then(|clip| {
                    Ok(match csp_format::render::render_like_preview(&clip)? {
                        Some((image, report, preview)) => {
                            json!({"psnr": csp_format::render::psnr(&image, &preview), "render": report})
                        }
                        None => json!({"psnr": null}),
                    })
                });
                match result {
                    Ok(value) => (size, true, value),
                    Err(error) => (size, false, json!({"errors": [error.to_string()]})),
                }
            }
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
