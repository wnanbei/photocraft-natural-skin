//! Reproducible synthetic quality acceptance. No photographs or biometric data.
use crate::{Frame, Result, color::ColorPipeline, processing};
use serde_json::{Value, json};
use std::path::Path;
const COLOURS: [[f32; 3]; 6] = [
    [0.82, 0.62, 0.49],
    [0.65, 0.43, 0.31],
    [0.36, 0.23, 0.17],
    [0.90, 0.75, 0.65],
    [0.50, 0.30, 0.21],
    [0.22, 0.145, 0.105],
];
fn source(width: u32, height: u32) -> Frame {
    let (w, h) = (width as usize, height as usize);
    let mut pixels = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let (cw, ch) = (w / 3, h / 2);
            let index = (y / ch).min(1) * 3 + (x / cw).min(2);
            let (lx, ly) = (x % cw, y % ch);
            let p = if lx < 8 || ly < 8 || lx >= cw - 8 || ly >= ch - 8 {
                [0.08, 0.25, 0.85]
            } else if lx == cw / 2 {
                [0.02, 0.02, 0.02]
            } else {
                let noise = (((x * 17 + y * 31) % 37) as f32 / 36.0 - 0.5) * 0.04;
                COLOURS[index].map(|c| c + noise)
            };
            pixels.extend_from_slice(&[p[0], p[1], p[2], if x < 4 { 0.0 } else { 1.0 }]);
        }
    }
    Frame {
        width,
        height,
        channels: 4,
        depth: 32,
        pixels,
        icc: Some(
            moxcms::ColorProfile::new_srgb()
                .encode()
                .expect("built-in sRGB"),
        ),
        resolution: Some((2, [300, 1], [300, 1])),
    }
}
fn noise_error(frame: &Frame, index: usize) -> f64 {
    let (w, h) = (frame.width as usize, frame.height as usize);
    let (cw, ch) = (w / 3, h / 2);
    let mut sum = 0.0;
    let mut count = 0;
    for ly in 24..ch - 24 {
        for lx in 24..cw - 24 {
            if lx.abs_diff(cw / 2) < 24 {
                continue;
            }
            let (x, y) = ((index % 3) * cw + lx, (index / 3) * ch + ly);
            for (c, base) in COLOURS[index].iter().enumerate() {
                sum += (frame.pixels[(y * w + x) * 4 + c] - base).abs() as f64;
                count += 1;
            }
        }
    }
    sum / count as f64
}
pub fn run(directory: &Path) -> Result<Value> {
    std::fs::create_dir_all(directory)?;
    let frame = source(1536, 1024);
    let cms = ColorPipeline::new(frame.icc.as_deref())?;
    frame.write(&directory.join("synthetic-original.tiff"))?;
    let variants = [
        ("smooth", json!({"Strength":100,"Radius":6,"Texture":0})),
        ("texture", json!({"Strength":100,"Radius":6,"Texture":100})),
        (
            "tone",
            json!({"Strength":0,"skinBrightening":30,"skinRosiness":20}),
        ),
        ("mask", json!({"Show_Mask":true})),
    ];
    let mut reports = Vec::new();
    for (name, parameters) in variants {
        let (pixels, report) = processing::process(&frame, &parameters, &cms, |_| true)?;
        let mut result = frame.clone();
        result.pixels = pixels;
        // Mask is a diagnostic sRGB image, not source-profile image data.
        result.write(&directory.join(format!("synthetic-{name}.tiff")))?;
        let mut samples = Vec::new();
        for (i, base) in COLOURS.iter().enumerate() {
            samples.push(json!({"rgb":base,"sourceNoiseMae":noise_error(&frame,i),"resultNoiseMae":noise_error(&result,i)}));
        }
        let mut protected_max = 0.0f32;
        let mut alpha_max = 0.0f32;
        if name != "mask" {
            for (a, b) in frame
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .zip(result.pixels.as_chunks::<4>().0.iter())
            {
                alpha_max = alpha_max.max((a[3] - b[3]).abs());
                if a[2] > 0.8 || a[3] == 0.0 {
                    for c in 0..3 {
                        protected_max = protected_max.max((a[c] - b[c]).abs());
                    }
                }
            }
        }
        reports.push(json!({"variant":name,"parameters":parameters,"patches":samples,
            "protectedRgbMaxDifference":protected_max,"alphaMaxDifference":alpha_max,"scratchBytes":report.scratch_bytes}));
    }
    let report = json!({"source":"generated synthetic patches; not portrait acceptance","width":frame.width,"height":frame.height,
        "profile":"sRGB","variants":reports,"limitations":["No human perceptual or portrait acceptance","Patch noise MAE is a diagnostic, not a skin-quality score"]});
    use std::io::Write;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("report.json"))?
        .write_all(&serde_json::to_vec_pretty(&report)?)?;
    Ok(report)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthetic_skin_shades_smooth_noise_and_texture_control_retains_more_detail() {
        let frame = source(768, 512);
        let cms = ColorPipeline::new(frame.icc.as_deref()).unwrap();
        let mut smooth = frame.clone();
        smooth.pixels =
            processing::process(&frame, &json!({"Strength":100,"Texture":0}), &cms, |_| true)
                .unwrap()
                .0;
        let mut texture = frame.clone();
        texture.pixels =
            processing::process(&frame, &json!({"Strength":100,"Texture":100}), &cms, |_| {
                true
            })
            .unwrap()
            .0;
        for i in 0..6 {
            let original = noise_error(&frame, i);
            let low = noise_error(&smooth, i);
            let high = noise_error(&texture, i);
            assert!(low < original * 0.95, "shade {i}: {low} vs {original}");
            assert!(high > low, "texture shade {i}: {high} vs {low}");
        }
        for (a, b) in frame
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .zip(smooth.pixels.as_chunks::<4>().0.iter())
        {
            assert_eq!(a[3].to_bits(), b[3].to_bits());
            if a[2] > 0.8 || a[3] == 0.0 {
                assert_eq!(a, b);
            }
            if a[..3] == [0.02, 0.02, 0.02] {
                assert!(
                    a[..3]
                        .iter()
                        .zip(b)
                        .all(|(a, b)| (a - b).abs() < 1.0 / 1024.0)
                );
            }
        }
    }
}
