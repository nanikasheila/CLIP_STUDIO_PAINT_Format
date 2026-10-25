//! Compositing the stored layer pixels into a canvas image (R10).
//!
//! The layers are drawn bottom to top (the first child of a folder is its
//! bottom layer) with visibility, opacity, blending mode, clipping, layer
//! masks and folders. Vector, gradient and correction layers, which CSP
//! draws when it opens a file, are drawn from their settings; text and 3D
//! layers without stored pixels are skipped and counted in the report.
//!
//! Rendering uses the mipmap level nearest above the wanted scale, so a
//! render at the size of `CanvasPreview` decodes only that level. Colours
//! are blended as premultiplied `f32` in the stored (sRGB) values. The
//! mapping of `LayerComposite` values to modes is in [`BlendMode`]; its
//! confidence is recorded in `docs/spec/`.

use std::collections::BTreeMap;

use crate::clip::ClipFile;
use crate::correction::Correction;
use crate::effect::{Border, LayerEffects, draw_border};
use crate::error::Result;
use crate::gradient::Gradient;
use crate::image::{GrayImage, RgbaImage};
use crate::model::{Layer, LayerKind, LayerNode};
use crate::offscreen::Offscreen;
use crate::stroke::draw_layer;

/// Blending modes, numbered as in `LayerComposite`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum BlendMode {
    Normal,
    Darken,
    Multiply,
    ColorBurn,
    LinearBurn,
    Subtract,
    DarkerColor,
    Lighten,
    Screen,
    ColorDodge,
    GlowDodge,
    Add,
    AddGlow,
    LighterColor,
    Overlay,
    SoftLight,
    HardLight,
    Difference,
    Exclusion,
    VividLight,
    LinearLight,
    PinLight,
    HardMix,
    Hue,
    Saturation,
    Color,
    Brightness,
    Divide,
    /// Folders only: the children blend with what is below the folder.
    PassThrough,
}

impl BlendMode {
    /// The mode of a `LayerComposite` value (None for unknown values).
    ///
    /// Confirmed with a file made in CSP EX that has one layer per mode
    /// (`docs/validation/V6-made-files.md`). The numbers do not follow the
    /// order of CSP's mode list: darker / lighter colour, difference,
    /// exclusion and divide were added later.
    pub fn from_code(code: i64) -> Option<Self> {
        use BlendMode::*;
        Some(match code {
            0 => Normal,
            1 => Darken,
            2 => Multiply,
            3 => ColorBurn,
            4 => LinearBurn,
            5 => Subtract,
            6 => DarkerColor,
            7 => Lighten,
            8 => Screen,
            9 => ColorDodge,
            10 => GlowDodge,
            11 => Add,
            12 => AddGlow,
            13 => LighterColor,
            14 => Overlay,
            15 => SoftLight,
            16 => HardLight,
            17 => VividLight,
            18 => LinearLight,
            19 => PinLight,
            20 => HardMix,
            21 => Difference,
            22 => Exclusion,
            23 => Hue,
            24 => Saturation,
            25 => Color,
            26 => Brightness,
            30 => PassThrough,
            36 => Divide,
            _ => return None,
        })
    }

    /// The `LayerComposite` value of the mode.
    pub fn code(self) -> i64 {
        (0..=36).find(|code| Self::from_code(*code) == Some(self)).unwrap_or(0)
    }

    /// 覆い焼き(発光) and 加算(発光).
    fn is_glow(self) -> bool {
        matches!(self, BlendMode::GlowDodge | BlendMode::AddGlow)
    }

