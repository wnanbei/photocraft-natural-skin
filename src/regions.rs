//! Bounded skin-cell planner. Merge overlapping context rectangles before writing
//! any pixels, so each independent region always reads unchanged neighbours.
#[cfg(test)]
use crate::{PLANES, process_region};
use crate::{Region, Settings, skin_gate, valid_rgb};

const CELL: usize = 64;
// Many disconnected fragments are a poor fit for repeated neighbourhood kernels.
// Fall back early instead of spending a full image scan trying to separate them.
const MAX_REGIONS: usize = 4;
const EMPTY: Region = Region {
    x: 0,
    y: 0,
    width: 0,
    height: 0,
};

pub(super) struct Plan {
    regions: [Region; MAX_REGIONS],
    len: usize,
}

impl Plan {
    pub(super) fn rectangles(&self) -> &[Region] {
        &self.regions[..self.len]
    }
    fn whole(region: Region) -> Self {
        let mut p = Self {
            regions: [EMPTY; MAX_REGIONS],
            len: 1,
        };
        p.regions[0] = region;
        p
    }
    pub(super) fn max_area(&self) -> usize {
        self.regions[..self.len]
            .iter()
            .map(|r| r.width * r.height)
            .max()
            .unwrap_or(0)
    }
    fn work(&self) -> usize {
        self.regions[..self.len]
            .iter()
            .map(|r| r.width * r.height)
            .sum()
    }
}

fn overlaps(a: Region, b: Region) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

fn union(a: Region, b: Region) -> Region {
    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
    Region {
        x,
        y,
        width: (a.x + a.width).max(b.x + b.width) - x,
        height: (a.y + a.height).max(b.y + b.height) - y,
    }
}

#[inline]
fn skin_pixel(pixel: &[f32], alpha: bool, settings: Settings, red_floor: f32) -> bool {
    // This is an existing exact-zero condition in skin_gate, moved ahead of its
    // chroma arithmetic for the planner's many background probes.
    pixel[0] - pixel[2] > red_floor
        && valid_rgb(pixel)
        && (!alpha || (pixel[3].is_finite() && pixel[3] > 0.0))
        && skin_gate(pixel[0], pixel[1], pixel[2], settings.skin_range) > 0.0
}

pub(super) fn plan(
    pixels: &[f32],
    w: usize,
    h: usize,
    channels: usize,
    alpha: bool,
    settings: Settings,
    whole: Region,
) -> Plan {
    if !settings.skin_only || whole.width * whole.height <= CELL * CELL * 2 {
        return Plan::whole(whole);
    }
    // This only selects the original safe path; a false dense guess cannot alter
    // pixels. Avoid planning a full skin frame or an already tight solid patch.
    let halo = settings.radius * 2;
    let red_floor = -0.025 - settings.skin_range * 0.055;
    let inset_x = halo.min((whole.width - 1) / 2);
    let inset_y = halo.min((whole.height - 1) / 2);
    let dense = [1, 2, 3].iter().all(|&qy| {
        [1, 2, 3].iter().all(|&qx| {
            let x = whole.x + inset_x + (whole.width - 1 - inset_x * 2) * qx / 4;
            let y = whole.y + inset_y + (whole.height - 1 - inset_y * 2) * qy / 4;
            let i = (y * w + x) * channels;
            let pixel = &pixels[i..i + channels];
            skin_pixel(pixel, alpha, settings, red_floor)
        })
    });
    if dense {
        return Plan::whole(whole);
    }
    let mut p = Plan {
        regions: [EMPTY; MAX_REGIONS],
        len: 0,
    };
    let (end_x, end_y) = (whole.x + whole.width, whole.y + whole.height);
    let mut tested = 0;
    let mut hits = 0;
    for y in (whole.y..end_y).step_by(CELL) {
        for x in (whole.x..end_x).step_by(CELL) {
            let (right, bottom) = ((x + CELL).min(end_x), (y + CELL).min(end_y));
            let hit = (y..bottom).any(|yy| {
                (x..right).any(|xx| {
                    let i = (yy * w + xx) * channels;
                    let pixel = &pixels[i..i + channels];
                    skin_pixel(pixel, alpha, settings, red_floor)
                })
            });
            tested += 1;
            hits += usize::from(hit);
            // A tiny skin speck in every cell is sparse in pixels but dense at
            // this grid size. Such a plan cannot skip much work; stop early.
            if tested == 8 && hits == 8 {
                return Plan::whole(whole);
            }
            if !hit {
                continue;
            }
            let (left, top) = (
                x.saturating_sub(halo).max(whole.x),
                y.saturating_sub(halo).max(whole.y),
            );
            let mut r = Region {
                x: left,
                y: top,
                width: (right + halo).min(w).min(end_x) - left,
                height: (bottom + halo).min(h).min(end_y) - top,
            };
            let mut index = 0;
            while index < p.len {
                if overlaps(r, p.regions[index]) {
                    r = union(r, p.regions[index]);
                    p.len -= 1;
                    p.regions[index] = p.regions[p.len];
                    // A growing bounding rectangle can meet an earlier rectangle.
                    index = 0;
                } else {
                    index += 1;
                }
            }
            if p.len == MAX_REGIONS {
                return Plan::whole(whole);
            }
            p.regions[p.len] = r;
            p.len += 1;
            // Dense, connected and expensive fragmented masks keep the original path.
            if p.work() as u64 * 10 >= (whole.width as u64 * whole.height as u64) * 7 {
                return Plan::whole(whole);
            }
        }
    }
    if p.len <= 1 { Plan::whole(whole) } else { p }
}

