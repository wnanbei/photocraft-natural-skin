//! Natural Skin native retouching core and bounded preview cache.
//! Scalar self-guided filtering, alpha-weighted statistics, and a feathered skin-colour gate.
//! Image processing is independent of the editor bridge and file exchange.
#![forbid(unsafe_code)]

const MAX_RADIUS: usize = 20;
const PLANES: usize = 7;
mod native;
mod regions;
pub use native::{
    NativeError, NativePreview, NativeProgress, NativeReport, NativeStage, PreviewReport,
    retouch_native,
};

#[derive(Clone, Copy, Debug)]
struct Settings {
    strength: f32,
    radius: usize,
    edge: f32,
    texture: f32,
    skin_only: bool,
    skin_range: f32,
    show_mask: bool,
    brightening: f32,
    rosiness: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            strength: 0.45,
            radius: 6,
            edge: 0.70,
            texture: 0.60,
            skin_only: true,
            skin_range: 0.60,
            show_mask: false,
            brightening: 0.0,
            rosiness: 0.0,
        }
    }
}

// The host supplies schema-validated JSON. Match the complete quoted ASCII key,
// then read a JSON scalar; do not confuse Radius with geometry inside _image.
fn scalar<'a>(json: &'a [u8], key: &[u8]) -> Option<&'a str> {
    let start = json.windows(key.len()).position(|x| x == key)? + key.len();
    let mut i = start;
    while json.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }
    if json.get(i) != Some(&b':') {
        return None;
    }
    i += 1;
    while json.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }
    let begin = i;
    while json
        .get(i)
        .is_some_and(|b| !matches!(*b, b',' | b'}' | b' ' | b'\n' | b'\r' | b'\t'))
    {
        i += 1;
    }
    core::str::from_utf8(&json[begin..i]).ok()
}

fn number(json: &[u8], key: &[u8], default: f32) -> f32 {
    scalar(json, key)
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|x| x.is_finite())
        .unwrap_or(default)
}

impl Settings {
    fn from_json(json: &[u8]) -> Self {
        let d = Self::default();
        Self {
            strength: number(json, b"\"Strength\"", d.strength * 100.0).clamp(0.0, 100.0) * 0.01,
            radius: (number(json, b"\"Radius\"", d.radius as f32).clamp(1.0, MAX_RADIUS as f32)
                + 0.5) as usize,
            edge: number(json, b"\"Edge_Protection\"", d.edge * 100.0).clamp(0.0, 100.0) * 0.01,
            texture: number(json, b"\"Texture\"", d.texture * 100.0).clamp(0.0, 100.0) * 0.01,
            skin_only: scalar(json, b"\"Skin_Only\"").map_or(d.skin_only, |s| s != "false"),
            skin_range: number(json, b"\"Skin_Range\"", d.skin_range * 100.0).clamp(0.0, 100.0)
                * 0.01,
            show_mask: scalar(json, b"\"Show_Mask\"") == Some("true"),
            brightening: number(json, b"\"skinBrightening\"", 0.0).clamp(0.0, 100.0) * 0.01,
            rosiness: number(json, b"\"skinRosiness\"", 0.0).clamp(-100.0, 100.0) * 0.01,
        }
    }
}