    /// Separable blend of one channel (backdrop `b`, source `s`, both 0..1).
    fn channel(self, b: f32, s: f32) -> f32 {
        use BlendMode::*;
        let burn = |b: f32, s: f32| {
            if b >= 1.0 {
                1.0
            } else if s <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - b) / s).min(1.0)
            }
        };
        let dodge = |b: f32, s: f32| {
            if b <= 0.0 {
                0.0
            } else if s >= 1.0 {
                1.0
            } else {
                (b / (1.0 - s)).min(1.0)
            }
        };
        let hard = |b: f32, s: f32| if s <= 0.5 { b * 2.0 * s } else { b + (2.0 * s - 1.0) - b * (2.0 * s - 1.0) };
        let vivid = |b: f32, s: f32| if s <= 0.5 { burn(b, 2.0 * s) } else { dodge(b, 2.0 * s - 1.0) };
        match self {
            Normal | PassThrough => s,
            Darken => b.min(s),
            Multiply => b * s,
            ColorBurn => burn(b, s),
            LinearBurn => (b + s - 1.0).max(0.0),
            Subtract => (b - s).max(0.0),
            Lighten => b.max(s),
            Screen => b + s - b * s,
            ColorDodge | GlowDodge => dodge(b, s),
            Add | AddGlow => (b + s).min(1.0),
            Overlay => hard(s, b),
            SoftLight => {
                if s <= 0.5 {
                    b - (1.0 - 2.0 * s) * b * (1.0 - b)
                } else {
                    let d = if b <= 0.25 { ((16.0 * b - 12.0) * b + 4.0) * b } else { b.sqrt() };
                    b + (2.0 * s - 1.0) * (d - b)
                }
            }
            HardLight => hard(b, s),
            Difference => (b - s).abs(),
            Exclusion => b + s - 2.0 * b * s,
            VividLight => vivid(b, s),
            LinearLight => (b + 2.0 * s - 1.0).clamp(0.0, 1.0),
            PinLight => {
                if s <= 0.5 {
                    b.min(2.0 * s)
                } else {
                    b.max(2.0 * s - 1.0)
                }
            }
            HardMix => {
                // In 8-bit units: b + s > 255, and at exactly 255 only when
                // s < 128 (measured in CSP, docs/validation/V6-made-files.md).
                let (b8, s8) = ((b * 255.0).round(), (s * 255.0).round());
                if b8 + s8 > 255.0 || (b8 + s8 == 255.0 && s8 < 128.0) { 1.0 } else { 0.0 }
            }
            Divide => {
                if s <= 0.0 {
                    if b > 0.0 { 1.0 } else { 0.0 }
                } else {
                    (b / s).min(1.0)
                }
            }
            DarkerColor | LighterColor | Hue | Saturation | Color | Brightness => s,
        }
    }

    /// Blend a colour (non-premultiplied RGB).
    fn blend(self, b: [f32; 3], s: [f32; 3]) -> [f32; 3] {
        use BlendMode::*;
        match self {
            DarkerColor => {
                if lum8(s) <= lum8(b) {
                    s
                } else {
                    b
                }
            }
            LighterColor => {
                if lum8(s) >= lum8(b) {
                    s
                } else {
                    b
                }
            }
            Hue => set_lum(set_sat(s, sat(b)), lum(b)),
            Saturation => set_lum(set_sat(b, sat(s)), lum(b)),
            Color => set_lum(s, lum(b)),
            Brightness => set_lum(b, lum(s)),
            _ => [self.channel(b[0], s[0]), self.channel(b[1], s[1]), self.channel(b[2], s[2])],
        }
    }
}

/// The luminance darker / lighter colour compare: 0.3 R + 0.59 G + 0.11 B
/// rounded to 8 bits, ties going to the source (exact on CSP's output).
fn lum8(c: [f32; 3]) -> f32 {
    (lum(c) * 255.0).round()
}

fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn clip_color(c: [f32; 3]) -> [f32; 3] {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    let mut out = c;
    for v in &mut out {
        if n < 0.0 {
            *v = l + (*v - l) * l / (l - n);
        }
        if x > 1.0 {
            *v = l + (*v - l) * (1.0 - l) / (x - l);
        }
    }
    out
}

fn set_lum(c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(c);
    clip_color([c[0] + d, c[1] + d, c[2] + d])
}

fn sat(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    if max <= min {
        return [0.0; 3];
    }
    c.map(|v| (v - min) * s / (max - min))
}

/// A premultiplied RGBA float image.
#[derive(Debug, Clone)]
struct Buffer {
    width: usize,
    height: usize,
    data: Vec<[f32; 4]>,
}

impl Buffer {
    fn new(width: usize, height: usize) -> Self {
        Buffer { width, height, data: vec![[0.0; 4]; width * height] }
    }

    fn to_image(&self) -> RgbaImage {
        let mut image = RgbaImage::new(self.width as u32, self.height as u32);
        for (i, p) in self.data.iter().enumerate() {
            let a = p[3].clamp(0.0, 1.0);
            let un = |v: f32| if a > 0.0 { (v / a * 255.0).round().clamp(0.0, 255.0) as u8 } else { 0 };
            image.pixels[i * 4..i * 4 + 4].copy_from_slice(&[un(p[0]), un(p[1]), un(p[2]), (a * 255.0).round() as u8]);
        }
        image
    }

    /// Composite `source` (premultiplied) onto self with a mode; `coverage`
    /// scales the source alpha per pixel (opacity × mask × clipping base).
    fn composite(&mut self, source: &Buffer, mode: BlendMode, coverage: impl Fn(usize) -> f32) {
        self.composite_with(source, mode, coverage, false);
    }

    /// Like [`Buffer::composite`], but keep self's alpha: the source shows
    /// only where self has pixels (clipping onto a base layer).
    fn composite_atop(&mut self, source: &Buffer, mode: BlendMode, coverage: impl Fn(usize) -> f32) {
        self.composite_with(source, mode, coverage, true);
    }

