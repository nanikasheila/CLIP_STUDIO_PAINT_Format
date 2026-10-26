//! Validation: copy layers between files with `copy_layer_from` and compare
//! the copies with the originals. Every raster, vector and text layer of
//! SOURCE (and every frame folder, without its layers) is copied into TARGET (a template
//! page, or a new canvas the size of the source when TARGET is `-`). The result must pass `check_clip`,
//! and each copy must have the original's kind, vector data, text and
//! visible stored pixels. With OUT, the result is saved there (to open in CSP).
//! Prints counts only (no names or text).
//!
//! COPY_KINDS (e.g. `Raster,Vector`), COPY_SKIP and COPY_MAX narrow what is
//! copied, to find which layers a CSP problem comes from.
//!
//! Usage: copy_check SOURCE.clip (TARGET.clip | -) [OUT.clip]
use csp_format::copy::CopyLayer;
use csp_format::database::{select, table_exists, value_as_text};
use csp_format::{ClipFile, LayerKind, NewCanvas};

fn vector_payloads(file: &ClipFile, layer: i64) -> Vec<Vec<u8>> {
    if !table_exists(file.database(), "VectorObjectList").unwrap_or(false) {
        return Vec::new();
    }
    select(file.database(), "SELECT VectorData FROM VectorObjectList WHERE LayerId = ?1 ORDER BY MainId", [layer])
        .unwrap()
        .iter()
        .filter_map(|row| row.values().first().and_then(value_as_text))
        .filter_map(|id| file.external(&id).map(<[u8]>::to_vec))
        .collect()
}

fn texts(file: &ClipFile, layer: i64) -> Vec<String> {
    let row = &file.layers.get(layer).unwrap().row;
    csp_format::text::read_blocks(row).unwrap_or_default().into_iter().map(|block| block.text).collect()
}

/// The image with fully transparent pixels made [0, 0, 0, 0].
fn visible(mut image: csp_format::RgbaImage) -> csp_format::RgbaImage {
    for pixel in image.pixels.chunks_mut(4) {
        if pixel[3] == 0 {
            pixel.fill(0);
        }
    }
    image
}

/// A コマ枠フォルダー: a folder with frame border images.
fn is_frame_folder(layer: &csp_format::Layer) -> bool {
    layer.kind == LayerKind::FrameFolder
}

