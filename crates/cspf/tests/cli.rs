//! The commands on synthetic files made by `cspf new` (R15, V5).

use std::path::Path;
use std::process::{Command, Output};

fn cspf(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cspf")).args(args).output().expect("run cspf")
}

fn ok(args: &[&str]) -> String {
    let output = cspf(args);
    assert!(output.status.success(), "cspf {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).expect("UTF-8")
}

fn path(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

/// A 64 × 48 PNG with a red square.
fn red_square(dir: &Path) -> String {
    let mut image = csp_format::RgbaImage::new(64, 48);
    for y in 8..24 {
        for x in 8..24 {
            image.set_pixel(x, y, [220, 20, 30, 255]);
        }
    }
    let png = path(dir, "square.png");
    std::fs::write(&png, image.to_png().unwrap()).unwrap();
    png
}

#[test]
fn new_inspect_check_render_and_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let png = red_square(dir.path());
    let clip = path(dir.path(), "new.clip");
    ok(&["new", &clip, "--size", "64x48", "--dpi", "300", "--image", &png]);

    let inspect: serde_json::Value = serde_json::from_str(&ok(&["inspect", &clip])).unwrap();
    assert_eq!(inspect["layers"].as_array().unwrap().len(), 3);
    let kinds: Vec<&str> = inspect["layers"].as_array().unwrap().iter().map(|l| l["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["root", "paper", "raster"]);
    assert!(ok(&["layers", &clip]).contains("[raster]"));

    for command in ["check", "verify"] {
        let report: serde_json::Value = serde_json::from_str(&ok(&[command, &clip])).unwrap();
        assert_eq!(report["ok"], true, "{report}");
    }
    let schema: serde_json::Value = serde_json::from_str(&ok(&["schema", &clip])).unwrap();
    assert!(schema.to_string().contains("Offscreen"));

    let rendered = path(dir.path(), "render.png");
    ok(&["render", &clip, &rendered]);
    let image = csp_format::image::decode_png(&std::fs::read(&rendered).unwrap()).unwrap();
    assert_eq!((image.width, image.height), (64, 48));
    assert_eq!(image.pixel(10, 10), [220, 20, 30, 255]);
    assert_eq!(image.pixel(40, 40), [255, 255, 255, 255]);

    let preview = path(dir.path(), "preview.png");
    ok(&["preview", &clip, &preview]);
    let compare: serde_json::Value = serde_json::from_str(ok(&["compare", &clip]).trim()).unwrap();
    assert!(compare["psnr"].as_f64().unwrap() > 60.0, "{compare}");

    let extracted = dir.path().join("layers");
    ok(&["extract", &clip, &extracted.to_string_lossy()]);
    assert!(std::fs::read_dir(&extracted).unwrap().count() >= 1);

    let copy = path(dir.path(), "copy.clip");
    let roundtrip: serde_json::Value = serde_json::from_str(&ok(&["roundtrip", &clip, &copy])).unwrap();
    assert_eq!(roundtrip["identical"], true);
    assert_eq!(std::fs::read(&clip).unwrap(), std::fs::read(&copy).unwrap());

    let version: serde_json::Value = serde_json::from_str(&ok(&["version", &clip])).unwrap();
    assert_eq!(version["version"]["saved_by"]["csp"], csp_format::version::GENERATIONS[0].csp);
    assert_eq!(version["version"]["resaved"], false);
}

#[test]
fn add_and_remove_layers() {
    let dir = tempfile::tempdir().unwrap();
    let png = red_square(dir.path());
    let clip = path(dir.path(), "new.clip");
    ok(&["new", &clip, "--size", "64x48", "--paper", "none"]);
    let added = path(dir.path(), "added.clip");
    let result: serde_json::Value =
        serde_json::from_str(&ok(&["add-layer", &clip, &png, &added, "--name", "top", "--at", "4,2"])).unwrap();
    let id = result["layer"].as_i64().unwrap().to_string();
    let report: serde_json::Value = serde_json::from_str(&ok(&["check", &added])).unwrap();
    assert_eq!(report["ok"], true, "{report}");
    assert!(ok(&["layers", &added]).contains("top [raster]"));
    let rendered = path(dir.path(), "added.png");
    ok(&["render", &added, &rendered]);
    let image = csp_format::image::decode_png(&std::fs::read(&rendered).unwrap()).unwrap();
    assert_eq!(image.pixel(12, 10), [220, 20, 30, 255]);

    let removed = path(dir.path(), "removed.clip");
    ok(&["remove-layer", &added, &id, &removed]);
    let report: serde_json::Value = serde_json::from_str(&ok(&["check", &removed])).unwrap();
    assert_eq!(report["ok"], true, "{report}");
    assert!(!ok(&["layers", &removed]).contains("top [raster]"));
}

#[test]
fn corpus_checks_a_folder_and_resumes() {
    let dir = tempfile::tempdir().unwrap();
    let files = dir.path().join("files");
    std::fs::create_dir(&files).unwrap();
    for name in ["a.clip", "b.clip"] {
        ok(&["new", &path(&files, name), "--size", "32x32"]);
    }
    let out = path(dir.path(), "corpus.jsonl");
    ok(&["corpus", &files.to_string_lossy(), &out]);
    let lines = std::fs::read_to_string(&out).unwrap();
    assert_eq!(lines.lines().count(), 2);
    assert!(lines.lines().all(|line| line.contains("\"ok\":true")), "{lines}");
    // A second run finds both done.
    let again = cspf(&["corpus", &files.to_string_lossy(), &out]);
    assert!(String::from_utf8_lossy(&again.stderr).contains("2 already done"));
    assert_eq!(std::fs::read_to_string(&out).unwrap().lines().count(), 2);
}

#[test]
fn bad_arguments_fail_cleanly() {
    assert_eq!(cspf(&["nonsense"]).status.code(), Some(2));
    assert_eq!(cspf(&["inspect"]).status.code(), Some(1));
    assert_eq!(cspf(&["new", "x.clip", "--size", "big"]).status.code(), Some(1));
}

#[test]
fn folders_and_attributes() {
    let dir = tempfile::tempdir().unwrap();
    let png = red_square(dir.path());
    let clip = path(dir.path(), "new.clip");
    ok(&["new", &clip, "--size", "64x48"]);
    let with_folder = path(dir.path(), "folder.clip");
    let added: serde_json::Value =
        serde_json::from_str(&ok(&["add-folder", &clip, &with_folder, "--name", "F"])).unwrap();
    let folder = added["folder"].as_i64().unwrap().to_string();
    let inside = path(dir.path(), "inside.clip");
    let added: serde_json::Value =
        serde_json::from_str(&ok(&["add-layer", &with_folder, &png, &inside, "--parent", &folder])).unwrap();
    let layer = added["layer"].as_i64().unwrap().to_string();
    let changed = path(dir.path(), "changed.clip");
    ok(&["set", &inside, &layer, &changed, "--opacity", "50", "--blend", "2", "--visible", "no", "--name", "L"]);
    let changed2 = path(dir.path(), "changed2.clip");
    ok(&["set", &changed, &folder, &changed2, "--clip", "yes"]);

    let report: serde_json::Value = serde_json::from_str(&ok(&["check", &changed2])).unwrap();
    assert_eq!(report["ok"], true, "{report}");
    let layers = ok(&["layers", &changed2]);
    assert!(layers.contains("F [folder] clip"), "{layers}");
    assert!(layers.contains("    L [raster] hidden  opacity=128 blend=2"), "{layers}");
    assert!(!cspf(&["set", &changed2, &layer, &changed, "--blend", "999"]).status.success());
}

#[test]
fn animation_commands_on_a_file_without_timelines() {
    let dir = tempfile::tempdir().unwrap();
    let clip = path(dir.path(), "still.clip");
    ok(&["new", &clip, "--size", "16x16"]);
    assert_eq!(ok(&["anim", &clip]).trim(), "null");
    let frames = cspf(&["frames", &clip, &path(dir.path(), "frames")]);
    assert!(!frames.status.success());
    assert!(String::from_utf8_lossy(&frames.stderr).contains("no timeline"));
    let set = cspf(&["anim-set", &clip, &path(dir.path(), "out.clip"), "--fps", "12"]);
    assert!(!set.status.success());
}

/// Fails without writing: the input's bytes stay the same.
fn refused(dir: &Path, args: &[&str], input: &str) {
    let before = std::fs::read(dir.join(input)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cspf")).args(args).current_dir(dir).output().expect("run cspf");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "cspf {args:?} wrote over {input}");
    assert!(stderr.contains("is the input"), "cspf {args:?}: {stderr}");
    assert_eq!(std::fs::read(dir.join(input)).unwrap(), before, "cspf {args:?} changed {input}");
}

#[test]
fn outputs_never_replace_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let png = red_square(root);
    let clip = path(root, "new.clip");
    ok(&["new", &clip, "--size", "64x48"]);
    std::fs::create_dir(root.join("sub")).unwrap();
    std::fs::hard_link(root.join("new.clip"), root.join("hard.clip")).unwrap();
    // The same file by an absolute path, a relative one, a roundabout one and a hard link.
    let mut names = vec![clip.clone(), "new.clip".to_owned(), "./sub/../new.clip".to_owned(), "hard.clip".to_owned()];
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("new.clip"), root.join("sub/link.clip")).unwrap();
        names.push("sub/link.clip".to_owned());
    }
    for out in &names {
        let out = out.as_str();
        for args in [
            vec!["roundtrip", "new.clip", out],
            vec!["add-layer", "new.clip", "square.png", out],
            vec!["add-folder", "new.clip", out],
            vec!["set", "new.clip", "3", out, "--opacity", "50"],
            vec!["remove-layer", "new.clip", "3", out],
            vec!["preview", "new.clip", out],
            vec!["render", "new.clip", out],
        ] {
            refused(root, &args, "new.clip");
        }
    }
    refused(root, &["add-layer", "new.clip", "square.png", &png], "square.png");
    refused(root, &["new", "square.png", "--image", "square.png"], "square.png");
    refused(root, &["corpus", ".", "new.clip"], "new.clip");

    // `anim-set` saves through the same check as `set` (`finish`).

    // A different output still works.
    ok(&["roundtrip", &clip, &path(root, "copy.clip")]);
}
