//! `cspf asset …` on synthetic files (`csp_assets::testing`, A13, W2).

use std::path::Path;
use std::process::{Command, Output};

use csp_assets::testing;

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

fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).expect("JSON")
}

#[test]
fn sub_tools_list_show_edit_and_extract() {
    let dir = tempfile::tempdir().unwrap();
    let db = path(dir.path(), "EditImageTool.todb");
    std::fs::write(&db, testing::tool_db()).unwrap();

    let tree = ok(&["asset", "subtools", &db]);
    assert!(tree.contains("    G-ペン  <brush>  [settings 1 / initial 101]"), "{tree}");
    let settings = json(&ok(&["asset", "settings", &db, "ペン/G-ペン"]));
    assert_eq!(settings["settings"]["BrushSize"], 5.0);
    assert_eq!(settings["settings"]["BrushSizeEffector"]["minimum"][0], 20);

    let edited = path(dir.path(), "edited.todb");
    ok(&["asset", "sut-set", &db, "G-ペン", &edited, "--name", "Gペン改", "BrushSize=8.5", "Opacity=50"]);
    let settings = json(&ok(&["asset", "settings", &edited, "Gペン改"]));
    assert_eq!(settings["settings"]["BrushSize"], 8.5);
    assert_eq!(settings["settings"]["Opacity"], 50);
    assert!(!cspf(&["asset", "settings", &edited, "G-ペン"]).status.success());

    // The brush tip material folder, as CSP keeps it.
    let folder = dir.path().join("Material/56/16").join(testing::TIP_UUID);
    for (name, data) in [
        ("catalog.xml", b"<archive/>".as_slice()),
        ("catalog.zip", b"z"),
        ("info.zip", b"i"),
        ("data/material_0.layer", b"d"),
        ("thumbnail/thumbnail.png", b"\x89PNG"),
        ("icedata/layerData.xml", b"<infolist/>"),
    ] {
        let file = folder.join(name);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, data).unwrap();
    }
    let sut = path(dir.path(), "G.sut");
    let materials = path(dir.path(), "Material");
    ok(&["asset", "sut-extract", &db, "G-ペン", &sut, "--materials", &materials]);
    let inspect = json(&ok(&["asset", "inspect", &sut]));
    assert_eq!(inspect["format"]["pattern"], ".sut");
    assert_eq!(inspect["content"]["kind"], "subTool");
    assert_eq!(inspect["content"]["subTools"], 1);
    let files: Vec<&str> = inspect["content"]["materials"][0]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        files,
        ["catalog.zip", "info.zip", "data/material_0.layer", "thumbnail/thumbnail.png", "icedata/layerData.xml"]
    );

    // Without the material folder the sub tool is written and the miss reported.
    let bare = path(dir.path(), "bare.sut");
    let output = cspf(&["asset", "sut-extract", &db, "G-ペン", &bare]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("material not found"));
    assert!(Path::new(&bare).is_file());
}

#[test]
fn colour_sets_actions_gradients_and_corpus() {
    let dir = tempfile::tempdir().unwrap();
    let cls = path(dir.path(), "肌.cls");
    ok(&["asset", "colorset-new", &cls, "肌", "F2D3C4", "#80402080=影", "00000000"]);
    let set = json(&ok(&["asset", "colorset", &cls]));
    assert_eq!(set["name"], "肌");
    assert_eq!(set["colors"][1]["rgba"], serde_json::json!([128, 64, 32, 128]));
    assert_eq!(set["colors"][1]["name"], "影");
    assert!(ok(&["asset", "kind", &cls]).starts_with("colorSet\t.cls"));

    let las = path(dir.path(), "test.las");
    std::fs::write(&las, testing::action_set().to_bytes()).unwrap();
    let text = ok(&["asset", "actions", &las]);
    assert!(text.contains("下描きレイヤーの作成") && text.contains("[x] layeractionrasterdetailnew"), "{text}");
    let dumped = path(dir.path(), "test.json");
    std::fs::write(&dumped, ok(&["asset", "dump", &las])).unwrap();
    let again = path(dir.path(), "again.las");
    ok(&["asset", "undump", &dumped, &again]);
    assert_eq!(std::fs::read(&las).unwrap(), std::fs::read(&again).unwrap());

    let sgs = path(dir.path(), "default.sgs");
    std::fs::write(&sgs, testing::gradient_set().to_document().to_bytes()).unwrap();
    let gradients = json(&ok(&["asset", "gradients", &sgs]));
    assert_eq!(gradients["groups"][0]["gradients"][0]["name"], "黒→透明");

    std::fs::write(path(dir.path(), "EditImageTool.todb"), testing::tool_db()).unwrap();
    let report = path(dir.path(), "report.jsonl");
    let summary = ok(&["asset", "corpus", dir.path().to_str().unwrap(), &report]);
    assert!(summary.ends_with("6 ok, 0 failed\n"), "{summary}");
}

#[test]
fn shortcuts_with_tool_names() {
    let dir = tempfile::tempdir().unwrap();
    let spws = path(dir.path(), "workspace.spws");
    std::fs::write(&spws, testing::workspace_shortcuts()).unwrap();
    let todb = path(dir.path(), "EditImageTool.todb");
    std::fs::write(&todb, testing::tool_db()).unwrap();
    let text = ok(&["asset", "shortcuts", &spws, "--tools", &todb]);
    assert_eq!(text, "applicationpreference\tCtrl+K\nredo\tCtrl+Shift+Z\ntool:ペン\tP\n");
    let unnamed = ok(&["asset", "shortcuts", &spws]);
    assert!(unnamed.ends_with(&format!("tool:{}\tP\n", "02".repeat(16))), "{unnamed}");
}

#[test]
fn modifier_keys_with_tool_names() {
    let dir = tempfile::tempdir().unwrap();
    let tomd = path(dir.path(), "DefaultToolModifyKey.tomd");
    std::fs::write(&tomd, testing::modifier_keys()).unwrap();
    let todb = path(dir.path(), "EditImageTool.todb");
    std::fs::write(&todb, testing::tool_db()).unwrap();
    let text = ok(&["asset", "modifier-keys", &tomd, "--tools", &todb]);
    assert_eq!(text, "ops 0 0 0\n  Space\ttool:ペン\nops 1 10 0  (G-ペン)\n  Shift\toperation 100\n");
}