#[cfg(test)]
pub(super) fn run(
    pixels: &mut [f32],
    w: usize,
    channels: usize,
    alpha: bool,
    settings: Settings,
    p: &Plan,
    scratch: &mut [f32],
) -> i32 {
    for &r in &p.regions[..p.len] {
        let needed = r.width * r.height * PLANES;
        let code = process_region(
            pixels,
            w,
            channels,
            alpha,
            settings,
            r,
            &mut scratch[..needed],
        );
        if code != 0 {
            return code;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{process, processing_region};
    #[test]
    fn background_shortcut_matches_the_skin_gate() {
        for skin_range in [0.0, 0.2, 0.6, 1.0] {
            let settings = Settings {
                skin_range,
                ..Settings::default()
            };
            let red_floor = -0.025 - skin_range * 0.055;
            for r in 0..=32 {
                for g in 0..=32 {
                    for b in 0..=32 {
                        let p = [r as f32 / 32.0, g as f32 / 32.0, b as f32 / 32.0, 0.5];
                        assert_eq!(
                            skin_pixel(&p, true, settings, red_floor),
                            skin_gate(p[0], p[1], p[2], skin_range) > 0.0
                        );
                    }
                }
            }
            for p in [
                [f32::NAN, 0.4, 0.3, 1.0],
                [1.5, 0.4, 0.3, 1.0],
                [0.64, 0.44, 0.33, 0.0],
                [0.64, 0.44, 0.33, f32::NAN],
            ] {
                assert!(!skin_pixel(&p, true, settings, red_floor));
            }
        }
    }
    #[test]
    fn isolated_patches_keep_disjoint_context_and_match_the_original_filter() {
        let (w, h) = (513, 301);
        for radius in [1, 6, 20] {
            for alpha in [false, true] {
                let channels = if alpha { 4 } else { 3 };
                let mut pixels = Vec::new();
                for y in 0..h {
                    for x in 0..w {
                        let skin = ((8..46).contains(&x) || (w - 49..w - 7).contains(&x))
                            && (57..h - 58).contains(&y);
                        let n = ((x * 17 + y * 31) % 101) as f32 / 100.0 * 0.05;
                        pixels.extend_from_slice(&if skin {
                            [0.64 + n, 0.44 + n, 0.33 + n]
                        } else {
                            [0.1, 0.3, 0.9]
                        });
                        if alpha {
                            pixels.push(if (x + y) % 13 == 0 { 0.0 } else { 0.5 });
                        }
                    }
                }
                pixels[(70 * w + 15) * channels] = f32::NAN;
                pixels[(90 * w + 30) * channels] = 1.2;
                let settings = Settings {
                    radius,
                    brightening: 0.5,
                    rosiness: -0.3,
                    ..Settings::default()
                };
                let whole = processing_region(&pixels, w, h, channels, alpha, settings).unwrap();
                let p = plan(&pixels, w, h, channels, alpha, settings, whole);
                assert!(p.len >= 2);
                for i in 0..p.len {
                    for j in i + 1..p.len {
                        assert!(!overlaps(p.regions[i], p.regions[j]));
                    }
                }
                assert!(p.work() < whole.width * whole.height);
                let mut expected = pixels.clone();
                let mut actual = pixels.clone();
                let mut scratch = vec![0.0; whole.width * whole.height * PLANES];
                assert_eq!(
                    process_region(
                        &mut expected,
                        w,
                        channels,
                        alpha,
                        settings,
                        whole,
                        &mut scratch
                    ),
                    0
                );
                let mut small = vec![0.0; p.max_area() * PLANES];
                assert_eq!(
                    run(&mut actual, w, channels, alpha, settings, &p, &mut small),
                    0
                );
                // Exercise the public processing path with only the planned scratch.
                let mut integrated = pixels.clone();
                assert_eq!(
                    process(&mut integrated, w, h, channels, alpha, settings, &mut small),
                    0
                );
                for (a, b) in integrated.iter().zip(&expected) {
                    assert!(a.to_bits() == b.to_bits() || (a - b).abs() < 2e-6);
                }
                for (a, b) in actual.iter().zip(&expected) {
                    assert!(
                        a.to_bits() == b.to_bits() || (a - b).abs() < 2e-6,
                        "{a} != {b}"
                    );
                }
            }
        }
    }
    #[test]
    fn dense_and_unrestricted_inputs_keep_the_original_path() {
        let (w, h) = (257, 193);
        let pixels = [0.64, 0.44, 0.33, 1.0].repeat(w * h);
        for skin_only in [false, true] {
            let s = Settings {
                skin_only,
                ..Settings::default()
            };
            let whole = Region {
                x: 0,
                y: 0,
                width: w,
                height: h,
            };
            let p = plan(&pixels, w, h, 4, true, s, whole);
            assert_eq!(p.len, 1);
            assert_eq!(p.regions[0], whole);
        }
    }
}
