//! Test files for correction, gradient and fill layers.
//!
//! Input: a CSP-made file with raster layers and one layer of each kind
//! (tone curve, hue/saturation/value, brightness/contrast, posterize,
//! invert, levels, colour balance, threshold, gradient map, a gradient and
//! fill layers; see `docs/validation/V6b-generated-layers.md`). For every
//! variant, the bottom raster layer gets a hue × lightness backdrop with a
//! transparent strip, every other raster and generated layer is hidden,
//! and one generated layer is shown with the variant's settings.
//!
//! Usage: corr_test TEMPLATE.clip OUT_DIR [round2|round3|round4] [VARIANT…]

use csp_format::{ClipFile, LayerKind, Result, RgbaImage};
use rusqlite::types::Value;

fn hsl(h: f64, s: f64, l: f64) -> [u8; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [r, g, b].map(|v| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8)
}

fn words(values: &[i32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_be_bytes()).collect()
}

/// A FilterLayerInfo blob: type, payload size, payload.
fn filter(kind: i32, payload: &[i32]) -> Vec<u8> {
    let mut data = words(&[kind, payload.len() as i32 * 4]);
    data.extend(words(payload));
    data
}

/// GradationData with straight 16-bit stops `(rgba, position 0..1)`.
fn gradation_data(mode: i32, stops: &[([u16; 4], f64)]) -> Vec<u8> {
    gradation_data_full(Some((mode, 3)), stops, &[])
}

/// GradationData with an optional `(mixing mode, luminance correction)`
/// header (None: the older 16-byte header) and a curve on the first stop.
fn gradation_data_full(header: Option<(i32, i32)>, stops: &[([u16; 4], f64)], curve: &[(f64, f64)]) -> Vec<u8> {
    let mut data = Vec::new();
    match header {
        Some((mode, correction)) => data.extend(words(&[0x18, 0x1c, stops.len() as i32, 0x10, mode, correction])),
        None => data.extend(words(&[0x10, 0x1c, stops.len() as i32, 0x10])),
    }
    for (index, (rgba, position)) in stops.iter().enumerate() {
        for channel in rgba {
            let v = *channel as u32;
            data.extend(((v << 16) | v).to_be_bytes());
        }
        let curved = index == 0 && !curve.is_empty();
        let flags = if curved { 0x10 } else { 0 };
        let points = if curved { curve.len() as i32 } else { 0 };
        data.extend(words(&[flags, (position * 32768.0).round() as i32, points]));
    }
    for (x, y) in curve {
        data.extend(x.to_be_bytes());
        data.extend(y.to_be_bytes());
    }
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend(data);
    out
}

fn name(text: &str) -> Vec<u8> {
    let mut out = (text.encode_utf16().count() as u32).to_be_bytes().to_vec();
    for unit in text.encode_utf16() {
        out.extend(unit.to_be_bytes());
    }
    out
}

/// A whole GradationFillInfo: stops, shape (0 linear, 1 circular), the
/// ellipse and the start/end points; `fill` makes it a fill layer of that colour.
fn gradation_fill(
    mode: i32,
    stops: &[([u16; 4], f64)],
    shape: i32,
    geometry: [f64; 7],
    fill: Option<[u8; 3]>,
) -> Vec<u8> {
    gradation_fill_with(gradation_data(mode, stops), shape, geometry, fill)
}

fn gradation_fill_with(stops_data: Vec<u8>, shape: i32, geometry: [f64; 7], fill: Option<[u8; 3]>) -> Vec<u8> {
    let mut data = words(&[0, 2]);
    data.extend(name("GradationData"));
    data.extend(stops_data);
    data.extend(name("GradationSetting"));
    data.extend(words(&[0, shape, 1]));
    for value in geometry {
        data.extend(value.to_be_bytes());
    }
    data.extend(name("GradationSettingAdd0001"));
    let [r, g, b] = fill.unwrap_or([0; 3]).map(|c| (c as u32 * 0x0101_0101) as i32);
    data.extend(words(&[0x14, fill.is_some() as i32, r, g, b, 1]));
    let total = data.len() as u32;
    data[..4].copy_from_slice(&total.to_be_bytes());
    data
}

