//! SDR RGB ICC conversion. The retouching kernel always works in encoded sRGB.
//! Alpha, hidden RGB, invalid/HDR samples and unchanged pixels bypass conversion
//! on export. A source profile is never replaced by a different profile's label.
use crate::Result;
use moxcms::{
    ColorProfile, DataColorSpace, Layout, ParsingOptions, RenderingIntent, TransformF32Executor,
    TransformOptions,
};
use std::sync::Arc;

pub struct ColorPipeline {
    forward: Option<Arc<TransformF32Executor>>,
    reverse: Option<Arc<TransformF32Executor>>,
    pub description: String,
}
impl ColorPipeline {
    pub fn new(icc: Option<&[u8]>) -> Result<Self> {
        let Some(bytes) = icc else {
            return Ok(Self {
                forward: None,
                reverse: None,
                description: "Untagged RGB · assumed sRGB".into(),
            });
        };
        let profile = ColorProfile::new_from_slice_with_options(bytes, ParsingOptions {
            max_profile_size: 4 * 1024 * 1024,
            max_allowed_clut_size: 1_000_000,
            max_allowed_trc_size: 40_000,
        }).map_err(|e| format!("Invalid or unsupported ICC profile: {e}. Convert the image to sRGB in PhotoCraft first."))?;
        if profile.color_space != DataColorSpace::Rgb
            || !profile.is_matrix_shaper()
            || profile.lut_a_to_b_perceptual.is_some()
            || profile.lut_a_to_b_colorimetric.is_some()
            || profile.lut_a_to_b_saturation.is_some()
            || profile.lut_b_to_a_perceptual.is_some()
            || profile.lut_b_to_a_colorimetric.is_some()
            || profile.lut_b_to_a_saturation.is_some()
            || profile
                .cicp
                .as_ref()
                .is_some_and(|c| matches!(c.transfer_characteristics as u8, 16 | 18))
        {
            return Err("Only SDR RGB matrix/TRC ICC profiles are supported. Convert to sRGB in PhotoCraft first; HDR/LUT profiles are not assumed to be sRGB.".into());
        }
        let srgb = ColorProfile::new_srgb();
        let options = TransformOptions {
            rendering_intent: RenderingIntent::RelativeColorimetric,
            prefer_fixed_point: false,
            allow_extended_range_rgb_xyz: true,
            ..Default::default()
        };
        Ok(Self {
            forward: Some(profile.create_transform_f32(
                Layout::Rgb,
                &srgb,
                Layout::Rgb,
                options,
            )?),
            reverse: Some(srgb.create_transform_f32(
                Layout::Rgb,
                &profile,
                Layout::Rgb,
                options,
            )?),
            description: "ICC RGB ↔ sRGB · source profile retained".into(),
        })
    }
    pub fn is_tagged(&self) -> bool {
        self.forward.is_some()
    }
    pub fn to_working(&self, source: &[f32], channels: usize) -> Result<Vec<f32>> {
        let mut output = source.to_vec();
        if let Some(transform) = &self.forward {
            convert(transform.as_ref(), source, &mut output, channels, false)?;
        }
        Ok(output)
    }
    /// Restore source encoding only where the kernel changed RGB. No-op is bit exact.
    pub fn to_source(
        &self,
        source: &[f32],
        before: &[f32],
        working: &[f32],
        channels: usize,
    ) -> Result<Vec<f32>> {
        let Some(transform) = &self.reverse else {
            return Ok(working.to_vec());
        };
        let mut output = working.to_vec();
        convert(transform.as_ref(), working, &mut output, channels, true)?;
        for (((out, original), baseline), result) in output
            .chunks_exact_mut(channels)
            .zip(source.chunks_exact(channels))
            .zip(before.chunks_exact(channels))
            .zip(working.chunks_exact(channels))
        {
            if !editable(original, channels)
                || baseline[..3]
                    .iter()
                    .zip(&result[..3])
                    .all(|(a, b)| a.to_bits() == b.to_bits())
            {
                out.copy_from_slice(original);
            } else {
                for sample in &mut out[..3] {
                    *sample = sample.clamp(0.0, 1.0);
                }
                if channels == 4 {
                    out[3] = original[3];
                }
            }
        }
        Ok(output)
    }
}
fn editable(p: &[f32], channels: usize) -> bool {
    p[..3]
        .iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        && (channels == 3 || (p[3].is_finite() && p[3] > 0.0 && p[3] <= 1.0))
}
fn convertible(p: &[f32], channels: usize, extended: bool) -> bool {
    p[..3]
        .iter()
        .all(|v| v.is_finite() && (extended || (0.0..=1.0).contains(v)))
        && (channels == 3 || (p[3].is_finite() && p[3] > 0.0 && p[3] <= 1.0))
}
fn convert(
    transform: &TransformF32Executor,
    source: &[f32],
    output: &mut [f32],
    channels: usize,
    extended: bool,
) -> Result<()> {
    // Sanitize before entering a LUT implementation; never pass NaN/Inf to CMS.
    const CHUNK: usize = 4096;
    for (input, out) in source
        .chunks(CHUNK * channels)
        .zip(output.chunks_mut(CHUNK * channels))
    {
        let mut rgb = Vec::with_capacity(input.len() / channels * 3);
        for p in input.chunks_exact(channels) {
            rgb.extend_from_slice(if convertible(p, channels, extended) {
                &p[..3]
            } else {
                &[0.0; 3]
            });
        }
        let mut mapped = vec![0.0; rgb.len()];
        transform.transform(&rgb, &mut mapped)?;
        for ((p, q), rgb) in input
            .chunks_exact(channels)
            .zip(out.chunks_exact_mut(channels))
            .zip(mapped.as_chunks::<3>().0.iter())
        {
            if convertible(p, channels, extended) {
                if !rgb.iter().all(|v| v.is_finite()) {
                    return Err("ICC conversion produced non-finite RGB".into());
                }
                q[..3].copy_from_slice(rgb);
            }
        }
    }
    Ok(())
}

