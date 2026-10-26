//! Diagnostic variants of a pixel edit, to find what CSP rejects.
use csp_format::database::value_as_text;
use csp_format::offscreen::{Attribute, encode};
use csp_format::{ClipFile, RgbaImage};

fn patch(size: u32) -> RgbaImage {
    let mut image = RgbaImage::new(size, size);
    image.fill([200, 40, 40, 255]);
    image
}

fn main() -> csp_format::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (input, out) = (&args[0], &args[1]);
    let layer = 3; // bottom raster layer of the CSP-made file
    // A: only the 100% level, same external id (payload replaced in place).
    let mut file = ClipFile::open(input)?;
    let mipmap = file.mipmap(file.layers.get(layer).unwrap().render_mipmap)?.unwrap();
    let level = &mipmap.levels[0];
    let row = file.row("Offscreen", level.offscreen)?.unwrap();
    let mut attribute = Attribute::parse(row.blob("Attribute").unwrap())?;
    let parameter = attribute.parameter()?;
    let mut full = RgbaImage::new(parameter.width, parameter.height);
    full.paste(&patch(100), 10, 10);
    let data = encode(&parameter, Some(&full), None)?;
    attribute.set_block_sizes(&data.record_sizes());
    let id = row.get("BlockData").and_then(value_as_text).unwrap();
    file.set_external(&id, data.to_bytes());
    file.database()
        .execute("UPDATE Offscreen SET Attribute = ?1 WHERE MainId = ?2", (attribute.to_bytes(), level.offscreen))?;
    file.save(format!("{out}_A.clip"))?;
    // B: the library edit on one layer (all levels, new ids).
    let mut file = ClipFile::open(input)?;
    file.set_layer_pixels(layer, &patch(100), (10, 10))?;
    file.save(format!("{out}_B.clip"))?;
    // D: as B, with a new file id.
    file.renew_file_id();
    file.save(format!("{out}_D.clip"))?;
    // C: nothing but a save through the writer with one DB change (layer name).
    let mut file = ClipFile::open(input)?;
    file.set_layer_name(layer, "renamed")?;
    file.save(format!("{out}_C.clip"))?;
    Ok(())
}
