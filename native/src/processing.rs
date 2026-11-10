//! Immutable-source, bounded tile processing shared by export and detail preview.
use crate::{Frame, Result, color::ColorPipeline};
use natural_skin_core::{NativeProgress, NativeStage, filter_halo, retouch_native};
use serde_json::Value;

pub const TILE: usize = 256;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crop {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}
impl Crop {
    pub fn whole(frame: &Frame) -> Self {
        Self {
            x: 0,
            y: 0,
            width: frame.width as usize,
            height: frame.height as usize,
        }
    }
    fn validate(self, frame: &Frame) -> Result<()> {
        if self.width == 0
            || self.height == 0
            || self
                .x
                .checked_add(self.width)
                .is_none_or(|x| x > frame.width as usize)
            || self
                .y
                .checked_add(self.height)
                .is_none_or(|y| y > frame.height as usize)
        {
            return Err("Preview region is outside the source image".into());
        }
        Ok(())
    }
}
#[derive(Default, Clone, Copy, Debug)]
pub struct ProcessingReport {
    pub tiles: usize,
    pub scratch_bytes: usize,
    /// Conservative live tile-buffer bound, excluding immutable document, full
    /// output, preview cache, ICC executor internals and file encode/decode.
    pub tile_buffer_bytes: usize,
}
pub fn copy_crop(frame: &Frame, crop: Crop) -> Result<Vec<f32>> {
    crop.validate(frame)?;
    let mut output = Vec::with_capacity(crop.width * crop.height * frame.channels);
    for y in crop.y..crop.y + crop.height {
        let offset = (y * frame.width as usize + crop.x) * frame.channels;
        output.extend_from_slice(&frame.pixels[offset..offset + crop.width * frame.channels]);
    }
    Ok(output)
}
/// Exact full-resolution neighbourhood. Only the interior is returned, so no
/// artificial crop boundary can affect detail previews or exported tile seams.
pub fn render_crop(
    frame: &Frame,
    crop: Crop,
    parameters: &Value,
    cms: &ColorPipeline,
    mut progress: impl FnMut(NativeProgress) -> bool,
) -> Result<(Vec<f32>, ProcessingReport)> {
    crop.validate(frame)?;
    if crop.width * crop.height > 2_000_000 {
        return Err("Detail preview is limited to 2 MP".into());
    }
    if !progress(NativeProgress {
        stage: NativeStage::Analyze,
        completed: 0,
        total: 0,
    }) {
        return Err("Cancelled".into());
    }
    let bytes = serde_json::to_vec(parameters)?;
    let mask = parameters["Show_Mask"].as_bool().unwrap_or(false);
    let halo = filter_halo(&bytes);
    let x = crop.x.saturating_sub(halo);
    let y = crop.y.saturating_sub(halo);
    let context = Crop {
        x,
        y,
        width: (crop.x + crop.width + halo).min(frame.width as usize) - x,
        height: (crop.y + crop.height + halo).min(frame.height as usize) - y,
    };
    let source = copy_crop(frame, context)?;
    let mut working = cms.to_working(&source, frame.channels)?;
    let baseline = if cms.is_tagged() && !mask {
        working.clone()
    } else {
        Vec::new()
    };
    let report = retouch_native(
        &mut working,
        context.width,
        context.height,
        frame.channels,
        &bytes,
        &mut progress,
    )
    .map_err(|e| format!("Processing failed: {e:?}"))?;
    if cms.is_tagged() && !mask {
        working = cms.to_source(&source, &baseline, &working, frame.channels)?;
    }
    let mut output = Vec::with_capacity(crop.width * crop.height * frame.channels);
    for row in crop.y - y..crop.y - y + crop.height {
        let offset = (row * context.width + crop.x - x) * frame.channels;
        output.extend_from_slice(&working[offset..offset + crop.width * frame.channels]);
    }
    Ok((
        output,
        ProcessingReport {
            tiles: 1,
            scratch_bytes: report.scratch_bytes,
            tile_buffer_bytes: report.scratch_bytes
                + source.len() * 4 * if cms.is_tagged() { 5 } else { 2 }
                + crop.width * crop.height * frame.channels * 4,
        },
    ))
}
pub fn process(
    frame: &Frame,
    parameters: &Value,
    cms: &ColorPipeline,
    mut progress: impl FnMut(NativeProgress) -> bool,
) -> Result<(Vec<f32>, ProcessingReport)> {
    let (w, h) = (frame.width as usize, frame.height as usize);
    if w == 0
        || h == 0
        || w.checked_mul(h).is_none_or(|n| n > 36_000_000)
        || !matches!(frame.channels, 3 | 4)
        || frame.pixels.len() != w * h * frame.channels
    {
        return Err("Invalid image layout or 36 MP limit exceeded".into());
    }
    let count = w.div_ceil(TILE) * h.div_ceil(TILE);
    if !progress(NativeProgress {
        stage: NativeStage::Filter,
        completed: 0,
        total: count,
    }) {
        return Err("Cancelled".into());
    }
    let mut output = Vec::new();
    output.try_reserve_exact(frame.pixels.len())?;
    for (i, chunk) in frame.pixels.chunks(262_144).enumerate() {
        if !progress(NativeProgress {
            stage: NativeStage::Analyze,
            completed: i * 262_144,
            total: frame.pixels.len(),
        }) {
            return Err("Cancelled".into());
        }
        output.extend_from_slice(chunk);
    }
    let mut report = ProcessingReport::default();
    for y in (0..h).step_by(TILE) {
        for x in (0..w).step_by(TILE) {
            let crop = Crop {
                x,
                y,
                width: TILE.min(w - x),
                height: TILE.min(h - y),
            };
            let (tile, r) = render_crop(frame, crop, parameters, cms, |_| {
                progress(NativeProgress {
                    stage: NativeStage::Filter,
                    completed: report.tiles,
                    total: count,
                })
            })?;
            for row in 0..crop.height {
                let offset = ((y + row) * w + x) * frame.channels;
                output[offset..offset + crop.width * frame.channels].copy_from_slice(
                    &tile[row * crop.width * frame.channels
                        ..(row + 1) * crop.width * frame.channels],
                );
            }
            report.tiles += 1;
            report.scratch_bytes = report.scratch_bytes.max(r.scratch_bytes);
            report.tile_buffer_bytes = report.tile_buffer_bytes.max(r.tile_buffer_bytes);
            if !progress(NativeProgress {
                stage: NativeStage::Filter,
                completed: report.tiles,
                total: count,
            }) {
                return Err("Cancelled".into());
            }
        }
    }
    Ok((output, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;
    use serde_json::json;
    #[test]
    fn tiled_and_detail_match_full_kernel_at_edges_seams_and_max_radius() {
        for channels in [3, 4] {
            let mut frame = fixture(541, 283, 32, None);
            if channels == 3 {
                frame.pixels = frame
                    .pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|p| p[..3].iter().copied())
                    .collect();
                frame.channels = 3;
            }
            let cms = ColorPipeline::new(None).unwrap();
            for params in [
                json!({"Radius":20,"Skin_Only":false,"skinBrightening":25,"skinRosiness":-30}),
                json!({"Radius":6,"Texture":0}),
                json!({"Strength":0,"skinBrightening":30}),
                json!({"Show_Mask":true}),
            ] {
                let mut expected = frame.pixels.clone();
                retouch_native(
                    &mut expected,
                    541,
                    283,
                    channels,
                    &serde_json::to_vec(&params).unwrap(),
                    |_| true,
                )
                .unwrap();
                let (actual, report) = process(&frame, &params, &cms, |_| true).unwrap();
                let difference = expected
                    .iter()
                    .zip(&actual)
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0f32, f32::max);
                assert!(
                    difference < 0.000002,
                    "tile seam error {difference}, {params}"
                );
                assert!(report.scratch_bytes <= 336 * 336 * 7 * 4);
                for crop in [
                    Crop {
                        x: 0,
                        y: 0,
                        width: 75,
                        height: 80,
                    },
                    Crop {
                        x: 245,
                        y: 230,
                        width: 150,
                        height: 53,
                    },
                    Crop {
                        x: 500,
                        y: 0,
                        width: 41,
                        height: 60,
                    },
                ] {
                    let (detail, _) = render_crop(&frame, crop, &params, &cms, |_| true).unwrap();
                    let mut reference = frame.clone();
                    reference.pixels = expected.clone();
                    assert!(
                        detail
                            .iter()
                            .zip(copy_crop(&reference, crop).unwrap())
                            .all(|(a, b)| (a - b).abs() < 0.000002)
                    );
                }
            }
        }
    }
    #[test]
    fn cancelled_tiles_leave_source_unchanged_and_no_result() {
        let frame = fixture(600, 400, 32, None);
        let source = frame.pixels.clone();
        let cms = ColorPipeline::new(None).unwrap();
        let mut checks = 0;
        let result = process(&frame, &json!({}), &cms, |_| {
            checks += 1;
            checks < 15
        });
        assert!(result.is_err());
        assert_eq!(frame.pixels, source);
    }
    #[test]
    fn icc_identity_and_alpha_are_bit_exact() {
        let icc = moxcms::ColorProfile::new_adobe_rgb().encode().unwrap();
        let frame = fixture(70, 55, 32, Some(icc));
        let cms = ColorPipeline::new(frame.icc.as_deref()).unwrap();
        let (identity, _) = process(&frame, &json!({"Strength":0}), &cms, |_| true).unwrap();
        assert!(
            frame
                .pixels
                .iter()
                .zip(&identity)
                .all(|(a, b)| a.to_bits() == b.to_bits())
        );
        let (retouched, _) = process(&frame, &json!({}), &cms, |_| true).unwrap();
        assert_ne!(retouched, identity);
        for (a, b) in frame
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .zip(retouched.as_chunks::<4>().0.iter())
        {
            assert_eq!(a[3].to_bits(), b[3].to_bits());
            if a[3] == 0.0 || a[..3].iter().any(|v| !(0.0..=1.0).contains(v)) {
                assert_eq!(a, b);
            }
        }
    }
}
