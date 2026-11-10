//! Native execution entry point. Callers retain the source and discard working
//! pixels on cancellation; no result is published until this returns success.
use crate::{PLANES, Settings, adjust_skin_tone, processing_region, regions, show_mask};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeStage {
    Analyze,
    Filter,
    Tone,
    Mask,
    PrepareCache,
    Mix,
    Complete,
}

/// Preview source ownership makes cache identity explicit: a new source/scale/
/// document revision requires a new session. Cache is capped at 2 MP.
pub struct NativePreview {
    source: Vec<f32>,
    width: usize,
    height: usize,
    channels: usize,
    basis: Vec<f32>,
    key: Option<(usize, u32)>,
}

#[derive(Clone, Copy, Debug)]
pub struct PreviewReport {
    pub cache_hit: bool,
    pub cached_bytes: usize,
}

impl NativePreview {
    pub fn new(
        source: Vec<f32>,
        width: usize,
        height: usize,
        channels: usize,
    ) -> Result<Self, NativeError> {
        let n = width.checked_mul(height).ok_or(NativeError::Layout)?;
        if n == 0 || !matches!(channels, 3 | 4) || n.checked_mul(channels) != Some(source.len()) {
            return Err(NativeError::Layout);
        }
        if n > 2_000_000 {
            return Err(NativeError::TooLarge);
        }
        Ok(Self {
            source,
            width,
            height,
            channels,
            basis: Vec::new(),
            key: None,
        })
    }
    pub fn source(&self) -> &[f32] {
        &self.source
    }
    pub fn render(
        &mut self,
        parameters: &[u8],
        mut progress: impl FnMut(NativeProgress) -> bool,
    ) -> Result<(Vec<f32>, PreviewReport), NativeError> {
        let s = Settings::from_json(parameters);
        let mut output = self.source.clone();
        if s.show_mask || s.strength == 0.0 {
            retouch_native(
                &mut output,
                self.width,
                self.height,
                self.channels,
                parameters,
                progress,
            )?;
            return Ok((
                output,
                PreviewReport {
                    cache_hit: false,
                    cached_bytes: self.basis.len() * 4,
                },
            ));
        }
        let n = self.width * self.height;
        let key = (s.radius, s.edge.to_bits());
        let hit = self.key == Some(key);
        if !hit {
            self.key = None;
            if !progress(NativeProgress {
                stage: NativeStage::PrepareCache,
                completed: 0,
                total: 1,
            }) {
                return Err(NativeError::Cancelled);
            }
            self.basis
                .try_reserve(n * 6 - self.basis.len())
                .map_err(|_| NativeError::OutOfMemory)?;
            self.basis.resize(n * 6, 0.0);
            self.basis.fill(0.0);
            let mut scratch = Vec::new();
            scratch
                .try_reserve_exact(n * PLANES)
                .map_err(|_| NativeError::OutOfMemory)?;
            scratch.resize(n * PLANES, 0.0);
            let settings = Settings {
                strength: 1.0,
                texture: 1.0,
                skin_only: false,
                show_mask: false,
                brightening: 0.0,
                rosiness: 0.0,
                ..s
            };
            let region = crate::Region {
                x: 0,
                y: 0,
                width: self.width,
                height: self.height,
            };
            let code = crate::process_region_inner::<false>(
                &mut self.source,
                self.width,
                self.channels,
                self.channels == 4,
                settings,
                region,
                crate::FilterBuffers {
                    scratch: &mut scratch,
                    basis: Some(&mut self.basis),
                    check: &mut || {
                        progress(NativeProgress {
                            stage: NativeStage::PrepareCache,
                            completed: 0,
                            total: 0,
                        })
                    },
                },
            );
            if code == 2 {
                return Err(NativeError::Cancelled);
            }
            if code != 0 {
                return Err(NativeError::Kernel);
            }
            self.key = Some(key);
        }
        for (i, (pixel, base)) in output
            .chunks_exact_mut(self.channels)
            .zip(self.basis.as_chunks::<6>().0.iter())
            .enumerate()
        {
            if i % (self.width * 32) == 0
                && !progress(NativeProgress {
                    stage: NativeStage::Mix,
                    completed: i,
                    total: n,
                })
            {
                return Err(NativeError::Cancelled);
            }
            if !crate::valid_rgb(pixel)
                || (self.channels == 4 && (!pixel[3].is_finite() || pixel[3] <= 0.0))
            {
                continue;
            }
            let mask = if s.skin_only {
                crate::skin_gate(pixel[0], pixel[1], pixel[2], s.skin_range)
            } else {
                1.0
            };
            if mask == 0.0 {
                continue;
            }
            for c in 0..3 {
                let amount = if s.texture == 0.0 {
                    s.strength * mask
                } else {
                    s.strength * mask * (1.0 - s.texture * base[3 + c] * 0.85)
                };
                pixel[c] = (pixel[c] + base[c] * amount).clamp(0.0, 1.0);
            }
            adjust_skin_tone(pixel, self.channels, self.channels == 4, s, Some(&[mask]));
        }
        if !progress(NativeProgress {
            stage: NativeStage::Complete,
            completed: 1,
            total: 1,
        }) {
            return Err(NativeError::Cancelled);
        }
        Ok((
            output,
            PreviewReport {
                cache_hit: hit,
                cached_bytes: self.basis.len() * 4,
            },
        ))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NativeProgress {
    pub stage: NativeStage,
    pub completed: usize,
    pub total: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct NativeReport {
    pub regions: usize,
    pub scratch_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeError {
    Layout,
    TooLarge,
    OutOfMemory,
    Cancelled,
    Kernel,
}

/// Retouch normalized RGB/RGBA floats using the existing parameter names/ranges.
/// `progress` returns false to cancel; this may leave the private working buffer
/// partially processed. The source image must be held separately by the caller.
/// Initial native proof limit: 36 MP; allocation failures are returned as errors.
pub fn retouch_native(
    pixels: &mut [f32],
    width: usize,
    height: usize,
    channels: usize,
    parameters: &[u8],
    mut progress: impl FnMut(NativeProgress) -> bool,
) -> Result<NativeReport, NativeError> {
    let n = width.checked_mul(height).ok_or(NativeError::Layout)?;
    if width == 0
        || height == 0
        || !matches!(channels, 3 | 4)
        || n.checked_mul(channels) != Some(pixels.len())
    {
        return Err(NativeError::Layout);
    }
    if n > 36_000_000 {
        return Err(NativeError::TooLarge);
    }
    let settings = Settings::from_json(parameters);
    let alpha = channels == 4;
    let mut notify = |stage, completed, total| {
        if progress(NativeProgress {
            stage,
            completed,
            total,
        }) {
            Ok(())
        } else {
            Err(NativeError::Cancelled)
        }
    };
    notify(NativeStage::Analyze, 0, 1)?;
    let mut report = NativeReport {
        regions: 0,
        scratch_bytes: 0,
    };
    if settings.show_mask {
        notify(NativeStage::Mask, 0, 1)?;
        for (i, chunk) in pixels.chunks_mut(width * channels * 16).enumerate() {
            notify(NativeStage::Mask, (i * 16).min(height), height)?;
            show_mask(chunk, channels, alpha, settings);
        }
    } else if settings.strength == 0.0 {
        notify(NativeStage::Tone, 0, 1)?;
        for (i, chunk) in pixels.chunks_mut(width * channels * 16).enumerate() {
            notify(NativeStage::Tone, (i * 16).min(height), height)?;
            adjust_skin_tone(chunk, channels, alpha, settings, None);
        }
    } else if let Some(whole) = processing_region(pixels, width, height, channels, alpha, settings)
    {
        let plan = regions::plan(pixels, width, height, channels, alpha, settings, whole);
        let count = plan.rectangles().len();
        let samples = plan
            .max_area()
            .checked_mul(PLANES)
            .ok_or(NativeError::TooLarge)?;
        report = NativeReport {
            regions: count,
            scratch_bytes: samples.checked_mul(4).ok_or(NativeError::TooLarge)?,
        };
        let mut scratch = Vec::new();
        scratch
            .try_reserve_exact(samples)
            .map_err(|_| NativeError::OutOfMemory)?;
        scratch.resize(samples, 0.0);
        for (i, &region) in plan.rectangles().iter().enumerate() {
            notify(NativeStage::Filter, i, count)?;
            let len = region.width * region.height * PLANES;
            let code = crate::process_region_inner::<true>(
                pixels,
                width,
                channels,
                alpha,
                settings,
                region,
                crate::FilterBuffers {
                    scratch: &mut scratch[..len],
                    basis: None,
                    check: &mut || notify(NativeStage::Filter, i, count).is_ok(),
                },
            );
            if code == 2 {
                return Err(NativeError::Cancelled);
            }
            if code != 0 {
                return Err(NativeError::Kernel);
            }
            notify(NativeStage::Filter, i + 1, count)?;
        }
    }
    notify(NativeStage::Complete, 1, 1)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_inside_cache_does_not_publish_a_partial_basis() {
        let source = vec![0.5; 256 * 192 * 3];
        let mut preview = NativePreview::new(source.clone(), 256, 192, 3).unwrap();
        let mut checks = 0;
        assert!(matches!(
            preview.render(b"{}", |p| {
                if p.stage == NativeStage::PrepareCache {
                    checks += 1;
                }
                checks < 12
            }),
            Err(NativeError::Cancelled)
        ));
        assert_eq!(preview.source(), source);
        let (_, report) = preview.render(b"{}", |_| true).unwrap();
        assert!(!report.cache_hit);
        assert!(preview.render(b"{}", |_| true).unwrap().1.cache_hit);
    }
    #[test]
    fn cancellation_reaches_filter_rows_and_tone_rows() {
        for parameters in [b"{}".as_slice(), br#"{"Strength":0,"skinBrightening":20}"#] {
            let mut image = vec![0.55; 256 * 192 * 3];
            let mut count = 0;
            let result = retouch_native(&mut image, 256, 192, 3, parameters, |_| {
                count += 1;
                count < 10
            });
            assert!(matches!(result, Err(NativeError::Cancelled)));
        }
    }
    #[test]
    fn expanding_skin_range_reuses_basis_outside_the_previous_mask() {
        let (w, h) = (97, 79);
        let mut source = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let n = ((x * 17 + y * 31) % 19) as f32 * 0.001;
                let p = if x < 18 {
                    [0.64 + n, 0.44 + n, 0.33 + n, 1.0]
                } else if x > 76 {
                    [0.42 + n, 0.43 + n, 0.40 + n, 1.0]
                } else {
                    [0.1, 0.3, 0.9, 1.0]
                };
                source.extend_from_slice(&p);
            }
        }
        let mut preview = NativePreview::new(source.clone(), w, h, 4).unwrap();
        preview
            .render(br#"{"Strength":100,"Texture":0,"Skin_Range":0}"#, |_| true)
            .unwrap();
        let parameters = br#"{"Strength":100,"Texture":0,"Skin_Range":100}"#;
        let (actual, report) = preview.render(parameters, |_| true).unwrap();
        assert!(report.cache_hit);
        let mut expected = source;
        retouch_native(&mut expected, w, h, 4, parameters, |_| true).unwrap();
        for (a, b) in actual.iter().zip(expected) {
            assert!((a - b).abs() < 2e-6);
        }
    }
    #[test]
    fn preview_cache_reuses_only_filter_independent_controls() {
        let (w, h) = (97, 79);
        let mut source = Vec::new();
        for i in 0..w * h {
            let n = (i % 17) as f32 * 0.002;
            source.extend_from_slice(&[
                0.64 + n,
                0.44 + n,
                0.33 + n,
                if i % 19 == 0 { 0.0 } else { 0.5 },
            ]);
        }
        source[12] = f32::NAN;
        source[16] = 1.5;
        let mut preview = NativePreview::new(source.clone(), w, h, 4).unwrap();
        for (params, hit) in [
            ("{}", false),
            (
                r#"{"Strength":80,"Texture":0,"skinBrightening":50,"skinRosiness":20}"#,
                true,
            ),
            (r#"{"Skin_Range":0,"Skin_Only":false}"#, true),
            (r#"{"Radius":20}"#, false),
            (r#"{"Radius":20,"Edge_Protection":80}"#, false),
        ] {
            let mut expected = source.clone();
            retouch_native(&mut expected, w, h, 4, params.as_bytes(), |_| true).unwrap();
            let (actual, report) = preview.render(params.as_bytes(), |_| true).unwrap();
            assert_eq!(hit, report.cache_hit);
            for (a, b) in actual.iter().zip(&expected) {
                assert!(
                    a.to_bits() == b.to_bits() || (a - b).abs() < 2e-6,
                    "{a} != {b}"
                );
            }
            assert!(
                preview
                    .source
                    .iter()
                    .zip(&source)
                    .all(|(a, b)| a.to_bits() == b.to_bits())
            );
        }
        assert_eq!(
            preview.render(b"{}", |_| false).err(),
            Some(NativeError::Cancelled)
        );
    }
    #[test]
    fn cancellation_before_work_preserves_pixels() {
        let mut pixels = [0.64, 0.44, 0.33, 1.0].repeat(20 * 12);
        let before = pixels.clone();
        assert_eq!(
            retouch_native(&mut pixels, 20, 12, 4, b"{}", |_| false).err(),
            Some(NativeError::Cancelled)
        );
        assert_eq!(pixels, before);
    }
    #[test]
    fn native_entry_matches_the_existing_kernel() {
        let (w, h) = (73, 51);
        let mut pixels = Vec::new();
        for i in 0..w * h {
            let n = (i % 17) as f32 * 0.002;
            pixels.extend_from_slice(&[0.64 + n, 0.44 + n, 0.33 + n, 1.0]);
        }
        let mut expected = pixels.clone();
        let mut scratch = vec![0.0; w * h * PLANES];
        assert_eq!(
            crate::process(
                &mut expected,
                w,
                h,
                4,
                true,
                Settings::default(),
                &mut scratch
            ),
            0
        );
        let mut stages = Vec::new();
        retouch_native(&mut pixels, w, h, 4, b"{}", |p| {
            stages.push(p.stage);
            true
        })
        .unwrap();
        assert_eq!(pixels, expected);
        assert_eq!(stages.last(), Some(&NativeStage::Complete));
    }
}
