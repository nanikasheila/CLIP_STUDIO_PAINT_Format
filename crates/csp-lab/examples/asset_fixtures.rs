//! Write the synthetic files of `csp_assets::testing` (no CSP data) to a
//! folder, for checks that need files (the W5 comparison in CI).
//!
//!     asset_fixtures OUT_DIR

use csp_assets::colorset::{Color, ColorSet};
use csp_assets::testing;

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("usage: asset_fixtures OUT_DIR"));
    std::fs::create_dir_all(&dir).expect("create the folder");
    let colors =
        ColorSet::new("肌", vec![Color::new([242, 211, 196, 255], None), Color::new([128, 64, 32, 128], Some("影"))]);
    let files: [(&str, Vec<u8>); 4] = [
        ("EditImageTool.todb", testing::tool_db()),
        ("test.las", testing::action_set().to_bytes()),
        ("default.sgs", testing::gradient_set().to_document().to_bytes()),
        ("skin.cls", colors.to_bytes()),
    ];
    for (name, data) in files {
        std::fs::write(dir.join(name), data).expect("write");
    }
}
