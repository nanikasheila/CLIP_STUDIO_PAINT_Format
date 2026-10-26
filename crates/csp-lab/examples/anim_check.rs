//! Check the animation reader on files: every track mixer is parsed and
//! written back byte for byte (document and compressed chunk), and every
//! TrackValueMap is re-encoded unchanged.
//!
//! cargo run --release --example anim_check -- FILE...

use csp_format::ClipFile;
use csp_format::animation::{parse_value_map, value_map_bytes};
use csp_format::cmt::{Document, pack, unpack};
use csp_format::database::{select_all, value_as_text};

fn main() -> csp_format::Result<()> {
    let mut failures = 0;
    for path in std::env::args().skip(1) {
        let file = ClipFile::open(&path)?;
        let Some(animation) = file.animation()? else {
            println!("{path}: no timeline");
            continue;
        };
        let (mut mixers, mut exact, mut packed, mut maps) = (0, 0, 0, 0);
        for row in select_all(file.database(), "Track")? {
            for column in ["TrackActionMixer", "TrackActionMixer2"] {
                let Some(id) = row.get(column).and_then(value_as_text) else { continue };
                let Some(payload) = file.external(&id) else { continue };
                let bytes = unpack(payload)?;
                mixers += 1;
                exact += usize::from(Document::parse(&bytes)?.to_bytes()? == bytes);
                packed += usize::from(pack(&bytes)? == payload);
            }
            if let Some(map) = row.blob("TrackValueMap") {
                maps += usize::from(value_map_bytes(&parse_value_map(map)?) == map);
            }
        }
        let tracks: usize = animation.timelines.iter().map(|t| t.tracks.len()).sum();
        println!(
            "{path}: {} timelines, {tracks} tracks, mixers {exact}/{mixers} exact, {packed}/{mixers} recompressed, value maps {maps}/{tracks}",
            animation.timelines.len()
        );
        failures += (mixers - exact) + (mixers - packed) + (tracks - maps);
    }
    if failures > 0 {
        std::process::exit(1);
    }
    Ok(())
}