    fn composite_with(&mut self, source: &Buffer, mode: BlendMode, coverage: impl Fn(usize) -> f32, atop: bool) {
        for i in 0..self.data.len() {
            let s = source.data[i];
            let k = coverage(i);
            if k <= 0.0 || s[3] <= 0.0 {
                continue;
            }
            let sa = s[3] * k;
            let b = self.data[i];
            let ba = b[3];
            let sc = [s[0] / s[3], s[1] / s[3], s[2] / s[3]];
            let bc = if ba > 0.0 { [b[0] / ba, b[1] / ba, b[2] / ba] } else { [0.0; 3] };
            let oa = if atop { ba } else { sa + ba * (1.0 - sa) };
            // Source where the backdrop is empty (not when clipping).
            let alone = if atop { 0.0 } else { sa * (1.0 - ba) };
            let mut out = [0.0; 4];
            if mode.is_glow() {
                // Glow modes blend the backdrop with the source colour times
                // its alpha and are not then mixed by alpha (measured in CSP).
                let mixed = mode.blend(bc, sc.map(|v| v * sa));
                for c in 0..3 {
                    out[c] = alone * sc[c] + ba * mixed[c];
                }
            } else {
                let mixed = mode.blend(bc, sc);
                for c in 0..3 {
                    // W3C compositing: source over, with the blended colour where both exist.
                    out[c] = alone * sc[c] + sa * ba * mixed[c] + (1.0 - sa) * b[c];
                }
            }
            out[3] = oa;
            self.data[i] = out;
        }
    }
}

/// Options for [`render`].
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    /// Output scale (1.0 = canvas size).
    pub scale: f64,
    /// Output size; by default the canvas size × `scale` rounded down.
    /// CSP's half-size levels round down at each step, so a quarter-size
    /// image can be a pixel larger than `size × 0.25` rounded down.
    pub size: Option<(u32, u32)>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions { scale: 1.0, size: None }
    }
}

/// What the renderer did, by layer count.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct RenderReport {
    pub width: u32,
    pub height: u32,
    pub drawn: usize,
    pub hidden: usize,
    /// Layers drawn without pixels of their own (no stored pixels), by kind.
    pub without_pixels: BTreeMap<String, usize>,
    /// `LayerComposite` values not in [`BlendMode`] (drawn as normal).
    pub unknown_blend: BTreeMap<i64, usize>,
    /// Modes used, by `LayerComposite` value.
    pub blend_modes: BTreeMap<i64, usize>,
    pub clipped: usize,
    pub masked: usize,
    /// Layer effects drawn, by section name (`EffectEdge`, …).
    pub effects: BTreeMap<String, usize>,
    /// Switched-on effects not drawn yet (tone, watercolour edge, line extraction).
    pub effects_skipped: BTreeMap<String, usize>,
    /// Vector layers drawn from their vector data (see [`crate::stroke`]).
    pub vector_layers: usize,
    /// Gradient layers drawn from `GradationFillInfo` (see [`crate::gradient`]),
    /// and how many of them only approximately (the 知覚的 mix).
    pub gradient_layers: usize,
    pub gradients_approximate: usize,
    /// Correction layers applied (see [`crate::correction`]), and how many
    /// of them only approximately.
    pub correction_layers: usize,
    pub corrections_approximate: usize,
}

struct Renderer<'a> {
    file: &'a ClipFile,
    scale: f64,
    width: usize,
    height: usize,
    report: RenderReport,
}

/// Render the canvas. The image has the canvas size × `options.scale`.
pub fn render(file: &ClipFile, options: RenderOptions) -> Result<(RgbaImage, RenderReport)> {
    let scale = options.scale.clamp(1e-3, 1.0);
    let (width, height) = match options.size {
        Some((width, height)) => (width.max(1) as usize, height.max(1) as usize),
        None => (
            ((file.canvas.width_px as f64 * scale).floor() as usize).max(1),
            ((file.canvas.height_px as f64 * scale).floor() as usize).max(1),
        ),
    };
    let mut renderer = Renderer { file, scale, width, height, report: RenderReport::default() };
    let mut canvas = Buffer::new(width, height);
    renderer.children(&file.layers.root, &mut canvas)?;
    let mut report = renderer.report;
    report.width = width as u32;
    report.height = height as u32;
    Ok((canvas.to_image(), report))
}

/// Render at the size of the stored `CanvasPreview`.
pub fn render_like_preview(file: &ClipFile) -> Result<Option<(RgbaImage, RenderReport, RgbaImage)>> {
    let Some(preview) = file.preview()? else { return Ok(None) };
    let scale = preview.width as f64 / file.canvas.width_px.max(1) as f64;
    let (mut image, report) = render(file, RenderOptions { scale, size: Some((preview.width, preview.height)) })?;
    if (image.width, image.height) != (preview.width, preview.height) {
        image = resample(&image, preview.width, preview.height);
    }
    Ok(Some((image, report, preview)))
}

