//! Rebuild every stored offscreen payload from its decoded blocks and
//! compare with CSP's bytes (diagnostics for the block writer).
use csp_format::ClipFile;
use csp_format::database::{select_all, value_as_text};
use csp_format::offscreen::BlockData;

fn main() -> csp_format::Result<()> {
    for path in std::env::args().skip(1) {
        let file = ClipFile::open(&path)?;
        let (mut same, mut different) = (0, 0);
        for row in select_all(file.database(), "Offscreen")? {
            let Some(id) = row.get("BlockData").and_then(value_as_text) else { continue };
            let Some(payload) = file.external(&id) else { continue };
            let data = BlockData::parse(payload)?;
            let raw: Vec<Option<Vec<u8>>> =
                data.blocks.iter().map(|b| b.inflate()).collect::<csp_format::Result<_>>()?;
            let size = data.blocks.first().map(|b| b.raw_size as usize).unwrap_or(0);
            let rebuilt = BlockData::from_raw_blocks(&raw, size)?;
            if rebuilt.to_bytes() == payload {
                same += 1;
            } else {
                different += 1;
                if different <= 2 {
                    for (a, b) in data.tables.iter().zip(&rebuilt.tables) {
                        if a != b {
                            let diff: Vec<_> = a
                                .values
                                .iter()
                                .zip(&b.values)
                                .enumerate()
                                .filter(|(_, (x, y))| x != y)
                                .take(5)
                                .collect();
                            println!("  table {} header {} vs {}: {:?}", a.name, a.header, b.header, diff);
                        }
                    }
                    for (a, b) in data.blocks.iter().zip(&rebuilt.blocks) {
                        if a != b {
                            println!(
                                "  block {} differs: w {} h {} vs {} {}, raw {} vs {}",
                                a.index, a.width, a.height, b.width, b.height, a.raw_size, b.raw_size
                            );
                            break;
                        }
                    }
                }
            }
        }
        println!("{path}: same {same}, different {different}");
    }
    Ok(())
}