/// Optional explicit destination profile for the image canvas. This is a manual
/// output selection, not automatic monitor discovery or HDR display support.
pub struct DisplayTransform(Option<Arc<TransformF32Executor>>);
impl DisplayTransform {
    pub fn new(icc: Option<&[u8]>) -> Result<Self> {
        let Some(bytes) = icc else {
            return Ok(Self(None));
        };
        // Apply the same bounded SDR RGB validation used for input profiles.
        ColorPipeline::new(Some(bytes)).map_err(|_| "Unsupported display ICC profile")?;
        let profile = ColorProfile::new_from_slice(bytes)?;
        let options = TransformOptions {
            rendering_intent: RenderingIntent::RelativeColorimetric,
            prefer_fixed_point: false,
            allow_extended_range_rgb_xyz: true,
            ..Default::default()
        };
        Ok(Self(Some(ColorProfile::new_srgb().create_transform_f32(
            Layout::Rgb,
            &profile,
            Layout::Rgb,
            options,
        )?)))
    }
    pub fn from_path(path: Option<&std::path::Path>) -> Result<Self> {
        match path {
            None => Self::new(None),
            Some(path) => {
                let file = std::fs::File::open(path)?;
                if !file.metadata()?.is_file() || file.metadata()?.len() > 4 * 1024 * 1024 {
                    return Err("Display ICC profile must be a regular file up to 4 MiB".into());
                }
                use std::io::Read;
                let mut bytes = Vec::new();
                file.take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
                Self::new(Some(&bytes))
            }
        }
    }
    pub fn convert(&self, pixels: &[f32], channels: usize) -> Result<Vec<f32>> {
        let mut output = pixels.to_vec();
        if let Some(transform) = &self.0 {
            convert(transform.as_ref(), pixels, &mut output, channels, true)?;
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn p3_srgb_conversion_matches_reference_and_noop_preserves_payload() {
        let icc = ColorProfile::new_display_p3().encode().unwrap();
        let cms = ColorPipeline::new(Some(&icc)).unwrap();
        let source = [
            0.8,
            0.5,
            0.3,
            1.0,
            0.2,
            0.4,
            0.9,
            0.0,
            1.7,
            0.2,
            0.3,
            1.0,
            f32::from_bits(0x7fc01234),
            0.4,
            0.2,
            1.0,
        ];
        let working = cms.to_working(&source, 4).unwrap();
        // Display P3 D65 -> linear sRGB matrix -> IEC sRGB transfer function.
        fn linear(v: f64) -> f64 {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        }
        fn encoded(v: f64) -> f64 {
            if v <= 0.0031308 {
                12.92 * v
            } else {
                1.055 * v.powf(1.0 / 2.4) - 0.055
            }
        }
        let p = [linear(0.8), linear(0.5), linear(0.3)];
        let matrix = [
            [1.224745, -0.224904, 0.0],
            [-0.042058, 1.042081, 0.0],
            [-0.019642, -0.078655, 1.098537],
        ];
        for c in 0..3 {
            let expected = encoded((0..3).map(|i| matrix[c][i] * p[i]).sum());
            assert!(
                (working[c] as f64 - expected).abs() < 0.001,
                "{working:?} expected {expected}"
            );
        }
        let back = cms.to_source(&source, &working, &working, 4).unwrap();
        assert!(
            source
                .iter()
                .zip(back)
                .all(|(a, b)| a.to_bits() == b.to_bits())
        );
    }
    #[test]
    fn manual_display_profile_maps_srgb_to_monitor_including_extended_gamut() {
        let icc = ColorProfile::new_display_p3().encode().unwrap();
        let cms = ColorPipeline::new(Some(&icc)).unwrap();
        let source = [1.0, 0.0, 0.0, 1.0, 0.65, 0.45, 0.3, 0.7];
        let working = cms.to_working(&source, 4).unwrap();
        let display = DisplayTransform::new(Some(&icc)).unwrap();
        let output = display.convert(&working, 4).unwrap();
        for (a, b) in source.iter().zip(&output) {
            assert!((a - b).abs() < 0.001, "{working:?} -> {output:?}");
        }
    }
    #[test]
    fn invalid_and_non_rgb_profiles_are_rejected() {
        assert!(ColorPipeline::new(Some(b"invalid ICC")).is_err());
        assert!(
            ColorPipeline::new(Some(
                &ColorProfile::new_gray_with_gamma(2.2).encode().unwrap()
            ))
            .is_err()
        );
        assert!(
            ColorPipeline::new(Some(&ColorProfile::new_display_p3_pq().encode().unwrap())).is_err()
        );
    }
}
