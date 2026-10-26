//! Validation: `layer_pixels_in_box` and `stored_pixel_boxes` against
//! `layer_pixels` on real files. For every layer with stored pixels, each
//! stored box and a box reaching past the layer's corner must read exactly
//! the pixels of the whole decoded layer (transparent outside), and every
//! pixel with alpha must lie in a stored box. Prints one line per file
//! (counts only, no names).
//!
//! Usage: box_check FILE.clip...
use csp_format::{ClipFile, RgbaImage};

fn crop(image: &RgbaImage, origin: (i64, i64), region: (i64, i64, i64, i64)) -> RgbaImage {
    let mut out = RgbaImage::new((region.2 - region.0) as u32, (region.3 - region.1) as u32);
    out.paste(image, origin.0 - region.0, origin.1 - region.1);
    out
}

fn check(path: &str) -> csp_format::Result<(usize, usize, usize)> {
    let file = ClipFile::open(path)?;
    let (mut layers, mut boxes, mut failures) = (0, 0, 0);
    for (layer, _) in file.layers.flatten() {
        let Some((image, origin)) = file.layer_pixels(layer)? else { continue };
        layers += 1;
        let stored = file.stored_pixel_boxes(layer)?;
        let (w, h) = (image.width as i64, image.height as i64);
        let mut regions = stored.clone();
        regions.push((origin.0 + w - 300, origin.1 + h - 300, origin.0 + w + 50, origin.1 + h + 50));
        regions.push((origin.0 - 7, origin.1 - 7, origin.0 + 9, origin.1 + 9));
        for region in regions {
            boxes += 1;
            if file.layer_pixels_in_box(layer, region)? != crop(&image, origin, region) {
                failures += 1;
            }
        }
        // Every visible pixel lies in a stored block (init colour aside).
        let mut covered = RgbaImage::new(image.width, image.height);
        for (x0, y0, x1, y1) in &stored {
            let mut tile = RgbaImage::new((x1 - x0) as u32, (y1 - y0) as u32);
            tile.fill([0, 0, 0, 255]);
            covered.paste(&tile, x0 - origin.0, y0 - origin.1);
        }
        let painted_outside = image
            .pixels
            .chunks(4)
            .zip(covered.pixels.chunks(4))
            .any(|(pixel, cover)| pixel[3] != 0 && cover[3] == 0 && pixel != [255, 255, 255, 255]);
        if painted_outside {
            failures += 1;
        }
    }
    Ok((layers, boxes, failures))
}

fn main() {
    let mut total_failures = 0;
    for (index, path) in std::env::args().skip(1).enumerate() {
        match check(&path) {
            Ok((layers, boxes, failures)) => {
                total_failures += failures;
                println!("file {index}: layers {layers} boxes {boxes} failures {failures}");
            }
            Err(error) => {
                total_failures += 1;
                println!("file {index}: error {error}");
            }
        }
    }
    if total_failures > 0 {
        std::process::exit(1);
    }
}