fn smoothstep(lo: f32, hi: f32, value: f32) -> f32 {
    let t = ((value - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn skin_gate(r: f32, g: f32, b: f32, range: f32) -> f32 {
    // BT.601 chroma bands, feathered and deliberately broad across skin tones.
    // They can also match wood, sand and warm clothing: inspect Show_Mask/use a selection.
    let y = 0.299 * r + 0.587 * g + 0.114 * b;
    let cb = 0.5 - 0.168736 * r - 0.331264 * g + 0.5 * b;
    let cr = 0.5 + 0.5 * r - 0.418688 * g - 0.081312 * b;
    let spread = range * 0.055;
    // Most ordinary skin pixels sit on the flat part of all six feather ramps.
    // Return the exact plateau value without evaluating six smoothsteps. Keep the
    // original expression at feather boundaries, including cool/red skin tones.
    if cb >= 0.36 - spread
        && cb <= 0.50 + spread * 0.35
        && cr >= 0.55 - spread * 0.35
        && cr <= 0.69 + spread
        && r - b >= 0.035
        && (0.055..=0.96).contains(&y)
    {
        return 1.0;
    }
    if cb <= 0.30 - spread
        || cb >= 0.55 + spread
        || cr <= 0.505 - spread * 0.35
        || cr >= 0.77 + spread
        || r - b <= -0.025 - spread
        || !(0.015..1.0).contains(&y)
    {
        return 0.0;
    }
    let cb_gate = smoothstep(0.30 - spread, 0.36 - spread, cb)
        * (1.0 - smoothstep(0.50 + spread * 0.35, 0.55 + spread, cb));
    let cr_gate = smoothstep(0.505 - spread * 0.35, 0.55 - spread * 0.35, cr)
        * (1.0 - smoothstep(0.69 + spread, 0.77 + spread, cr));
    cb_gate
        * cr_gate
        * smoothstep(-0.025 - spread, 0.035, r - b)
        * smoothstep(0.015, 0.055, y)
        * (1.0 - smoothstep(0.96, 1.0, y))
}

// O(width * height) separable box mean with replicated image edges.
// f64 sliding accumulators keep large rows/columns from accumulating a visible drift.
// The maximum influence is 2 * radius: the manifest supplies 40 pixels on every band.
fn box_mean(data: &mut [f32], temp: &mut [f32], width: usize, height: usize, radius: usize) {
    let inv = 1.0 / (radius * 2 + 1) as f64;
    for y in 0..height {
        let row = y * width;
        let mut sum = data[row] as f64 * (radius + 1) as f64;
        for k in 1..=radius {
            sum += data[row + k.min(width - 1)] as f64;
        }
        for x in 0..width {
            temp[row + x] = (sum * inv) as f32;
            let remove = x.saturating_sub(radius);
            let add = (x + radius + 1).min(width - 1);
            sum += temp_input(data, row + add) - temp_input(data, row + remove);
        }
    }
    // Walk short contiguous spans instead of striding an entire image per column.
    // Fixed stack storage works in no_std without an extra image-sized allocation.
    const COLUMNS: usize = 64;
    for start in (0..width).step_by(COLUMNS) {
        let count = COLUMNS.min(width - start);
        let mut sums = [0.0f64; COLUMNS];
        for x in 0..count {
            sums[x] = temp[start + x] as f64 * (radius + 1) as f64;
        }
        for k in 1..=radius {
            let row = k.min(height - 1) * width + start;
            for x in 0..count {
                sums[x] += temp[row + x] as f64;
            }
        }
        for y in 0..height {
            let row = y * width + start;
            let remove = y.saturating_sub(radius) * width + start;
            let add = (y + radius + 1).min(height - 1) * width + start;
            for x in 0..count {
                data[row + x] = (sums[x] * inv) as f32;
                sums[x] += temp[add + x] as f64 - temp[remove + x] as f64;
            }
        }
    }
}

#[inline(always)]
fn temp_input(data: &[f32], index: usize) -> f64 {
    data[index] as f64
}

// Guided-filter moments always travel in pairs. Share index/border work and keep
// contiguous row slices so the interpreter does not repeat bounds checks per pixel.
fn box_mean_pair(
    a: &mut [f32],
    b: &mut [f32],
    ta: &mut [f32],
    tb: &mut [f32],
    width: usize,
    height: usize,
    radius: usize,
) {
    let inv = 1.0 / (radius * 2 + 1) as f64;
    for (((ra, rb), oa), ob) in a
        .chunks_exact(width)
        .zip(b.chunks_exact(width))
        .zip(ta.chunks_exact_mut(width))
        .zip(tb.chunks_exact_mut(width))
    {
        let mut sa = ra[0] as f64 * (radius + 1) as f64;
        let mut sb = rb[0] as f64 * (radius + 1) as f64;
        for k in 1..=radius {
            let x = k.min(width - 1);
            sa += ra[x] as f64;
            sb += rb[x] as f64;
        }
        for x in 0..ra.len() {
            oa[x] = (sa * inv) as f32;
            ob[x] = (sb * inv) as f32;
            let remove = x.saturating_sub(radius);
            let add = (x + radius + 1).min(width - 1);
            sa += ra[add] as f64 - ra[remove] as f64;
            sb += rb[add] as f64 - rb[remove] as f64;
        }
    }
    const COLUMNS: usize = 64;
    for start in (0..width).step_by(COLUMNS) {
        let count = COLUMNS.min(width - start);
        let mut sa = [0.0f64; COLUMNS];
        let mut sb = [0.0f64; COLUMNS];
        let sa = &mut sa[..count];
        let sb = &mut sb[..count];
        for ((sum, x), (other, y)) in sa
            .iter_mut()
            .zip(&ta[start..start + count])
            .zip(sb.iter_mut().zip(&tb[start..start + count]))
        {
            *sum = *x as f64 * (radius + 1) as f64;
            *other = *y as f64 * (radius + 1) as f64;
        }
        for k in 1..=radius {
            let row = k.min(height - 1) * width + start;
            for ((sum, x), (other, y)) in sa
                .iter_mut()
                .zip(&ta[row..row + count])
                .zip(sb.iter_mut().zip(&tb[row..row + count]))
            {
                *sum += *x as f64;
                *other += *y as f64;
            }
        }
        for y in 0..height {
            let row = y * width + start;
            let remove = y.saturating_sub(radius) * width + start;
            let add = (y + radius + 1).min(height - 1) * width + start;
            let output = a[row..row + count]
                .iter_mut()
                .zip(b[row..row + count].iter_mut());
            let sums = sa.iter_mut().zip(sb.iter_mut());
            let adding = ta[add..add + count].iter().zip(&tb[add..add + count]);
            let removing = ta[remove..remove + count]
                .iter()
                .zip(&tb[remove..remove + count]);
            for ((((oa, ob), (sa, sb)), (aa, ab)), (ra, rb)) in
                output.zip(sums).zip(adding).zip(removing)
            {
                *oa = (*sa * inv) as f32;
                *ob = (*sb * inv) as f32;
                *sa += *aa as f64 - *ra as f64;
                *sb += *ab as f64 - *rb as f64;
            }
        }
    }
}

fn valid_rgb(p: &[f32]) -> bool {
    p[..3]
        .iter()
        .all(|x| x.is_finite() && (0.0..=1.0).contains(x))
}

// The mask is pointwise: it needs neither neighbourhood statistics nor scratch memory.
fn show_mask(pixels: &mut [f32], channels: usize, alpha: bool, settings: Settings) {
    for p in pixels.chunks_exact_mut(channels) {
        if !valid_rgb(p) || (alpha && (!p[3].is_finite() || p[3] <= 0.0)) {
            continue;
        }
        let mask = if settings.skin_only {
            skin_gate(p[0], p[1], p[2], settings.skin_range)
        } else {
            1.0
        };
        p[..3].fill(mask);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Region {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

// Trim empty borders before allocating neighbourhood planes. Fully covered images
// usually need only four boundary probes; a sparse image scans only empty margins.
// Keep 2 * radius of original neighbours, including non-skin/transparent samples.
fn processing_region(
    pixels: &[f32],
    width: usize,
    height: usize,
    channels: usize,
    alpha: bool,
    settings: Settings,
) -> Option<Region> {
    let affected = |x: usize, y: usize| {
        let i = (y * width + x) * channels;
        let p = &pixels[i..i + channels];
        valid_rgb(p)
            && (!alpha || (p[3].is_finite() && p[3] > 0.0))
            && (!settings.skin_only || skin_gate(p[0], p[1], p[2], settings.skin_range) > 0.0)
    };
    let top = (0..height).find(|&y| (0..width).any(|x| affected(x, y)))?;
    let bottom = (top..height)
        .rev()
        .find(|&y| (0..width).any(|x| affected(x, y)))?;
    let left = (0..width).find(|&x| (top..=bottom).any(|y| affected(x, y)))?;
    let right = (left..width)
        .rev()
        .find(|&x| (top..=bottom).any(|y| affected(x, y)))?;
    let halo = settings.radius * 2;
    let x = left.saturating_sub(halo);
    let y = top.saturating_sub(halo);
    Some(Region {
        x,
        y,
        width: (right + halo + 1).min(width) - x,
        height: (bottom + halo + 1).min(height) - y,
    })
}

#[inline(always)]
fn pixel_offset<const CROPPED: bool>(
    i: usize,
    image_width: usize,
    channels: usize,
    region: Region,
) -> usize {
    if CROPPED {
        ((region.y + i / region.width) * image_width + region.x + i % region.width) * channels
    } else {
        i * channels
    }
}

#[cfg(test)]
fn process(
    pixels: &mut [f32],
    width: usize,
    height: usize,
    channels: usize,
    alpha: bool,
    settings: Settings,
    scratch: &mut [f32],
) -> i32 {
    if width == 0 || height == 0 || !matches!((channels, alpha), (3, false) | (4, true)) {
        return 1;
    }
    let Some(n) = width.checked_mul(height) else {
        return 1;
    };
    if n.checked_mul(channels) != Some(pixels.len()) {
        return 1;
    }
    if settings.strength == 0.0 && !settings.show_mask {
        adjust_skin_tone(pixels, channels, alpha, settings, None);
        return 0;
    }
    if settings.show_mask {
        show_mask(pixels, channels, alpha, settings);
        return 0;
    }
    let Some(region) = processing_region(pixels, width, height, channels, alpha, settings) else {
        return 0;
    };
    let plan = regions::plan(pixels, width, height, channels, alpha, settings, region);
    let Some(needed) = plan.max_area().checked_mul(PLANES) else {
        return 1;
    };
    if scratch.len() < needed {
        return 1;
    }
    regions::run(
        pixels,
        width,
        channels,
        alpha,
        settings,
        &plan,
        &mut scratch[..needed],
    )
}

fn process_region(
    pixels: &mut [f32],
    image_width: usize,
    channels: usize,
    alpha: bool,
    settings: Settings,
    region: Region,
    scratch: &mut [f32],
) -> i32 {
    if region.width == image_width {
        // Keep the contiguous full-width case free of per-pixel ROI indexing.
        let start = region.y * image_width * channels;
        let end = start + region.width * region.height * channels;
        process_region_inner::<false>(
            &mut pixels[start..end],
            image_width,
            channels,
            alpha,
            settings,
            Region { y: 0, ..region },
            FilterBuffers {
                scratch,
                basis: None,
            },
        )
    } else {
        process_region_inner::<true>(
            pixels,
            image_width,
            channels,
            alpha,
            settings,
            region,
            FilterBuffers {
                scratch,
                basis: None,
            },
        )
    }
}

struct FilterBuffers<'a> {
    scratch: &'a mut [f32],
    basis: Option<&'a mut [f32]>,
}

fn process_region_inner<const CROPPED: bool>(
    pixels: &mut [f32],
    image_width: usize,
    channels: usize,
    alpha: bool,
    settings: Settings,
    region: Region,
    buffers: FilterBuffers<'_>,
) -> i32 {
    let FilterBuffers { scratch, mut basis } = buffers;
    let (width, height) = (region.width, region.height);
    let n = width * height;
    if n.checked_mul(PLANES) != Some(scratch.len()) {
        return 1;
    }
    let (source, rest) = scratch.split_at_mut(n);
    let (temp, rest) = rest.split_at_mut(n);
    let (a, rest) = rest.split_at_mut(n);
    let (b, rest) = rest.split_at_mut(n);
    let (weight, rest) = rest.split_at_mut(n);
    let (weight_mean, mask) = rest.split_at_mut(n);
    let mut opaque = true;
    let mut affected = false;
    for i in 0..n {
        let j = pixel_offset::<CROPPED>(i, image_width, channels, region);
        let p = &pixels[j..j + channels];
        let opacity = if alpha { p[3] } else { 1.0 };
        weight[i] = if valid_rgb(p) && opacity.is_finite() {
            opacity.clamp(0.0, 1.0)
        } else {
            0.0
        };
        mask[i] = if weight[i] == 0.0 {
            0.0
        } else if settings.skin_only {
            skin_gate(p[0], p[1], p[2], settings.skin_range)
        } else {
            1.0
        };
        opaque &= weight[i] == 1.0;
        affected |= mask[i] > 0.0;
    }
    // Blue backgrounds and empty bands need no neighbourhood filtering.
    if !affected {
        return 0;
    }
    if !opaque {
        weight_mean.copy_from_slice(weight);
        box_mean(weight_mean, temp, width, height, settings.radius);
    }
    let edge = 1.0 - settings.edge;
    let epsilon = 0.00008 + edge * edge * 0.008;
    for channel in 0..3 {
        if opaque {
            for i in 0..n {
                let v = pixels[pixel_offset::<CROPPED>(i, image_width, channels, region) + channel];
                a[i] = v;
                b[i] = v * v;
            }
        } else {
            for i in 0..n {
                let v = pixels[pixel_offset::<CROPPED>(i, image_width, channels, region) + channel];
                let v = if weight[i] > 0.0 { v } else { 0.0 };
                a[i] = v * weight[i];
                b[i] = v * v * weight[i];
            }
        }
        box_mean_pair(a, b, temp, source, width, height, settings.radius);
        if opaque {
            for i in 0..n {
                let mean = a[i];
                let variance = (b[i] - mean * mean).max(0.0);
                let coefficient = variance / (variance + epsilon);
                a[i] = coefficient;
                b[i] = (1.0 - coefficient) * mean;
            }
        } else {
            for i in 0..n {
                let w = weight_mean[i];
                if w > 1.0e-8 {
                    let mean = a[i] / w;
                    let variance = (b[i] / w - mean * mean).max(0.0);
                    let coefficient = variance / (variance + epsilon);
                    a[i] = coefficient * weight[i];
                    b[i] = (1.0 - coefficient) * mean * weight[i];
                } else {
                    a[i] = 0.0;
                    b[i] = 0.0;
                }
            }
        }
        box_mean_pair(a, b, temp, source, width, height, settings.radius);
        // `source` doubles as the second temporary plane while averaging moments.
        // Restore the unchanged input channel before any neighbouring pixels are read.
        if settings.texture > 0.0 {
            for i in 0..n {
                source[i] = if weight[i] > 0.0 {
                    pixels[pixel_offset::<CROPPED>(i, image_width, channels, region) + channel]
                } else {
                    0.0
                };
            }
        }
        for y in 0..height {
            let top = y.saturating_sub(1) * width;
            let bottom = (y + 1).min(height - 1) * width;
            let start = ((region.y + y) * image_width + region.x) * channels;
            let row = &mut pixels[start..start + width * channels];
            for (x, p) in row.chunks_exact_mut(channels).enumerate() {
                let i = y * width + x;
                if mask[i] == 0.0 || (!opaque && (weight[i] == 0.0 || weight_mean[i] <= 1.0e-8)) {
                    continue;
                }
                let original = p[channel];
                let q = a[i] * original + b[i];
                let q = if opaque { q } else { q / weight_mean[i] }.clamp(0.0, 1.0);
                // Fine structure protection uses the unchanged channel plane, not partially
                // processed neighbouring pixels. Weight the centre and neighbours alike
                // so uniform alpha scaling does not change texture protection.
                let detail = if settings.texture == 0.0 && basis.is_none() {
                    0.0
                } else {
                    let left = x.saturating_sub(1);
                    let right = (x + 1).min(width - 1);
                    let neighbours = [top + x, bottom + x, y * width + left, y * width + right];
                    let fine = if opaque {
                        let mut sum = original;
                        for j in neighbours {
                            sum += source[j];
                        }
                        sum / 5.0
                    } else {
                        let mut sum = original * weight[i];
                        let mut total = weight[i];
                        for j in neighbours {
                            sum += source[j] * weight[j];
                            total += weight[j];
                        }
                        sum / total
                    };
                    ((original - fine).abs() / 0.035).min(1.0)
                };
                // Avoid a single aggressive pass causing a large tonal jump.
                let delta = (q - original).clamp(-0.12, 0.12);
                if let Some(cache) = basis.as_deref_mut() {
                    cache[i * 6 + channel] = delta;
                    cache[i * 6 + 3 + channel] = detail;
                } else {
                    let amount = if settings.texture == 0.0 {
                        settings.strength * mask[i]
                    } else {
                        settings.strength * mask[i] * (1.0 - settings.texture * detail * 0.85)
                    };
                    p[channel] = (original + delta * amount).clamp(0.0, 1.0);
                }
            }
        }
    }
    // Use the original skin gate, so smoothing never changes the colour-selection boundary.
    if settings.brightening != 0.0 || settings.rosiness != 0.0 {
        for y in 0..height {
            let start = ((region.y + y) * image_width + region.x) * channels;
            adjust_skin_tone(
                &mut pixels[start..start + width * channels],
                channels,
                alpha,
                settings,
                Some(&mask[y * width..(y + 1) * width]),
            );
        }
    }
    0
}

fn adjust_skin_tone(
    pixels: &mut [f32],
    channels: usize,
    alpha: bool,
    settings: Settings,
    original_mask: Option<&[f32]>,
) {
    // Avoid even a colour-space round trip for old presets and neutral controls.
    if settings.brightening == 0.0 && settings.rosiness == 0.0 {
        return;
    }
    let tint = [
        0.07 * settings.rosiness,
        -0.035 * settings.rosiness,
        0.005 * settings.rosiness,
    ];
    let tint_y = 0.299 * tint[0] + 0.587 * tint[1] + 0.114 * tint[2];
    let saturation = 1.0 - 0.12 * settings.brightening;
    for (i, p) in pixels.chunks_exact_mut(channels).enumerate() {
        if !valid_rgb(p) || (alpha && (!p[3].is_finite() || p[3] <= 0.0)) {
            continue;
        }
        let amount = original_mask.map_or_else(
            || {
                if settings.skin_only {
                    skin_gate(p[0], p[1], p[2], settings.skin_range)
                } else {
                    1.0
                }
            },
            |mask| mask[i],
        );
        if amount == 0.0 {
            continue;
        }
        // Work in the host's normalized RGB. Midtone-weighted brightening leaves
        // black/white endpoints fixed, with gentle desaturation for a lighter tone.
        let y = 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];
        let target_y = y + settings.brightening * 0.6 * y * (1.0 - y);
        let mut chroma = [0.0; 3];
        let mut scale = 1.0f32;
        for c in 0..3 {
            chroma[c] = (p[c] - y) * saturation + tint[c] - tint_y;
            // Compress chroma uniformly at the gamut boundary rather than clipping
            // individual channels and distorting skin hue or the target luminance.
            if chroma[c] > 0.0 {
                scale = scale.min((1.0 - target_y) / chroma[c]);
            } else if chroma[c] < 0.0 {
                scale = scale.min(target_y / -chroma[c]);
            }
        }
        for c in 0..3 {
            let target = (target_y + chroma[c] * scale).clamp(0.0, 1.0);
            p[c] = (p[c] + (target - p[c]) * amount).clamp(0.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(pixels: &mut [f32], w: usize, h: usize, channels: usize, settings: Settings) {
        let mut scratch = vec![0.0; w * h * PLANES];
        assert_eq!(
            process(
                pixels,
                w,
                h,
                channels,
                channels == 4,
                settings,
                &mut scratch
            ),
            0
        );
    }

    #[test]
    fn box_mean_matches_naive_at_edges_and_on_tiny_images() {
        for (w, h) in [(1, 1), (2, 3), (23, 19), (65, 7), (129, 3)] {
            for r in [1, 6, 20] {
                let input: Vec<f32> = (0..w * h).map(|i| ((i * 17 % 67) as f32) / 67.0).collect();
                let mut actual = input.clone();
                box_mean(&mut actual, &mut vec![0.0; w * h], w, h, r);
                let mut paired = input.clone();
                let mut other: Vec<f32> = input.iter().map(|x| x * x).collect();
                let mut expected_other = other.clone();
                box_mean(&mut expected_other, &mut vec![0.0; w * h], w, h, r);
                box_mean_pair(
                    &mut paired,
                    &mut other,
                    &mut vec![0.0; w * h],
                    &mut vec![0.0; w * h],
                    w,
                    h,
                    r,
                );
                assert_eq!(paired, actual);
                assert_eq!(other, expected_other);
                for y in 0..h {
                    for x in 0..w {
                        let mut sum = 0.0f64;
                        for dy in -(r as isize)..=r as isize {
                            for dx in -(r as isize)..=r as isize {
                                let yy = (y as isize + dy).clamp(0, h as isize - 1) as usize;
                                let xx = (x as isize + dx).clamp(0, w as isize - 1) as usize;
                                sum += input[yy * w + xx] as f64;
                            }
                        }
                        let expected = (sum / ((2 * r + 1) * (2 * r + 1)) as f64) as f32;
                        assert!((actual[y * w + x] - expected).abs() < 2.0e-6);
                    }
                }
            }
        }
    }

    #[test]
    fn mask_needs_no_scratch_and_preserves_special_pixels() {
        let mut p = [
            0.64,
            0.44,
            0.33,
            0.5, // skin
            0.1,
            0.3,
            0.9,
            1.0, // blue
            0.6,
            0.4,
            0.3,
            0.0, // transparent payload
            2.0,
            0.4,
            0.3,
            1.0, // HDR
            f32::NAN,
            0.4,
            0.3,
            1.0,
        ];
        let before = p;
        assert_eq!(
            process(
                &mut p,
                5,
                1,
                4,
                true,
                Settings {
                    strength: 0.0,
                    show_mask: true,
                    ..Settings::default()
                },
                &mut [],
            ),
            0
        );
        assert!(p[0] > 0.8);
        assert_eq!(&p[0..3], &[p[0]; 3]);
        assert_eq!(&p[4..7], &[0.0; 3]);
        for i in 8..p.len() {
            assert_eq!(p[i].to_bits(), before[i].to_bits());
        }
        for i in (3..p.len()).step_by(4) {
            assert_eq!(p[i], before[i]);
        }
    }

    #[test]
    fn opaque_fast_path_matches_weighted_path() {
        for radius in [1, 6, 20] {
            let mut opaque = Vec::new();
            for i in 0..129 * 7 {
                let n = ((i * 17 % 101) as f32 / 100.0 - 0.5) * 0.05;
                opaque.extend_from_slice(&[0.64 + n, 0.44 + n, 0.33 + n, 1.0]);
            }
            let mut weighted = opaque.clone();
            for p in weighted.as_chunks_mut::<4>().0.iter_mut() {
                p[3] = 0.5;
            }
            let settings = Settings {
                radius,
                ..Settings::default()
            };
            run(&mut opaque, 129, 7, 4, settings);
            run(&mut weighted, 129, 7, 4, settings);
            for (a, b) in opaque
                .as_chunks::<4>()
                .0
                .iter()
                .zip(weighted.as_chunks::<4>().0.iter())
            {
                for c in 0..3 {
                    assert!((a[c] - b[c]).abs() < 2.0e-6);
                }
            }
        }
    }

    #[test]
    fn strength_zero_is_bit_exact_including_nan_and_hdr() {
        let mut p = vec![f32::NAN, 0.2, 1.5, 0.3, 0.7, 0.4, 0.2, 1.0];
        let before: Vec<u32> = p.iter().map(|x| x.to_bits()).collect();
        run(
            &mut p,
            2,
            1,
            4,
            Settings {
                strength: 0.0,
                ..Settings::default()
            },
        );
        assert_eq!(before, p.iter().map(|x| x.to_bits()).collect::<Vec<_>>());
    }

    #[test]
    fn constant_colour_alpha_and_transparent_payload_are_preserved() {
        let mut p = Vec::new();
        for i in 0..51 {
            p.extend_from_slice(if i % 7 == 0 {
                &[0.0, 1.0, 1.0, 0.0]
            } else {
                &[0.6, 0.4, 0.3, 0.4]
            });
        }
        let before = p.clone();
        run(
            &mut p,
            17,
            3,
            4,
            Settings {
                strength: 1.0,
                ..Settings::default()
            },
        );
        for (a, b) in p
            .as_chunks::<4>()
            .0
            .iter()
            .zip(before.as_chunks::<4>().0.iter())
        {
            assert_eq!(a[3].to_bits(), b[3].to_bits());
            for c in 0..3 {
                assert!((a[c] - b[c]).abs() < 3.0e-5);
            }
        }
    }

    #[test]
    fn invalid_and_hdr_pixels_do_not_poison_neighbours() {
        let mut p = [
            0.6,
            0.4,
            0.3,
            2.0,
            0.2,
            0.1,
            f32::NAN,
            0.4,
            0.3,
            0.6,
            0.4,
            0.3,
        ];
        run(&mut p, 4, 1, 3, Settings::default());
        assert_eq!(p[3], 2.0);
        assert!(p[6].is_nan());
        for i in [0, 1, 2, 9, 10, 11] {
            assert!(p[i].is_finite());
        }
    }

    #[test]
    fn chroma_gate_covers_multiple_skin_shades_and_rejects_blue() {
        for rgb in [[0.82, 0.62, 0.49], [0.53, 0.34, 0.24], [0.27, 0.16, 0.11]] {
            assert!(skin_gate(rgb[0], rgb[1], rgb[2], 0.6) > 0.8, "{rgb:?}");
        }
        assert_eq!(skin_gate(0.1, 0.3, 0.9, 0.6), 0.0);
        assert_eq!(skin_gate(0.1, 0.8, 0.2, 0.6), 0.0);
    }

    #[test]
    fn skin_plateau_shortcuts_match_full_feather_ramps() {
        for range in [0.0, 0.2, 0.6, 1.0] {
            for ri in 0..=16 {
                for gi in 0..=16 {
                    for bi in 0..=16 {
                        let (r, g, b) = (ri as f32 / 16.0, gi as f32 / 16.0, bi as f32 / 16.0);
                        let y = 0.299 * r + 0.587 * g + 0.114 * b;
                        let cb = 0.5 - 0.168736 * r - 0.331264 * g + 0.5 * b;
                        let cr = 0.5 + 0.5 * r - 0.418688 * g - 0.081312 * b;
                        let spread = range * 0.055;
                        let expected = smoothstep(0.30 - spread, 0.36 - spread, cb)
                            * (1.0 - smoothstep(0.50 + spread * 0.35, 0.55 + spread, cb))
                            * smoothstep(0.505 - spread * 0.35, 0.55 - spread * 0.35, cr)
                            * (1.0 - smoothstep(0.69 + spread, 0.77 + spread, cr))
                            * smoothstep(-0.025 - spread, 0.035, r - b)
                            * smoothstep(0.015, 0.055, y)
                            * (1.0 - smoothstep(0.96, 1.0, y));
                        // The production expression groups the two chroma ramps first.
                        // Allow only floating-point grouping noise in the feather region.
                        assert!((skin_gate(r, g, b, range) - expected).abs() < 2.0e-7);
                    }
                }
            }
        }
    }

    #[test]
    fn host_json_scalars_and_geometry_are_not_confused() {
        let s = Settings::from_json(br#"{"Strength":7.5e1,"Radius":9,"Skin_Only":false,"Show_Mask":true,"_image":{"width":800}}"#);
        assert_eq!(s.radius, 9);
        assert_eq!(s.strength, 0.75);
        assert!(!s.skin_only);
        assert!(s.show_mask);
        assert_eq!(Settings::from_json(br#"{"Radius":null}"#).radius, 6);
        let s = Settings::from_json(br#"{"skinBrightening":200,"skinRosiness":-200}"#);
        assert_eq!(s.brightening, 1.0);
        assert_eq!(s.rosiness, -1.0);
        let s = Settings::from_json(br#"{"skinBrightening":null,"skinRosiness":"bad"}"#);
        assert_eq!(s.brightening, 0.0);
        assert_eq!(s.rosiness, 0.0);
    }

    #[test]
    fn tone_only_brightens_and_controls_redness_without_scratch() {
        let original = [0.64, 0.44, 0.33, 0.4];
        let luma = |p: &[f32]| 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];
        for (brightening, rosiness) in [(1.0, 0.0), (0.0, 1.0), (0.0, -1.0), (1.0, 1.0)] {
            let mut p = original;
            assert_eq!(
                process(
                    &mut p,
                    1,
                    1,
                    4,
                    true,
                    Settings {
                        strength: 0.0,
                        brightening,
                        rosiness,
                        ..Settings::default()
                    },
                    &mut []
                ),
                0
            );
            assert_eq!(p[3], original[3]);
            if brightening > 0.0 {
                assert!(luma(&p) > luma(&original) + 0.1);
            } else {
                assert!((luma(&p) - luma(&original)).abs() < 2e-6);
            }
            if rosiness > 0.0 {
                assert!(p[0] - p[1] > original[0] - original[1]);
            }
            if rosiness < 0.0 {
                assert!(p[0] - p[1] < original[0] - original[1]);
            }
        }
    }

    #[test]
    fn tone_preserves_blue_transparency_hdr_and_invalid_samples() {
        let mut p = [
            0.1,
            0.3,
            0.9,
            1.0,
            0.6,
            0.4,
            0.3,
            0.0,
            2.0,
            0.4,
            0.3,
            1.0,
            f32::NAN,
            0.4,
            0.3,
            1.0,
            0.6,
            0.4,
            0.3,
            f32::NAN,
        ];
        let original = p.map(f32::to_bits);
        run(
            &mut p,
            5,
            1,
            4,
            Settings {
                brightening: 1.0,
                rosiness: 1.0,
                ..Settings::default()
            },
        );
        assert_eq!(p.map(f32::to_bits), original);
    }

    #[test]
    fn tone_gamut_compression_keeps_pixels_finite_and_in_range() {
        for brightening in [0.0, 1.0] {
            for rosiness in [-1.0, 1.0] {
                for r in [0.0, 0.001, 0.5, 0.999, 1.0] {
                    for g in [0.0, 0.001, 0.5, 0.999, 1.0] {
                        for b in [0.0, 0.001, 0.5, 0.999, 1.0] {
                            let mut p = [r, g, b];
                            let expected_y = 0.299 * r + 0.587 * g + 0.114 * b;
                            let expected_y =
                                expected_y + brightening * 0.6 * expected_y * (1.0 - expected_y);
                            assert_eq!(
                                process(
                                    &mut p,
                                    1,
                                    1,
                                    3,
                                    false,
                                    Settings {
                                        strength: 0.0,
                                        skin_only: false,
                                        brightening,
                                        rosiness,
                                        ..Settings::default()
                                    },
                                    &mut []
                                ),
                                0
                            );
                            assert!(p.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
                            let actual_y = 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];
                            assert!((actual_y - expected_y).abs() < 2e-6);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn region_trims_empty_borders_and_keeps_full_filter_halo() {
        let (w, h) = (160, 120);
        let mut p = [0.1, 0.3, 0.9, 1.0].repeat(w * h);
        assert_eq!(
            processing_region(&p, w, h, 4, true, Settings::default()),
            None
        );
        for y in 60..66 {
            for x in 80..88 {
                p[(y * w + x) * 4..(y * w + x) * 4 + 3].copy_from_slice(&[0.64, 0.44, 0.33]);
            }
        }
        assert_eq!(
            processing_region(
                &p,
                w,
                h,
                4,
                true,
                Settings {
                    radius: 3,
                    ..Settings::default()
                }
            ),
            Some(Region {
                x: 74,
                y: 54,
                width: 20,
                height: 18
            })
        );
        assert_eq!(
            processing_region(
                &p,
                w,
                h,
                4,
                true,
                Settings {
                    skin_only: false,
                    ..Settings::default()
                }
            ),
            Some(Region {
                x: 0,
                y: 0,
                width: w,
                height: h
            })
        );
    }

    #[test]
    fn cropped_filter_matches_whole_buffer_at_corners_and_with_alpha() {
        let (w, h) = (161, 113);
        for (left, top) in [(0, 0), (73, 45), (150, 103)] {
            for alpha in [false, true] {
                let channels = if alpha { 4 } else { 3 };
                let mut input = Vec::new();
                for y in 0..h {
                    for x in 0..w {
                        let skin = (left..left + 11).contains(&x) && (top..top + 10).contains(&y);
                        let noise = ((x * 17 + y * 31) % 101) as f32 / 2000.0 - 0.025;
                        let rgb = if skin {
                            [0.64 + noise, 0.44 + noise, 0.33 + noise]
                        } else {
                            [0.1, 0.3, 0.9]
                        };
                        input.extend_from_slice(&rgb);
                        if alpha {
                            input.push(((x + y) % 5) as f32 / 4.0);
                        }
                    }
                }
                // Include unusual neighbour samples near an affected patch.
                input[((top + 1) * w + (left + 1)) * channels] = f32::NAN;
                input[((top + 2) * w + (left + 2)) * channels] = 2.0;
                for radius in [1, 6, 20] {
                    for texture in [0.0, 0.6] {
                        let settings = Settings {
                            radius,
                            texture,
                            brightening: 0.5,
                            rosiness: 0.3,
                            ..Settings::default()
                        };
                        let mut actual = input.clone();
                        run(&mut actual, w, h, channels, settings);
                        let mut expected = input.clone();
                        assert_eq!(
                            process_region(
                                &mut expected,
                                w,
                                channels,
                                alpha,
                                settings,
                                Region {
                                    x: 0,
                                    y: 0,
                                    width: w,
                                    height: h
                                },
                                &mut vec![0.0; w * h * PLANES]
                            ),
                            0
                        );
                        for (a, b) in actual.iter().zip(&expected) {
                            assert!(
                                a.to_bits() == b.to_bits() || (a - b).abs() < 2e-6,
                                "crop mismatch at ({left},{top}), radius {radius}, alpha {alpha}: {a} vs {b}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn opaque_crop_matches_full_frame_with_distant_transparency() {
        let (w, h) = (129, 97);
        let mut input = [0.1, 0.3, 0.9, 1.0].repeat(w * h);
        for y in 0..h {
            for x in 0..w {
                if x < 5 || y < 5 || x >= w - 5 || y >= h - 5 {
                    input[(y * w + x) * 4 + 3] = 0.0;
                }
                if (60..69).contains(&x) && (44..53).contains(&y) {
                    let noise = ((x * 17 + y * 31) % 101) as f32 / 2000.0 - 0.025;
                    input[(y * w + x) * 4..(y * w + x) * 4 + 3].copy_from_slice(&[
                        0.64 + noise,
                        0.44 + noise,
                        0.33 + noise,
                    ]);
                }
            }
        }
        let settings = Settings::default();
        assert_eq!(
            processing_region(&input, w, h, 4, true, settings),
            Some(Region {
                x: 48,
                y: 32,
                width: 33,
                height: 33
            })
        );
        let mut actual = input.clone();
        run(&mut actual, w, h, 4, settings);
        let mut expected = input;
        assert_eq!(
            process_region(
                &mut expected,
                w,
                4,
                true,
                settings,
                Region {
                    x: 0,
                    y: 0,
                    width: w,
                    height: h
                },
                &mut vec![0.0; w * h * PLANES]
            ),
            0
        );
        for (a, b) in actual.iter().zip(expected) {
            assert!((a - b).abs() < 2e-6);
        }
    }
}