fn resample(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    if image.width >= width && image.height >= height {
        return image.resize_area(width, height);
    }
    // Nearest neighbour for the (one pixel) enlargements rounding causes.
    let mut out = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let sx = (x as u64 * image.width as u64 / width as u64) as u32;
            let sy = (y as u64 * image.height as u64 / height as u64) as u32;
            out.set_pixel(x, y, image.pixel(sx.min(image.width - 1), sy.min(image.height - 1)));
        }
    }
    out
}

impl Renderer<'_> {
    /// Draw the children of `node` (bottom first) onto `target`.
    ///
    /// A layer and the clipping layers above it form a group, as in CSP:
    /// the clipping layers are drawn onto the base layer alone (keeping its
    /// alpha), and the group is then drawn with the base layer's mode,
    /// opacity and mask (measured in CSP, `docs/validation/V6-made-files.md`).
    /// A pass-through folder or a correction layer as the base has no image
    /// of its own; its clipping layers are drawn onto the backdrop, masked
    /// by the base's alpha.
    fn children(&mut self, node: &LayerNode, target: &mut Buffer) -> Result<()> {
        let mut group: Option<Group> = None;
        for child in &node.children {
            let layer = &child.layer;
            if !layer.clipping
                && let Some(done) = group.take()
            {
                done.flush(target);
            }
            if !layer.visible {
                self.report.hidden += 1;
                if !layer.clipping {
                    group = Some(Group::Empty);
                }
                continue;
            }
            let backdrop = match (&group, layer.clipping) {
                (Some(Group::Isolated { buffer, .. }), true) => buffer,
                _ => &*target,
            };
            let Some(content) = self.content(child, backdrop)? else {
                if !layer.clipping {
                    group = Some(Group::Empty);
                }
                continue;
            };
            let mut coverage = vec![layer.opacity_fraction() as f32; self.width * self.height];
            if layer.mask_enabled
                && !content.masked
                && let Some(mask) = self.mask(layer)?
            {
                self.report.masked += 1;
                for (c, m) in coverage.iter_mut().zip(mask) {
                    *c *= m;
                }
            }
            self.report.drawn += 1;
            if !layer.clipping {
                group = Some(match content.mode {
                    Some(mode) => Group::Isolated { buffer: content.buffer, mode, coverage },
                    None => {
                        let alpha = content.buffer.data.iter().zip(&coverage).map(|(p, c)| p[3] * c).collect();
                        mix(target, &content.buffer, &coverage);
                        Group::Backdrop { alpha }
                    }
                });
                continue;
            }
            self.report.clipped += 1;
            match &mut group {
                Some(Group::Isolated { buffer, .. }) => match content.mode {
                    Some(mode) => buffer.composite_atop(&content.buffer, mode, |i| coverage[i]),
                    None => mix(buffer, &content.buffer, &coverage),
                },
                Some(Group::Backdrop { alpha }) => {
                    for (c, a) in coverage.iter_mut().zip(alpha.iter()) {
                        *c *= a;
                    }
                    match content.mode {
                        Some(mode) => target.composite(&content.buffer, mode, |i| coverage[i]),
                        None => mix(target, &content.buffer, &coverage),
                    }
                }
                // No base (or a hidden or empty one): nothing shows.
                Some(Group::Empty) | None => self.report.drawn -= 1,
            }
        }
        if let Some(done) = group {
            done.flush(target);
        }
        Ok(())
    }

    /// The layer's own image: pixels, a folder's composite, or None.
    /// `mode` None means pass-through (the buffer already includes the backdrop).
    fn content(&mut self, node: &LayerNode, backdrop: &Buffer) -> Result<Option<Content>> {
        let layer = &node.layer;
        *self.report.blend_modes.entry(layer.blend).or_default() += 1;
        let mode = BlendMode::from_code(layer.blend).unwrap_or_else(|| {
            *self.report.unknown_blend.entry(layer.blend).or_default() += 1;
            BlendMode::Normal
        });
        if layer.kind.is_folder() {
            // Pass-through (30) folders blend their children with what is
            // below; any other mode makes the folder an isolated group.
            if mode == BlendMode::PassThrough {
                let mut buffer = backdrop.clone();
                self.children(node, &mut buffer)?;
                return Ok(Some(Content { buffer, mode: None, masked: false }));
            }
            let mut buffer = Buffer::new(self.width, self.height);
            self.children(node, &mut buffer)?;
            return Ok(Some(Content { buffer, mode: Some(mode), masked: false }));
        }
        if layer.kind == LayerKind::Paper {
            // The paper's stored pixels are not its look (older files store
            // opaque black); CSP draws the paper colour, white when unset.
            let color =
                layer.color("DrawColorMain").filter(|_| layer.row.truthy("DrawColorEnable")).unwrap_or([255; 3]);
            let pixel = [color[0] as f32 / 255.0, color[1] as f32 / 255.0, color[2] as f32 / 255.0, 1.0];
            let mut buffer = Buffer::new(self.width, self.height);
            buffer.data.fill(pixel);
            return Ok(Some(Content { buffer, mode: Some(mode), masked: false }));
        }
        if layer.kind == LayerKind::Correction
            && let Some(correction) = layer.row.blob("FilterLayerInfo").and_then(|data| Correction::parse(data).ok())
        {
            // The corrected backdrop, mixed in by opacity and mask like a
            // pass-through folder.
            self.report.correction_layers += 1;
            self.report.corrections_approximate += usize::from(correction.is_approximate());
            let filter = correction.filter();
            let mut buffer = backdrop.clone();
            for pixel in &mut buffer.data {
                let a = pixel[3];
                if a <= 0.0 {
                    continue;
                }
                let out = filter.apply([0, 1, 2].map(|c| pixel[c] / a * 255.0));
                for c in 0..3 {
                    pixel[c] = out[c] / 255.0 * a;
                }
            }
            return Ok(Some(Content { buffer, mode: None, masked: false }));
        }
        // Gradient layers are drawn from their settings, as CSP does when it
        // opens a file; the pixels some files store are a cache.
        if layer.kind == LayerKind::Gradient
            && let Some(mut gradient) = layer.row.blob("GradationFillInfo").and_then(|data| Gradient::parse(data).ok())
        {
            // Its points are relative to the layer's offset (a moved layer),
            // as vector points are.
            let (dx, dy) = (layer.offset.0 as f64, layer.offset.1 as f64);
            gradient.start = (gradient.start.0 + dx, gradient.start.1 + dy);
            gradient.end = (gradient.end.0 + dx, gradient.end.1 + dy);
            self.report.gradient_layers += 1;
            self.report.gradients_approximate += usize::from(gradient.is_approximate());
            let opaque = gradient.fill && gradient.ramp.stops.first().is_some_and(|stop| stop.color[3] >= 1.0)
                || gradient.ramp.stops.iter().all(|stop| stop.color[3] >= 1.0);
            let canvas_pixels = self.file.canvas.width_px as u64 * self.file.canvas.height_px as u64;
            if let Some((border, apply_opacity)) = self.effects(layer)
                // A border shows only where the layer has holes. It needs the
                // full-size image (see `pixels`), which very large canvases
                // cannot afford: those are drawn without it.
                && (layer.mask_enabled || !opaque)
                && canvas_pixels <= BORDER_PIXELS_MAX
            {
                let image = gradient.draw(self.file.canvas.width_px, self.file.canvas.height_px, 1.0);
                let (buffer, masked) = self.bordered(layer, image, (0, 0), &border, apply_opacity)?;
                return Ok(Some(Content { buffer, mode: Some(mode), masked }));
            }
            let image = gradient.draw(self.width as u32, self.height as u32, self.scale);
            return Ok(Some(Content {
                buffer: self.place(image, (0, 0), self.scale),
                mode: Some(mode),
                masked: false,
            }));
        }
        if let Some((buffer, masked)) = self.pixels(layer)? {
            return Ok(Some(Content { buffer, mode: Some(mode), masked }));
        }
        if layer.kind == LayerKind::Vector
            && let Some(image) = draw_layer(self.file, layer, self.width as u32, self.height as u32, self.scale)?
        {
            self.report.vector_layers += 1;
            return Ok(Some(Content {
                buffer: self.place(image, (0, 0), self.scale),
                mode: Some(mode),
                masked: false,
            }));
        }
        *self.report.without_pixels.entry(layer.kind.name().to_owned()).or_default() += 1;
        Ok(None)
    }

    /// The mipmap level to read for the output scale: the smallest stored
    /// level at or above it. CSP often stores only some levels (hidden
    /// layers only the full size), so levels without data are passed over;
    /// when none has data the full-size level is returned (its init colour
    /// still applies). Returns the offscreen and the level's scale.
    fn level(&self, mipmap: i64, scale: f64) -> Result<Option<(Offscreen, f64)>> {
        let Some(mipmap) = self.file.mipmap(mipmap)? else { return Ok(None) };
        let mut levels: Vec<_> = mipmap.levels.iter().filter(|level| level.scale / 100.0 + 1e-9 >= scale).collect();
        levels.sort_by(|a, b| a.scale.total_cmp(&b.scale));
        for level in levels {
            if let Some(offscreen) = self.file.offscreen(level.offscreen)?
                && offscreen.data.is_some()
            {
                return Ok(Some((offscreen, level.scale / 100.0)));
            }
        }
        let Some(first) = mipmap.levels.first() else { return Ok(None) };
        Ok(self.file.offscreen(first.offscreen)?.map(|offscreen| (offscreen, first.scale / 100.0)))
    }

    /// Place an image read at `level_scale` with its top-left at canvas
    /// `origin` into an output-sized buffer.
    fn place(&self, image: RgbaImage, origin: (i64, i64), level_scale: f64) -> Buffer {
        let factor = self.scale / level_scale;
        let halvings = exact_halvings(factor);
        // Output sizes are rounded down from the canvas size, so the scale
        // asked for is a little under the level's (826 / 1653 for a half-size
        // preview); within the tolerance of `exact_halvings` the level is
        // used as it is rather than resampled by a fraction of a pixel.
        let image = if (factor - 1.0).abs() < 0.005 {
            image
        } else if halvings > 0 {
            // CSP's mipmap levels: each halves the one above with a 2×2 box
            // (sizes rounded down). Following the same steps keeps the
            // pixel grid of the stored lower levels.
            let mut image = image;
            for _ in 0..halvings {
                image = image.halve();
            }
            image
        } else {
            let w = ((image.width as f64 * factor).round() as u32).max(1);
            let h = ((image.height as f64 * factor).round() as u32).max(1);
            image.resize_area(w, h)
        };
        let ox = (origin.0 as f64 * self.scale).round() as i64;
        let oy = (origin.1 as f64 * self.scale).round() as i64;
        let mut buffer = Buffer::new(self.width, self.height);
        for y in 0..image.height as i64 {
            let ty = oy + y;
            if ty < 0 || ty >= self.height as i64 {
                continue;
            }
            for x in 0..image.width as i64 {
                let tx = ox + x;
                if tx < 0 || tx >= self.width as i64 {
                    continue;
                }
                let p = image.pixel(x as u32, y as u32);
                let a = p[3] as f32 / 255.0;
                buffer.data[ty as usize * self.width + tx as usize] =
                    [p[0] as f32 / 255.0 * a, p[1] as f32 / 255.0 * a, p[2] as f32 / 255.0 * a, a];
            }
        }
        buffer
    }

    /// The layer's stored pixels placed in an output buffer, and whether
    /// the mask was applied to them already.
    fn pixels(&mut self, layer: &Layer) -> Result<Option<(Buffer, bool)>> {
        if layer.render_mipmap == 0 {
            return Ok(None);
        }
        let border = self.effects(layer);
        // A border is drawn at full size (its width is in canvas pixels),
        // then scaled with the layer.
        let scale = if border.is_some() { 1.0 } else { self.scale };
        let Some((offscreen, level_scale)) = self.level(layer.render_mipmap, scale)? else { return Ok(None) };
        if offscreen.data.is_none() {
            return Ok(None);
        }
        if let Some((border, apply_opacity)) = border {
            return Ok(Some(self.bordered(
                layer,
                offscreen.to_rgba()?,
                layer.pixel_origin(),
                &border,
                apply_opacity,
            )?));
        }
        // Halve block by block when the wanted scale is a power of two
        // below the level (large canvases read for a preview).
        let halvings = exact_halvings(self.scale / level_scale);
        let (image, level_scale) = if halvings > 0 {
            (offscreen.to_rgba_halved(halvings)?, level_scale / 2f64.powi(halvings as i32))
        } else {
            (offscreen.to_rgba()?, level_scale)
        };
        Ok(Some((self.place(image, layer.pixel_origin(), level_scale), false)))
    }

    /// Draw a border around a full-size layer image whose top-left is at
    /// canvas `origin`. The mask is applied first, so the border follows
    /// the masked shape (CSP draws the border of a masked fill layer around
    /// the mask's hole), and the result is not masked again.
    fn bordered(
        &self,
        layer: &Layer,
        mut image: RgbaImage,
        origin: (i64, i64),
        border: &Border,
        apply_opacity: bool,
    ) -> Result<(Buffer, bool)> {
        let masked = layer.mask_enabled && self.fold_mask(layer, &mut image, origin)?;
        let (bordered, margin) = draw_border(&image, border, apply_opacity);
        let origin = (origin.0 - margin as i64, origin.1 - margin as i64);
        Ok((self.place(bordered, origin, 1.0), masked))
    }

    /// Multiply the alpha of a full-size image at canvas `origin` by the
    /// layer's mask. Returns false when the layer has no mask.
    fn fold_mask(&self, layer: &Layer, image: &mut RgbaImage, origin: (i64, i64)) -> Result<bool> {
        if layer.mask_mipmap == 0 {
            return Ok(false);
        }
        let Some((offscreen, _)) = self.level(layer.mask_mipmap, 1.0)? else { return Ok(false) };
        let outside = offscreen.attribute.init_color().map(|init| (init.alpha & 0xff) as u8).unwrap_or(0);
        let mask = offscreen.to_alpha()?;
        let (mx, my) = layer.mask_origin();
        for y in 0..image.height {
            for x in 0..image.width {
                let (cx, cy) = (origin.0 + x as i64 - mx, origin.1 + y as i64 - my);
                let m = if cx >= 0 && cy >= 0 && (cx as u32) < mask.width && (cy as u32) < mask.height {
                    mask.pixels[cy as usize * mask.width as usize + cx as usize]
                } else {
                    outside
                };
                let i = (y * image.width + x) as usize * 4 + 3;
                image.pixels[i] = ((image.pixels[i] as u32 * m as u32 + 127) / 255) as u8;
            }
        }
        Ok(true)
    }

    /// The layer's switched-on effects: returns the border to draw and
    /// counts the others (not drawn yet) in the report.
    fn effects(&mut self, layer: &Layer) -> Option<(Border, bool)> {
        if !layer.row.truthy("LayerEffectAttached") {
            return None;
        }
        let effects = LayerEffects::parse(layer.row.blob("LayerEffectInfo")?).ok()?;
        for name in effects.enabled() {
            if name != "EffectEdge" {
                *self.report.effects_skipped.entry(name.to_owned()).or_default() += 1;
            }
        }
        let border = effects.border().filter(|border| border.enabled && border.width > 0.0)?;
        *self.report.effects.entry("EffectEdge".to_owned()).or_default() += 1;
        Some((border, effects.apply_opacity()))
    }

    /// The mask as coverage 0..1 per output pixel (outside the mask image:
    /// the mask's init value, usually hidden).
    fn mask(&self, layer: &Layer) -> Result<Option<Vec<f32>>> {
        if layer.mask_mipmap == 0 {
            return Ok(None);
        }
        let Some((offscreen, level_scale)) = self.level(layer.mask_mipmap, self.scale)? else { return Ok(None) };
        let outside = offscreen.attribute.init_color().map(|init| (init.alpha & 0xff) as f32 / 255.0).unwrap_or(0.0);
        let halvings = exact_halvings(self.scale / level_scale);
        let (image, level_scale) = if halvings > 0 {
            (offscreen.to_mask_rgba_halved(halvings)?, level_scale / 2f64.powi(halvings as i32))
        } else {
            (gray_to_rgba(&offscreen.to_alpha()?), level_scale)
        };
        // Inside the placed image alpha is 1 (fractional on resampled edges).
        let placed = self.place(image, layer.mask_origin(), level_scale);
        Ok(Some(placed.data.iter().map(|p| p[0] + outside * (1.0 - p[3])).collect()))
    }
}