/// Tone curve: 32 slots (master, R, G, B, …) of `u16 count, 32 × (u16 x, u16 y)`.
fn tone_curve(channels: &[&[(u16, u16)]]) -> Vec<u8> {
    let mut payload = Vec::new();
    for slot in 0..32 {
        let points: &[(u16, u16)] = channels.get(slot).copied().unwrap_or(&[(0, 0), (0xffff, 0xffff)]);
        payload.extend((points.len() as u16).to_be_bytes());
        for i in 0..32 {
            let (x, y) = points.get(i).copied().unwrap_or((0, 0));
            payload.extend(x.to_be_bytes());
            payload.extend(y.to_be_bytes());
        }
    }
    let mut data = words(&[3, payload.len() as i32]);
    data.extend(payload);
    data
}

/// Levels: 32 slots of `u16 input black, gamma point, input white, output black, output white`.
fn levels(channels: &[[u16; 5]]) -> Vec<u8> {
    let mut payload = Vec::new();
    for slot in 0..32 {
        for value in channels.get(slot).copied().unwrap_or([0, 0x7fff, 0xffff, 0, 0xffff]) {
            payload.extend(value.to_be_bytes());
        }
    }
    let mut data = words(&[2, payload.len() as i32]);
    data.extend(payload);
    data
}

struct Variant {
    name: &'static str,
    /// FilterLayerInfo type of the layer to show (0: the gradient layer, -1: the fill layer).
    target: i32,
    blob: Vec<u8>,
    opacity: Option<f64>,
    /// Hide the backdrop layer (the paper shows through).
    hide_backdrop: bool,
}

