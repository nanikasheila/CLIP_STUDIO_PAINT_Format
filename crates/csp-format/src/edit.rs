//! Editing an opened .clip (R12): layer pixels, layer properties and the
//! stored preview.
//!
//! Each change is made in the in-memory database and external chunks, so
//! [`ClipFile::save`] writes a consistent file: new external chunks are
//! listed in `ExternalChunk` with their offsets, and replaced chunks are
//! dropped. Layer pixels are written to every mipmap level (each level is
//! the one above halved with a 2×2 box, as CSP makes them).

use std::hash::{BuildHasher, Hasher};

use rusqlite::types::Value;

use crate::clip::ClipFile;
use crate::database::value_as_text;
use crate::error::{Error, Result, bail};
use crate::image::RgbaImage;
use crate::offscreen::{Attribute, PixelKind, encode};
use crate::render::{BlendMode, render_like_preview};

/// 16 random bytes shaped as a version 4 UUID (CSP's ids are UUIDs).
pub fn new_uuid() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    for half in bytes.chunks_mut(8) {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u128(
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos(),
        );
        half.copy_from_slice(&hasher.finish().to_be_bytes());
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    bytes
}

/// A new external chunk id: "extrnlid" and a UUID as 32 upper-case hex digits.
pub fn new_external_id() -> String {
    let mut text = String::from("extrnlid");
    for byte in new_uuid() {
        text.push_str(&format!("{byte:02X}"));
    }
    text
}

impl ClipFile {
    /// Give the saved file a new random id (see [`ClipFile::set_file_id`]).
    pub fn renew_file_id(&mut self) {
        self.set_file_id(new_uuid());
    }

    /// Replace a raster layer's pixels. `image` is placed with its top-left
    /// at canvas position `position`; pixels outside the layer's offscreen
    /// are dropped. Only colour layers are supported.
    pub fn set_layer_pixels(&mut self, layer_id: i64, image: &RgbaImage, position: (i64, i64)) -> Result<()> {
        let layer = self.layers.get(layer_id).ok_or_else(|| Error::format(format!("No layer {layer_id}")))?.clone();
        let mipmap = self
            .mipmap(layer.render_mipmap)?
            .ok_or_else(|| Error::format(format!("Layer {layer_id} has no render mipmap")))?;
        let origin = layer.pixel_origin();
        let mut level_image: Option<RgbaImage> = None;
        for level in &mipmap.levels {
            let row = self
                .row("Offscreen", level.offscreen)?
                .ok_or_else(|| Error::format(format!("No Offscreen {}", level.offscreen)))?;
            let mut attribute = Attribute::parse(row.blob("Attribute").unwrap_or_default())?;
            let parameter = attribute.parameter()?;
            if parameter.kind() != PixelKind::Color {
                bail!("Layer {layer_id} is not a colour layer ({:?})", parameter.kind());
            }
            let pixels = match level_image.take() {
                None => {
                    let mut full = RgbaImage::new(parameter.width, parameter.height);
                    full.paste(image, position.0 - origin.0, position.1 - origin.1);
                    full
                }
                Some(above) => {
                    let mut halved = above.halve();
                    if (halved.width, halved.height) != (parameter.width, parameter.height) {
                        halved = halved.resize_area(parameter.width, parameter.height);
                    }
                    halved
                }
            };
            let data = encode(&parameter, Some(&pixels), None)?;
            attribute.set_block_sizes(&data.record_sizes());
            let old = row.get("BlockData").and_then(value_as_text).filter(|id| !id.is_empty());
            let id = new_external_id();
            if let Some(old) = &old {
                self.remove_external(old);
            }
            self.set_external(&id, data.to_bytes());
            self.database().execute(
                "UPDATE Offscreen SET Attribute = ?1, BlockData = ?2 WHERE MainId = ?3",
                (attribute.to_bytes(), Value::Blob(id.into_bytes()), level.offscreen),
            )?;
            level_image = Some(pixels);
        }
        Ok(())
    }

    /// Set one `Layer` column.
    pub fn set_layer_value(&mut self, layer_id: i64, column: &str, value: Value) -> Result<()> {
        if !crate::database::has_column(self.database(), "Layer", column)? {
            bail!("Layer has no column {column}");
        }
        let sql = format!("UPDATE Layer SET {} = ?1 WHERE MainId = ?2", crate::database::quote(column));
        if self.database().execute(&sql, (value, layer_id))? == 0 {
            bail!("No layer {layer_id}");
        }
        self.reload()
    }

    pub fn set_layer_name(&mut self, layer_id: i64, name: &str) -> Result<()> {
        self.set_layer_value(layer_id, "LayerName", Value::Text(name.to_owned()))
    }

    /// Opacity 0.0..=1.0 (stored as 0..=256).
    pub fn set_layer_opacity(&mut self, layer_id: i64, opacity: f64) -> Result<()> {
        let stored = (opacity.clamp(0.0, 1.0) * crate::model::OPACITY_FULL as f64).round() as i64;
        self.set_layer_value(layer_id, "LayerOpacity", Value::Integer(stored))
    }

    pub fn set_layer_blend(&mut self, layer_id: i64, mode: BlendMode) -> Result<()> {
        self.set_layer_value(layer_id, "LayerComposite", Value::Integer(mode.code()))
    }

    pub fn set_layer_visible(&mut self, layer_id: i64, visible: bool) -> Result<()> {
        let layer = self.layers.get(layer_id).ok_or_else(|| Error::format(format!("No layer {layer_id}")))?;
        let value = (layer.row.int("LayerVisibility") & !1) | visible as i64;
        self.set_layer_value(layer_id, "LayerVisibility", Value::Integer(value))
    }

    pub fn set_layer_clipping(&mut self, layer_id: i64, clipping: bool) -> Result<()> {
        self.set_layer_value(layer_id, "LayerClip", Value::Integer(clipping as i64))
    }

    /// Re-render `CanvasPreview` from the layers (call after editing pixels
    /// so file browsers show the new content).
    pub fn update_preview(&mut self) -> Result<bool> {
        let Some((image, _, _)) = render_like_preview(self)? else { return Ok(false) };
        let png = image.to_png()?;
        self.database().execute(
            "UPDATE CanvasPreview SET ImageData = ?1, ImageWidth = ?2, ImageHeight = ?3",
            (png, image.width as i64, image.height as i64),
        )?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_ids_have_csp_shape() {
        let id = new_external_id();
        assert!(crate::container::is_external_id(&id), "{id}");
        assert_ne!(id, new_external_id());
    }
}