/// The largest canvas (pixels) on which a gradient layer's border is drawn
/// (it needs several full-size buffers).
const BORDER_PIXELS_MAX: u64 = 40_000_000;

/// `n` when `factor` is 1/2ⁿ (n ≥ 1), else 0.
fn exact_halvings(factor: f64) -> u32 {
    let halvings = (1.0 / factor).log2().round();
    if halvings >= 1.0 && ((1.0 / factor) / 2f64.powf(halvings) - 1.0).abs() < 0.005 { halvings as u32 } else { 0 }
}

struct Content {
    buffer: Buffer,
    mode: Option<BlendMode>,
    /// The layer mask is already in the buffer (a border was drawn around
    /// the masked image).
    masked: bool,
}

/// A base layer and the clipping layers drawn onto it so far.
enum Group {
    /// The base's own image, drawn with its mode and coverage at the end.
    Isolated { buffer: Buffer, mode: BlendMode, coverage: Vec<f32> },
    /// The base was mixed into the backdrop (pass-through or correction);
    /// clipping layers are masked by its alpha.
    Backdrop { alpha: Vec<f32> },
    /// The base is hidden or has no image.
    Empty,
}

impl Group {
    fn flush(self, target: &mut Buffer) {
        if let Group::Isolated { buffer, mode, coverage } = self {
            target.composite(&buffer, mode, |i| coverage[i]);
        }
    }
}