fn variants() -> Vec<Variant> {
    let v = |name, target, blob| Variant { name, target, blob, opacity: None, hide_backdrop: false };
    let geometry = |x0, y0, x1, y1| [100.0, 100.0, 0.0, x0, y0, x1, y1];
    vec![
        v("curve", 3, tone_curve(&[&[(0, 0), (0x4000, 0x8000), (0xffff, 0xffff)]])),
        v("curve3", 3, tone_curve(&[&[(0, 0x1000), (0x5000, 0x3000), (0xa000, 0xe000), (0xffff, 0xf000)]])),
        v(
            "curve_rgb",
            3,
            tone_curve(&[
                &[(0, 0), (0xffff, 0xffff)],
                &[(0, 0), (0x8000, 0xc000), (0xffff, 0xffff)],
                &[(0, 0x4000), (0xffff, 0xc000)],
                &[(0, 0xffff), (0xffff, 0)],
            ]),
        ),
        v("hsv", 4, filter(4, &[30, 20, -10])),
        v("hsv2", 4, filter(4, &[-120, -50, 30])),
        v("bc", 1, filter(1, &[10, 20])),
        v("bc2", 1, filter(1, &[-40, -50])),
        v("posterize", 7, filter(7, &[5])),
        v("invert", 6, filter(6, &[])),
        v("levels", 2, levels(&[[0x2000, 0x5000, 0xe000, 0x1000, 0xf000]])),
        v(
            "levels_rgb",
            2,
            levels(&[
                [0, 0x7fff, 0xffff, 0, 0xffff],
                [0x4000, 0x7fff, 0xffff, 0, 0xffff],
                [0, 0x9000, 0xffff, 0, 0xffff],
                [0, 0x7fff, 0xffff, 0x4000, 0xc000],
            ]),
        ),
        v("balance", 5, filter(5, &[1, 0, 0, 0, 20, 0, 0, 0, 0, 0])),
        v("balance2", 5, filter(5, &[0, -30, 10, 0, 0, -40, 0, 0, 0, 50])),
        v("threshold", 8, filter(8, &[100])),
        v("gmap", 9, {
            let mut data = words(&[9]);
            let body = gradation_data(0, &[([0, 0, 0x8000, 0xffff], 0.0), ([0xffff, 0x8000, 0, 0xffff], 1.0)]);
            data.extend((body.len() as u32).to_be_bytes());
            data.extend(body);
            data
        }),
        v("gmap_perceptual", 9, {
            let mut data = words(&[9]);
            let body = gradation_data(1, &[([0, 0, 0x8000, 0xffff], 0.0), ([0xffff, 0x8000, 0, 0xffff], 1.0)]);
            data.extend((body.len() as u32).to_be_bytes());
            data.extend(body);
            data
        }),
        v(
            "grad_normal",
            0,
            gradation_fill(
                0,
                &[([0xffff, 0, 0, 0xffff], 0.0), ([0, 0, 0xffff, 0x4000], 1.0)],
                0,
                geometry(64.0, 64.0, 448.0, 320.0),
                None,
            ),
        ),
        v(
            "grad_perceptual",
            0,
            gradation_fill(
                1,
                &[([0xffff, 0, 0, 0xffff], 0.0), ([0, 0, 0xffff, 0x4000], 1.0)],
                0,
                geometry(64.0, 64.0, 448.0, 320.0),
                None,
            ),
        ),
        v(
            "grad_ps",
            0,
            gradation_fill(
                2,
                &[([0xffff, 0, 0, 0xffff], 0.0), ([0, 0, 0xffff, 0x4000], 1.0)],
                0,
                geometry(64.0, 64.0, 448.0, 320.0),
                None,
            ),
        ),
        v(
            "grad_linear",
            0,
            gradation_fill(
                3,
                &[([0xffff, 0, 0, 0xffff], 0.0), ([0, 0, 0xffff, 0x4000], 1.0)],
                0,
                geometry(64.0, 64.0, 448.0, 320.0),
                None,
            ),
        ),
        v(
            "grad_3stops",
            0,
            gradation_fill(
                0,
                &[([0, 0, 0, 0xffff], 0.0), ([0xffff, 0xffff, 0, 0xffff], 0.3), ([0, 0xffff, 0xffff, 0], 1.0)],
                0,
                geometry(256.0, 0.0, 256.0, 512.0),
                None,
            ),
        ),
        v(
            "grad_circle",
            0,
            gradation_fill(
                0,
                &[([0xffff, 0xffff, 0xffff, 0xffff], 0.0), ([0, 0x8000, 0, 0xffff], 1.0)],
                1,
                [300.0, 150.0, 30.0, 256.0, 256.0, 400.0, 256.0],
                None,
            ),
        ),
        v(
            "fill",
            -1,
            gradation_fill(
                0,
                &[([0, 0, 0, 0xffff], 0.0), ([0xffff, 0xffff, 0xffff, 0xffff], 1.0)],
                0,
                geometry(256.0, 0.0, 256.0, 512.0),
                Some([0x33, 0x66, 0xcc]),
            ),
        ),
        Variant {
            name: "fill_half",
            target: -1,
            blob: gradation_fill(
                0,
                &[([0, 0, 0, 0xffff], 0.0), ([0xffff, 0xffff, 0xffff, 0xffff], 1.0)],
                0,
                geometry(256.0, 0.0, 256.0, 512.0),
                Some([0xcc, 0x33, 0x66]),
            ),
            opacity: Some(0.5),
            hide_backdrop: false,
        },
    ]
}

