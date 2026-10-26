//! Which kinds of layer keep a render from matching CSP's stored preview,
//! over a corpus: render each file with everything, then with its vector
//! layers drawn from vector data hidden, then with its text layers hidden,
//! and split the squared error into the pixels each kind changes. Prints
//! one JSON line per file with keys and numbers only (no names or paths).
//!
//! Usage: kind_ablation ROOT LIST.txt OUT.jsonl [--shard K/N] [--seconds S]

use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::time::Instant;

use csp_format::render::{psnr, render_like_preview};
use csp_format::{ClipFile, LayerKind, Result, RgbaImage};
use serde_json::json;

fn key_of(relative: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in relative.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// A pixel on white, as the PSNR sees it.
fn flat(p: [u8; 4]) -> [f64; 3] {
    let a = p[3] as f64 / 255.0;
    [0, 1, 2].map(|c| p[c] as f64 * a + 255.0 * (1.0 - a))
}

fn squared(a: [u8; 4], b: [u8; 4]) -> f64 {
    let (a, b) = (flat(a), flat(b));
    (0..3).map(|c| (a[c] - b[c]).powi(2)).sum()
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [root, list, out, rest @ ..] = args.as_slice() else {
        panic!("usage: kind_ablation ROOT LIST.txt OUT.jsonl [--shard K/N] [--seconds S]");
    };
    let mut shard = (0usize, 1usize);
    let mut seconds = 0f64;
    let mut iter = rest.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--shard" => {
                let text = iter.next().cloned().unwrap_or_default();
                let (k, n) = text.split_once('/').unwrap_or(("0", "1"));
                shard = (k.parse().unwrap_or(0), n.parse().unwrap_or(1));
            }
            "--seconds" => seconds = iter.next().and_then(|s| s.parse().ok()).unwrap_or(0.0),
            _ => {}
        }
    }
    let mut done = HashSet::new();
    if let Ok(existing) = std::fs::File::open(out) {
        for line in std::io::BufReader::new(existing).lines().map_while(std::result::Result::ok) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line)
                && let Some(key) = value.get("key").and_then(|k| k.as_str())
            {
                done.insert(key.to_owned());
            }
        }
    }
    let mut output = std::fs::OpenOptions::new().create(true).append(true).open(out)?;
    if std::fs::read(out).is_ok_and(|bytes| bytes.last().is_some_and(|last| *last != b'\n')) {
        writeln!(output)?;
    }
    let started = Instant::now();
    let mut files: Vec<String> =
        std::fs::read_to_string(list)?.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_owned).collect();
    files.sort();
    let mut remaining = 0;
    for (index, relative) in files.iter().enumerate() {
        if index % shard.1.max(1) != shard.0 {
            continue;
        }
        let relative = relative.trim_start_matches("./").replace('\\', "/");
        let key = key_of(&relative);
        if done.contains(&key) {
            continue;
        }
        if seconds > 0.0 && started.elapsed().as_secs_f64() > seconds {
            remaining += 1;
            continue;
        }
        // A file that was started but never finished (killed for time or
        // memory) is recorded as such instead of being tried again.
        let started_path = format!("{out}.started");
        let started_keys = std::fs::read_to_string(&started_path).unwrap_or_default();
        if started_keys.lines().any(|line| line == key) {
            writeln!(output, "{}", serde_json::to_string(&json!({"key": key, "killed": true})).expect("JSON"))?;
            continue;
        }
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&started_path)?
            .write_all(format!("{key}\n").as_bytes())?;
        let value = match measure(&format!("{root}/{relative}")) {
            Ok(Some(value)) => value,
            Ok(None) => json!({"skip": true}),
            Err(error) => json!({"error": error.to_string()}),
        };
        let mut line = json!({"key": key});
        line.as_object_mut().unwrap().extend(value.as_object().cloned().unwrap_or_default());
        writeln!(output, "{}", serde_json::to_string(&line).expect("JSON"))?;
        output.flush()?;
    }
    eprintln!("remaining {remaining}");
    Ok(())
}

fn measure(path: &str) -> Result<Option<serde_json::Value>> {
    let mut file = ClipFile::open(path)?;
    let layers: Vec<_> = file.layers.flatten().into_iter().map(|(layer, _)| layer.clone()).collect();
    let mut vectors = Vec::new();
    let mut vectors_stored = 0;
    let mut texts = Vec::new();
    let mut texts_stored = 0;
    let mut balloon_texts = 0;
    for layer in layers.iter().filter(|l| l.visible) {
        let stored = matches!(file.layer_pixels(layer), Ok(Some(_)));
        match layer.kind {
            LayerKind::Vector if stored => vectors_stored += 1,
            LayerKind::Vector => vectors.push(layer.main_id),
            LayerKind::Text => {
                texts_stored += stored as usize;
                if !csp_format::stroke::layer_vectors(&file, layer)?.is_empty() {
                    balloon_texts += 1;
                }
                texts.push(layer.main_id);
            }
            _ => {}
        }
    }
    if vectors.is_empty() && texts.is_empty() {
        return Ok(None);
    }
    let Some((base, report, preview)) = render_like_preview(&file)? else { return Ok(None) };
    let without = |file: &mut ClipFile, ids: &[i64]| -> Result<RgbaImage> {
        for id in ids {
            file.set_layer_visible(*id, false)?;
        }
        let image = render_like_preview(file)?.expect("preview").0;
        for id in ids {
            file.set_layer_visible(*id, true)?;
        }
        Ok(image)
    };
    let no_vector = if vectors.is_empty() { None } else { Some(without(&mut file, &vectors)?) };
    let no_text = if texts.is_empty() { None } else { Some(without(&mut file, &texts)?) };
    // Squared error of the full render, split by which hidden kind changes
    // the pixel (vector first), and the error left elsewhere.
    let (mut in_vector, mut in_text, mut elsewhere) = (0f64, 0f64, 0f64);
    let (mut vector_px, mut text_px) = (0usize, 0usize);
    for y in 0..base.height {
        for x in 0..base.width {
            let (b, p) = (base.pixel(x, y), preview.pixel(x, y));
            let e = squared(b, p);
            if no_vector.as_ref().is_some_and(|i| i.pixel(x, y) != b) {
                in_vector += e;
                vector_px += 1;
            } else if no_text.as_ref().is_some_and(|i| i.pixel(x, y) != b) {
                in_text += e;
                text_px += 1;
            } else {
                elsewhere += e;
            }
        }
    }
    let n = (base.width as f64 * base.height as f64 * 3.0).max(1.0);
    Ok(Some(json!({
        "psnr": psnr(&base, &preview),
        "psnr_no_vector": no_vector.as_ref().map(|i| psnr(i, &preview)),
        "psnr_no_text": no_text.as_ref().map(|i| psnr(i, &preview)),
        "mse": (in_vector + in_text + elsewhere) / n,
        "mse_vector": in_vector / n,
        "mse_text": in_text / n,
        "mse_else": elsewhere / n,
        "vector_px": vector_px,
        "text_px": text_px,
        "px": base.width as usize * base.height as usize,
        "vectors_drawn": vectors.len(),
        "vectors_stored": vectors_stored,
        "texts": texts.len(),
        "texts_stored": texts_stored,
        "balloon_texts": balloon_texts,
        "without_pixels": report.without_pixels,
        "effects_skipped": report.effects_skipped,
    })))
}
