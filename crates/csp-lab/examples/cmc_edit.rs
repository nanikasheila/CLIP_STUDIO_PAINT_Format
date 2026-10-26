//! Validation (V6, R14): edit a work's page list and write it as a new work
//! to open in CSP. Keeps the first four pages, removes the rest, adds a page
//! after the first and swaps the last two. Each written page gets a layer
//! with N black bars (N = its page number in the source; the new page has
//! 5), so CSP's page list shows whether the order is right. The source is
//! only read.
//!
//! Usage: cmc_edit SOURCE.cmc OUT_DIR
use std::path::Path;

use csp_format::{ClipFile, CmcFile, RgbaImage};

fn bars(width: u32, height: u32, count: u32) -> RgbaImage {
    let mut image = RgbaImage::new(width, height);
    let bar_height = (height / 4).max(1);
    for bar in 0..count {
        let left = width / 10 + bar * width / 12;
        for y in height / 10..(height / 10 + bar_height).min(height) {
            for x in left..(left + width / 24).min(width) {
                image.set_pixel(x, y, [0, 0, 0, 255]);
            }
        }
    }
    image
}

fn write_page(source: &Path, out: &Path, count: u32) -> csp_format::Result<()> {
    let mut page = ClipFile::open(source)?;
    let image = bars(page.canvas.width_px, page.canvas.height_px, count);
    page.add_raster_layer(&format!("bars {count}"), &image, (0, 0))?;
    page.update_preview()?;
    page.renew_file_id();
    page.save(out)
}

fn main() -> csp_format::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (source, out) = (Path::new(&args[0]), Path::new(&args[1]));
    let folder = source.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(out)?;
    let mut work = CmcFile::open(source)?;
    let pages: Vec<(i64, std::path::PathBuf)> =
        work.pages()?.iter().map(|node| (node.main_id, node.relative_path().expect("page file"))).collect();
    if pages.len() < 4 {
        return Err(csp_format::Error::format("the source work needs at least 4 pages"));
    }
    for (number, (_, file)) in pages.iter().take(4).enumerate() {
        write_page(&folder.join(file), &out.join(file), number as u32 + 1)?;
    }
    for (id, _) in &pages[4..] {
        work.remove_page(*id)?;
    }
    let (new_id, new_file) = work.add_page(Some(pages[0].0), false)?;
    write_page(&folder.join(&pages[0].1), &out.join(&new_file), 5)?;
    let order = [pages[0].0, new_id, pages[1].0, pages[3].0, pages[2].0];
    work.reorder(&order)?;
    let name = source.file_name().expect("file name");
    work.save(out.join(name))?;
    let check = CmcFile::open(out.join(name))?;
    let findings = check.findings(Some(out));
    println!("pages {:?} findings {:?}", check.pages()?.iter().map(|n| n.main_id).collect::<Vec<_>>(), findings);
    println!("expected bars in order: 1 5 2 4 3");
    Ok(())
}