/// Second round: one setting at a time, and gradients over the paper.
fn variants2() -> Vec<Variant> {
    let v = |name, target, blob| Variant { name, target, blob, opacity: None, hide_backdrop: false };
    let on_paper = |name, blob| Variant { name, target: 0, blob, opacity: None, hide_backdrop: true };
    let gmap = |header: Option<(i32, i32)>| {
        let mut data = words(&[9]);
        let body =
            gradation_data_full(header, &[([0, 0, 0x8000, 0xffff], 0.0), ([0xffff, 0x8000, 0, 0xffff], 1.0)], &[]);
        data.extend((body.len() as u32).to_be_bytes());
        data.extend(body);
        data
    };
    let stops = [([0xffff, 0, 0, 0xffff], 0.0), ([0, 0, 0xffff, 0x4000], 1.0)];
    let across = [100.0, 100.0, 0.0, 0.0, 256.0, 512.0, 256.0];
    let grad =
        |header, curve: &[(f64, f64)]| gradation_fill_with(gradation_data_full(header, &stops, curve), 0, across, None);
    vec![
        v("hsv_h60", 4, filter(4, &[60, 0, 0])),
        v("hsv_s50", 4, filter(4, &[0, 50, 0])),
        v("hsv_sm50", 4, filter(4, &[0, -50, 0])),
        v("hsv_v30", 4, filter(4, &[0, 0, 30])),
        v("hsv_vm30", 4, filter(4, &[0, 0, -30])),
        v("bc3", 1, filter(1, &[20, -30])),
        v("bc4", 1, filter(1, &[-20, 40])),
        v(
            "curve5",
            3,
            tone_curve(&[&[(0, 0), (0x2800, 0x6400), (0x8000, 0x3c00), (0xc800, 0xdc00), (0xffff, 0xffff)]]),
        ),
        v(
            "curve_mix",
            3,
            tone_curve(&[&[(0, 0), (0x8000, 0x4000), (0xffff, 0xffff)], &[(0, 0x4000), (0xffff, 0xffff)]]),
        ),
        v("bal_s", 5, filter(5, &[0, 100, 0, 0, 0, 0, 0, 0, 0, 0])),
        v("bal_m", 5, filter(5, &[0, 0, 0, 0, 100, 0, 0, 0, 0, 0])),
        v("bal_h", 5, filter(5, &[0, 0, 0, 0, 0, 0, 0, 100, 0, 0])),
        v("bal_s_neg", 5, filter(5, &[0, -100, 0, 0, 0, 0, 0, 0, 0, 0])),
        v("bal_m_pres", 5, filter(5, &[1, 0, 0, 0, 100, 0, 0, 0, 0, 0])),
        v("gmap_lc0", 9, gmap(Some((0, 0)))),
        v("gmap_lc3", 9, gmap(Some((0, 3)))),
        v("gmap_old", 9, gmap(None)),
        on_paper("grad_lc0", grad(Some((0, 0)), &[])),
        on_paper("grad_lc3", grad(Some((0, 3)), &[])),
        on_paper("grad_old", grad(None, &[])),
        on_paper("grad_perc0", grad(Some((1, 0)), &[])),
        on_paper("grad_curve", grad(Some((0, 0)), &[(0.0, 0.0), (0.25, 0.1), (0.75, 0.9), (1.0, 1.0)])),
    ]
}

/// Third round, over a grid where red runs down and blue across (green
/// halfway between), so every (max, min) pair appears: saturation, colour
/// balance at more values, and opaque gradients in every mixing mode.
fn variants3() -> Vec<Variant> {
    let v = |name, target, blob| Variant { name, target, blob, opacity: None, hide_backdrop: false };
    let on_paper = |name, blob| Variant { name, target: 0, blob, opacity: None, hide_backdrop: true };
    let balance = |values: [i32; 10]| filter(5, &values);
    let across = [100.0, 100.0, 0.0, 0.0, 256.0, 512.0, 256.0];
    let red_blue = [([0xffff, 0, 0, 0xffff], 0.0), ([0, 0, 0xffff, 0xffff], 1.0)];
    let black_white = [([0, 0, 0, 0xffff], 0.0), ([0xffff, 0xffff, 0xffff, 0xffff], 1.0)];
    let grad = |mode, correction, stops: &[([u16; 4], f64)]| {
        gradation_fill_with(gradation_data_full(Some((mode, correction)), stops, &[]), 0, across, None)
    };
    let gmap = |mode| {
        let mut data = words(&[9]);
        let body = gradation_data_full(Some((mode, 3)), &red_blue, &[]);
        data.extend((body.len() as u32).to_be_bytes());
        data.extend(body);
        data
    };
    vec![
        v("g_s25", 4, filter(4, &[0, 25, 0])),
        v("g_s50", 4, filter(4, &[0, 50, 0])),
        v("g_s100", 4, filter(4, &[0, 100, 0])),
        v("g_sm100", 4, filter(4, &[0, -100, 0])),
        v("g_v65", 4, filter(4, &[0, 0, 65])),
        v("g_hsv_a", 4, filter(4, &[20, 30, -30])),
        v("g_hsv_b", 4, filter(4, &[0, 100, -50])),
        v("g_hsv_c", 4, filter(4, &[-20, -40, 40])),
        v("g_bal_s50", 5, balance([0, 50, 0, 0, 0, 0, 0, 0, 0, 0])),
        v("g_bal_sm50", 5, balance([0, -50, 0, 0, 0, 0, 0, 0, 0, 0])),
        v("g_bal_m50", 5, balance([0, 0, 0, 0, 50, 0, 0, 0, 0, 0])),
        v("g_bal_mm100", 5, balance([0, 0, 0, 0, -100, 0, 0, 0, 0, 0])),
        v("g_bal_hm100", 5, balance([0, 0, 0, 0, 0, 0, 0, -100, 0, 0])),
        v("g_bal_hm50", 5, balance([0, 0, 0, 0, 0, 0, 0, -50, 0, 0])),
        v("g_bal_mix", 5, balance([0, 50, 0, 0, -50, 0, 0, 50, 0, 0])),
        v("g_bal_pres_s", 5, balance([1, 50, 0, 0, 0, 0, 0, 0, 0, 0])),
        v("g_bc_c100", 1, filter(1, &[-18, 100])),
        v("g_gmap_m1", 9, gmap(1)),
        on_paper("gp_m0", grad(0, 3, &red_blue)),
        on_paper("gp_m1", grad(1, 3, &red_blue)),
        on_paper("gp_m1_lc0", grad(1, 0, &red_blue)),
        on_paper("gp_m2", grad(2, 3, &red_blue)),
        on_paper("gp_m3", grad(3, 3, &red_blue)),
        on_paper("gp_m1_bw", grad(1, 3, &black_white)),
    ]
}