/// Mix `source` (which already includes the backdrop, as a pass-through
/// folder or a correction layer does) into `target` by `coverage`.
fn mix(target: &mut Buffer, source: &Buffer, coverage: &[f32]) {
    for (i, pixel) in target.data.iter_mut().enumerate() {
        let k = coverage[i];
        let s = source.data[i];
        for c in 0..4 {
            pixel[c] += (s[c] - pixel[c]) * k;
        }
    }
}

fn gray_to_rgba(gray: &GrayImage) -> RgbaImage {
    let mut image = RgbaImage::new(gray.width, gray.height);
    for (i, v) in gray.pixels.iter().enumerate() {
        image.pixels[i * 4..i * 4 + 4].copy_from_slice(&[*v, *v, *v, 255]);
    }
    image
}

/// PSNR in dB of the RGB channels after compositing both images on white.
pub fn psnr(a: &RgbaImage, b: &RgbaImage) -> f64 {
    if (a.width, a.height) != (b.width, b.height) {
        return 0.0;
    }
    let flatten = |p: &[u8]| {
        let alpha = p[3] as f64 / 255.0;
        [0, 1, 2].map(|c| p[c] as f64 * alpha + 255.0 * (1.0 - alpha))
    };
    let mut sum = 0.0;
    for (pa, pb) in a.pixels.as_chunks::<4>().0.iter().zip(b.pixels.as_chunks::<4>().0.iter()) {
        let (x, y) = (flatten(pa), flatten(pb));
        for c in 0..3 {
            sum += (x[c] - y[c]).powi(2);
        }
    }
    let mse = sum / (a.pixels.len() / 4 * 3).max(1) as f64;
    if mse <= 1e-12 { 99.0 } else { 10.0 * (255.0f64 * 255.0 / mse).log10() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_modes_match_their_definitions() {
        assert_eq!(BlendMode::Multiply.channel(0.5, 0.5), 0.25);
        assert_eq!(BlendMode::Screen.channel(0.5, 0.5), 0.75);
        assert_eq!(BlendMode::Add.channel(0.7, 0.7), 1.0);
        assert_eq!(BlendMode::Overlay.channel(0.25, 1.0), 0.5);
        assert_eq!(BlendMode::Brightness.blend([1.0, 0.0, 0.0], [0.5, 0.5, 0.5]), set_lum([1.0, 0.0, 0.0], 0.5));
        assert_eq!(BlendMode::from_code(2), Some(BlendMode::Multiply));
        assert_eq!(BlendMode::from_code(99), None);
        assert_eq!(BlendMode::from_code(36), Some(BlendMode::Divide));
        assert_eq!(BlendMode::Divide.code(), 36);
    }

    #[test]
    fn normal_over_opaque_backdrop() {
        let mut backdrop = Buffer::new(1, 1);
        backdrop.data[0] = [1.0, 1.0, 1.0, 1.0];
        let mut source = Buffer::new(1, 1);
        source.data[0] = [0.0, 0.0, 0.5, 0.5];
        backdrop.composite(&source, BlendMode::Normal, |_| 1.0);
        assert_eq!(backdrop.data[0], [0.5, 0.5, 1.0, 1.0]);
        let mut backdrop = Buffer::new(1, 1);
        backdrop.data[0] = [0.5, 0.5, 0.5, 1.0];
        source.data[0] = [0.5, 0.5, 0.5, 1.0];
        backdrop.composite(&source, BlendMode::Multiply, |_| 1.0);
        assert_eq!(backdrop.data[0], [0.25, 0.25, 0.25, 1.0]);
    }

    #[test]
    fn psnr_of_equal_images_is_high() {
        let mut a = RgbaImage::new(2, 2);
        a.fill([10, 20, 30, 255]);
        assert_eq!(psnr(&a, &a), 99.0);
        let mut b = a.clone();
        b.set_pixel(0, 0, [11, 20, 30, 255]);
        assert!(psnr(&a, &b) > 50.0);
    }

    /// A clipping layer is drawn onto its base layer alone, and the group
    /// then with the base's mode and opacity (measured in CSP: a screen
    /// layer clipped to a half-transparent multiply layer at 50%).
    #[test]
    fn clipping_layers_group_with_their_base() {
        let canvas = crate::NewCanvas { width: 4, height: 4, paper: Some([200, 200, 200]), ..Default::default() };
        let mut file = ClipFile::create(&canvas).unwrap();
        let mut base = RgbaImage::new(4, 4);
        base.fill([56, 199, 199, 140]);
        let base_id = file.add_raster_layer("base", &base, (0, 0)).unwrap();
        file.set_layer_blend(base_id, BlendMode::Multiply).unwrap();
        file.set_layer_opacity(base_id, 0.5).unwrap();
        let mut top = RgbaImage::new(4, 4);
        top.fill([151, 202, 160, 255]);
        let top_id = file.add_raster_layer("top", &top, (0, 0)).unwrap();
        file.set_layer_blend(top_id, BlendMode::Screen).unwrap();
        file.set_layer_clipping(top_id, true).unwrap();
        let (image, report) = render(&file, RenderOptions::default()).unwrap();
        assert_eq!(report.clipped, 1);
        // Expected: group colour = screen(base, top) (the top is opaque), alpha
        // = the base's; then multiplied onto the paper at 0.5 × 140/255.
        let (b, t, paper): ([f64; 3], [f64; 3], f64) = ([56.0, 199.0, 199.0], [151.0, 202.0, 160.0], 200.0);
        let a = 0.5 * 140.0 / 255.0;
        let expected: Vec<u8> = (0..3)
            .map(|c| {
                let group = 255.0 - (255.0 - b[c]) * (255.0 - t[c]) / 255.0;
                (paper * (1.0 - a) + paper * group / 255.0 * a).round() as u8
            })
            .collect();
        let got = image.pixel(1, 1);
        for c in 0..3 {
            assert!(got[c].abs_diff(expected[c]) <= 1, "{got:?} vs {expected:?}");
        }
    }
}