fn main() -> csp_format::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let source = ClipFile::open(&args[0])?;
    let mut target = match args[1].as_str() {
        "-" => ClipFile::create(&NewCanvas {
            width: source.canvas.width_px,
            height: source.canvas.height_px,
            resolution: source.canvas.resolution,
            ..Default::default()
        })?,
        path => ClipFile::open(path)?,
    };
    let kinds: Option<Vec<String>> =
        std::env::var("COPY_KINDS").ok().map(|kinds| kinds.split(',').map(str::to_owned).collect());
    let limit = std::env::var("COPY_MAX").ok().and_then(|n| n.parse().ok()).unwrap_or(usize::MAX);
    let skip = std::env::var("COPY_SKIP").ok().and_then(|n| n.parse().ok()).unwrap_or(0);
    target.renew_file_id();
    target.prepare_copy_from(&source)?;
    let wanted: Vec<i64> = source
        .layers
        .flatten()
        .into_iter()
        .filter(|(layer, _)| {
            matches!(layer.kind, LayerKind::Raster | LayerKind::Vector | LayerKind::Text) || is_frame_folder(layer)
        })
        .filter(|(layer, _)| kinds.as_ref().is_none_or(|kinds| kinds.contains(&format!("{:?}", layer.kind))))
        .map(|(layer, _)| layer.main_id)
        .skip(skip)
        .take(limit)
        .collect();
    let mut pairs = Vec::new();
    let mut frames = 0;
    for id in &wanted {
        let copy = target.copy_layer_from(&source, *id, CopyLayer::default())?;
        if is_frame_folder(source.layers.get(*id).unwrap()) {
            target.add_frame_images(copy)?;
            frames += 1;
        }
        pairs.push((*id, copy));
    }
    let bytes = target.to_bytes()?;
    let report = csp_format::verify::check_clip(bytes.clone(), Default::default());
    let copied = ClipFile::from_bytes(bytes.clone())?;
    let mut failures = Vec::new();
    for (old, new) in &pairs {
        let (a, b) = (source.layers.get(*old).unwrap(), copied.layers.get(*new).unwrap());
        if a.kind != b.kind {
            failures.push(format!("{new}: kind {:?} → {:?}", a.kind, b.kind));
        }
        if vector_payloads(&source, *old) != vector_payloads(&copied, *new) {
            failures.push(format!("{new}: vector data differs"));
        }
        if texts(&source, *old) != texts(&copied, *new) {
            failures.push(format!("{new}: text differs"));
        }
        if a.kind == LayerKind::Raster {
            let pixels = |file: &ClipFile, layer| {
                file.layer_pixels(layer).unwrap().map(|(image, origin)| (image.alpha_bbox(), origin, image))
            };
            let (pa, pb) = (pixels(&source, a), pixels(&copied, b));
            let same = match (&pa, &pb) {
                (None, None) => true,
                (Some((Some(bbox), origin, image)), Some(_)) => {
                    let region = (
                        origin.0 + bbox.0 as i64,
                        origin.1 + bbox.1 as i64,
                        origin.0 + bbox.2 as i64,
                        origin.1 + bbox.3 as i64,
                    );
                    let mut expected = csp_format::RgbaImage::new(bbox.2 - bbox.0, bbox.3 - bbox.1);
                    expected.paste(image, -(bbox.0 as i64), -(bbox.1 as i64));
                    // Colour under fully transparent pixels is not kept
                    // (blocks with no visible pixel are not stored).
                    visible(copied.layer_pixels_in_box(b, region)?) == visible(expected)
                }
                (Some((None, ..)), _) => true,
                _ => false,
            };
            if !same {
                // Why: the stored size and place against the canvas, the
                // pixel layout, and how many visible pixels differ.
                let base = source.mipmap_base(a.render_mipmap)?.unwrap();
                let (w, h) = (base.parameter.width as i64, base.parameter.height as i64);
                let origin = a.pixel_origin();
                let canvas = (copied.canvas.width_px as i64, copied.canvas.height_px as i64);
                let mut differing = 0;
                let mut outside = 0;
                if let Some((image, origin)) = source.layer_pixels(a)? {
                    let inside = copied.layer_pixels_in_box(b, (0, 0, canvas.0, canvas.1))?;
                    for y in 0..image.height as i64 {
                        for x in 0..image.width as i64 {
                            let (cx, cy) = (x + origin.0, y + origin.1);
                            let pixel = image.pixel(x as u32, y as u32);
                            if !(0..canvas.0).contains(&cx) || !(0..canvas.1).contains(&cy) {
                                outside += (pixel[3] != 0) as usize;
                            } else if pixel != inside.pixel(cx as u32, cy as u32)
                                && (pixel[3] != 0 || inside.pixel(cx as u32, cy as u32)[3] != 0)
                            {
                                differing += 1;
                            }
                        }
                    }
                }
                failures.push(format!(
                    "{new}: pixels differ ({:?}, stored {w}×{h} at {origin:?}, canvas {canvas:?}, init {}, {differing} differ inside, {outside} visible outside)",
                    base.kind(),
                    base.attribute.init_color().is_some_and(|init| init.do_initialize)
                ));
            }
        }
    }
    println!(
        "layers {} copied ({frames} frame folders), check {}, failures {}",
        pairs.len(),
        if report.ok() { "ok".to_owned() } else { format!("{:?}", report.errors) },
        failures.len()
    );
    for failure in &failures {
        println!("  {failure}");
    }
    if let Some(out) = args.get(2) {
        std::fs::write(out, bytes)?;
    }
    if !report.ok() || !failures.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}
