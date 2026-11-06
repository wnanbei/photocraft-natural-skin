//! Natural Skin: a local, import-free PhotoCraft ABI v1 filter.
//! Scalar self-guided filtering, alpha-weighted statistics, and a feathered skin-colour gate.
//! No face recognition, generative editing, network access, or host dependencies.
#![cfg_attr(target_arch = "wasm32", no_std)]
#![cfg_attr(not(any(target_arch = "wasm32", test)), allow(dead_code))]

const MAX_RADIUS: usize = 20;
const PLANES: usize = 7;

#[cfg(target_arch = "wasm32")]
const MANIFEST: &str = include_str!("../manifest.json");

#[derive(Clone, Copy, Debug)]
struct Settings {
    strength: f32,
    radius: usize,
    edge: f32,
    texture: f32,
    skin_only: bool,
    skin_range: f32,
    show_mask: bool,
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
    for x in 0..width {
        let mut sum = temp[x] as f64 * (radius + 1) as f64;
        for k in 1..=radius {
            sum += temp[k.min(height - 1) * width + x] as f64;
        }
        for y in 0..height {
            data[y * width + x] = (sum * inv) as f32;
            let remove = y.saturating_sub(radius);
            let add = (y + radius + 1).min(height - 1);
            sum += temp[add * width + x] as f64 - temp[remove * width + x] as f64;
        }
    }
}

#[inline(always)]
fn temp_input(data: &[f32], index: usize) -> f64 {
    data[index] as f64
}