/// Fourth round: 知覚的 gradients, preserve luminosity, ellipse sizes and
/// a transparent stop (over the round-3 grid; gradients over the paper).
fn variants4() -> Vec<Variant> {
    let v = |name, target, blob| Variant { name, target, blob, opacity: None, hide_backdrop: false };
    let on_paper = |name, blob| Variant { name, target: 0, blob, opacity: None, hide_backdrop: true };
    let balance = |values: [i32; 10]| filter(5, &values);
    let across = [100.0, 100.0, 0.0, 0.0, 256.0, 512.0, 256.0];
    let opaque = |c: [u16; 3]| [c[0], c[1], c[2], 0xffff];
    let (black, white) = (opaque([0, 0, 0]), opaque([0xffff; 3]));
    let (red, green) = (opaque([0xffff, 0, 0]), opaque([0, 0xffff, 0]));
    let grad = |mode, correction, from: [u16; 4], to: [u16; 4]| {
        gradation_fill_with(
            gradation_data_full(Some((mode, correction)), &[(from, 0.0), (to, 1.0)], &[]),
            0,
            across,
            None,
        )
    };
    let ellipse = |geometry| {
        gradation_fill_with(gradation_data_full(Some((0, 3)), &[(white, 0.0), (black, 1.0)], &[]), 1, geometry, None)
    };
    vec![
        v("h_pres_m50", 5, balance([1, 0, 0, 0, 50, 0, 0, 0, 0, 0])),
        v("h_pres_sm50", 5, balance([1, -50, 0, 0, 0, 0, 0, 0, 0, 0])),
        v("h_pres_h50", 5, balance([1, 0, 0, 0, 0, 0, 0, 50, 0, 0])),
        v("h_pres_m50g", 5, balance([1, 0, 0, 0, 0, 50, 0, 0, 0, 0])),
        on_paper("h_p_bw0", grad(1, 0, black, white)),
        on_paper("h_p_bw1", grad(1, 1, black, white)),
        on_paper("h_p_bw2", grad(1, 2, black, white)),
        on_paper("h_p_wb3", grad(1, 3, white, black)),
        on_paper("h_p_rg0", grad(1, 0, red, green)),
        on_paper("h_p_rg3", grad(1, 3, red, green)),
        on_paper("h_e1", ellipse([300.0, 100.0, 0.0, 256.0, 256.0, 406.0, 256.0])),
        on_paper("h_e2", ellipse([300.0, 100.0, 45.0, 256.0, 256.0, 362.066, 362.066])),
        on_paper("h_clear", grad(0, 3, red, [0, 0, 0xffff, 0])),
    ]
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(template), Some(out_dir)) = (args.first(), args.get(1)) else {
        eprintln!("usage: corr_test TEMPLATE.clip OUT_DIR [round2|round3|round4] [VARIANT…]");
        std::process::exit(2);
    };
    let round = args.get(2).filter(|a| a.starts_with("round")).cloned().unwrap_or_default();
    let round2 = round == "round2";
    let round3 = round == "round3" || round == "round4";
    let only: Vec<&String> = args[2..].iter().filter(|a| !a.starts_with("round")).collect();
    let all = match round.as_str() {
        "round2" => variants2(),
        "round3" => variants3(),
        "round4" => variants4(),
        _ => variants(),
    };
    for variant in all {
        if !only.is_empty() && !only.iter().any(|name| *name == variant.name) {
            continue;
        }
        let mut file = ClipFile::open(template)?;
        let (width, height) = (file.canvas.width_px, file.canvas.height_px);
        let layers: Vec<_> = file.layers.flatten().into_iter().map(|(layer, _)| layer.clone()).collect();
        let rasters: Vec<i64> = layers.iter().filter(|l| l.kind == LayerKind::Raster).map(|l| l.main_id).collect();
        let mut backdrop = RgbaImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let [r, g, b] = hsl(x as f64 / width as f64 * 360.0, 0.8, 0.05 + 0.9 * y as f64 / height as f64);
                // A transparent strip and a half-transparent strip on the right.
                let [r, g, b] = if round2 && y < 32 { [(x * 255 / (width - 1)) as u8; 3] } else { [r, g, b] };
                let [r, g, b] = if round3 {
                    let (red, blue) = ((y * 255 / (height - 1)) as u8, (x * 255 / (width - 1)) as u8);
                    [red, ((red as u16 + blue as u16) / 2) as u8, blue]
                } else {
                    [r, g, b]
                };
                let alpha = if round3 {
                    255
                } else if x >= width - 32 {
                    0
                } else if x >= width - 64 {
                    128
                } else {
                    255
                };
                backdrop.set_pixel(x, y, [r, g, b, alpha]);
            }
        }
        file.set_layer_pixels(rasters[0], &backdrop, (0, 0))?;
        if variant.hide_backdrop {
            file.set_layer_visible(rasters[0], false)?;
        }
        // The template's raster layers carry a border effect; switch it off.
        file.set_layer_value(rasters[0], "LayerEffectAttached", Value::Integer(0))?;
        for id in &rasters[1..] {
            file.set_layer_visible(*id, false)?;
        }
        let mut shown = None;
        let mut fills = Vec::new();
        for layer in &layers {
            let kind = match layer.kind {
                LayerKind::Correction => layer
                    .row
                    .blob("FilterLayerInfo")
                    .and_then(|b| b.get(..4))
                    .map(|b| i32::from_be_bytes(b.try_into().expect("4"))),
                LayerKind::Gradient => {
                    let fill = layer.row.blob("GradationFillInfo").is_some_and(|b| {
                        // GradationSettingAdd0001's second word is the fill flag.
                        b.len() >= 20 && b[b.len() - 20..b.len() - 16] == [0, 0, 0, 1]
                    });
                    if fill {
                        fills.push(layer.main_id);
                    }
                    Some(if fill { -1 } else { 0 })
                }
                _ => None,
            };
            let Some(kind) = kind else { continue };
            let wanted =
                kind == variant.target && shown.is_none() && !(kind == -1 && layer.row.truthy("LayerEffectAttached"));
            file.set_layer_visible(layer.main_id, wanted)?;
            if wanted {
                shown = Some(layer.main_id);
            }
        }
        let Some(id) = shown else {
            eprintln!("{}: no layer of type {}", variant.name, variant.target);
            continue;
        };
        let column = if variant.target <= 0 { "GradationFillInfo" } else { "FilterLayerInfo" };
        file.set_layer_value(id, column, Value::Blob(variant.blob.clone()))?;
        if let Some(opacity) = variant.opacity {
            file.set_layer_opacity(id, opacity)?;
        }
        file.update_preview()?;
        file.renew_file_id();
        let path = format!("{out_dir}/corr_{}.clip", variant.name);
        file.save(&path)?;
        let report = csp_format::verify::check_clip(std::fs::read(&path)?, Default::default());
        println!("{path}: layer {id}, self-check ok = {}", report.ok());
    }
    Ok(())
}