fn valid_rgb(p: &[f32]) -> bool {
    p[..3]
        .iter()
        .all(|x| x.is_finite() && (0.0..=1.0).contains(x))
}

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
    if n.checked_mul(channels) != Some(pixels.len()) || n.checked_mul(PLANES) != Some(scratch.len())
    {
        return 1;
    }
    if settings.strength == 0.0 && !settings.show_mask {
        return 0;
    }
    let (source, rest) = scratch.split_at_mut(n);
    let (temp, rest) = rest.split_at_mut(n);
    let (a, rest) = rest.split_at_mut(n);
    let (b, rest) = rest.split_at_mut(n);
    let (weight, rest) = rest.split_at_mut(n);
    let (weight_mean, mask) = rest.split_at_mut(n);
    for i in 0..n {
        let p = &pixels[i * channels..(i + 1) * channels];
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
    }
    if settings.show_mask {
        for i in 0..n {
            if weight[i] > 0.0 {
                pixels[i * channels..i * channels + 3].fill(mask[i]);
            }
        }
        return 0;
    }
    weight_mean.copy_from_slice(weight);
    box_mean(weight_mean, temp, width, height, settings.radius);
    let edge = 1.0 - settings.edge;
    let epsilon = 0.00008 + edge * edge * 0.008;
    for channel in 0..3 {
        for i in 0..n {
            let v = pixels[i * channels + channel];
            source[i] = if weight[i] > 0.0 { v } else { 0.0 };
            a[i] = source[i] * weight[i];
            b[i] = source[i] * source[i] * weight[i];
        }
        box_mean(a, temp, width, height, settings.radius);
        box_mean(b, temp, width, height, settings.radius);
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
        box_mean(a, temp, width, height, settings.radius);
        box_mean(b, temp, width, height, settings.radius);
        for y in 0..height {
            let top = y.saturating_sub(1) * width;
            let bottom = (y + 1).min(height - 1) * width;
            for x in 0..width {
                let i = y * width + x;
                if weight[i] == 0.0 || mask[i] == 0.0 || weight_mean[i] <= 1.0e-8 {
                    continue;
                }
                let original = source[i];
                let q = ((a[i] * original + b[i]) / weight_mean[i]).clamp(0.0, 1.0);
                // Fine structure protection uses the unchanged channel plane, not partially
                // processed neighbouring pixels. Weight the centre and neighbours alike
                // so uniform alpha scaling does not change texture protection.
                let left = x.saturating_sub(1);
                let right = (x + 1).min(width - 1);
                let neighbours = [top + x, bottom + x, y * width + left, y * width + right];
                let mut fine = original * weight[i];
                let mut total = weight[i];
                for j in neighbours {
                    fine += source[j] * weight[j];
                    total += weight[j];
                }
                let detail = ((original - fine / total).abs() / 0.035).min(1.0);
                let amount = settings.strength * mask[i] * (1.0 - settings.texture * detail * 0.85);
                // Avoid a single aggressive pass causing a large tonal jump.
                let delta = (q - original).clamp(-0.12, 0.12);
                pixels[i * channels + channel] = (original + delta * amount).clamp(0.0, 1.0);
            }
        }
    }
    0
}

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn pc_abi_version() -> u32 {
    1
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn pc_manifest() -> u64 {
    ((MANIFEST.len() as u64) << 32) | MANIFEST.as_ptr() as u64
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn pc_alloc(size: u32) -> u32 {
    if size == 0 {
        return 0;
    }
    let pages = (size as usize).div_ceil(65536);
    let old = core::arch::wasm32::memory_grow(0, pages);
    if old == usize::MAX {
        0
    } else {
        (old * 65536) as u32
    }
}

#[cfg(target_arch = "wasm32")]
fn memory_range(ptr: u32, len: u32) -> bool {
    ptr != 0
        && (ptr as usize)
            .checked_add(len as usize)
            .is_some_and(|end| end <= core::arch::wasm32::memory_size(0) * 65536)
}

/// Host ABI: error 1=invalid layout, 2=unsupported colour mode, 3=allocation failure,
/// 4=invalid memory range. Inputs are distinct blocks allocated with pc_alloc.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn pc_filter(
    buf: u32,
    len: u32,
    width: u32,
    height: u32,
    channels: u32,
    format: u32,
    params: u32,
    params_len: u32,
) -> i32 {
    let alpha = format & 256 != 0;
    if (format >> 16) & 255 != 3 {
        return 2;
    }
    if !matches!((channels, alpha), (3, false) | (4, true)) || width == 0 || height == 0 {
        return 1;
    }
    if buf % 4 != 0 || !memory_range(buf, len) || !memory_range(params, params_len) {
        return 4;
    }
    let Some(n) = width.checked_mul(height) else {
        return 1;
    };
    if n.checked_mul(channels).and_then(|x| x.checked_mul(4)) != Some(len) {
        return 1;
    }
    // Host buffer and JSON may not overlap, even for callers outside PhotoCraft.
    if (buf as u64) < params as u64 + params_len as u64 && (params as u64) < buf as u64 + len as u64
    {
        return 4;
    }
    let settings = Settings::from_json(unsafe {
        core::slice::from_raw_parts(params as *const u8, params_len as usize)
    });
    if settings.strength == 0.0 && !settings.show_mask {
        return 0;
    }
    let Some(scratch_len) = n.checked_mul(PLANES as u32).and_then(|x| x.checked_mul(4)) else {
        return 3;
    };
    let scratch = pc_alloc(scratch_len);
    if scratch == 0 {
        return 3;
    }
    // No memory references are created before memory.grow, including source/JSON references.
    process(
        unsafe { core::slice::from_raw_parts_mut(buf as *mut f32, len as usize / 4) },
        width as usize,
        height as usize,
        channels as usize,
        alpha,
        settings,
        unsafe { core::slice::from_raw_parts_mut(scratch as *mut f32, scratch_len as usize / 4) },
    )
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
        for (w, h) in [(1, 1), (2, 3), (23, 19)] {
            for r in [1, 6, 20] {
                let input: Vec<f32> = (0..w * h).map(|i| ((i * 17 % 67) as f32) / 67.0).collect();
                let mut actual = input.clone();
                box_mean(&mut actual, &mut vec![0.0; w * h], w, h, r);
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
        for (a, b) in p.chunks_exact(4).zip(before.chunks_exact(4)) {
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
    fn host_json_scalars_and_geometry_are_not_confused() {
        let s = Settings::from_json(br#"{"Strength":7.5e1,"Radius":9,"Skin_Only":false,"Show_Mask":true,"_image":{"width":800}}"#);
        assert_eq!(s.radius, 9);
        assert_eq!(s.strength, 0.75);
        assert!(!s.skin_only);
        assert!(s.show_mask);
        assert_eq!(Settings::from_json(br#"{"Radius":null}"#).radius, 6);
    }
}
