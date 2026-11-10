//! Native companion feasibility backend. RGB TIFF is the high-precision exchange
//! channel; this does not use clipboard pixels or an 8-bit rendered screenshot.
pub mod color;
pub mod control;
pub mod desktop;
pub mod exchange;
pub mod handoff;
pub mod i18n;
pub mod pcraft;
pub mod processing;
pub mod quality;
pub mod startup;
pub mod worker;
use std::{
    borrow::Cow,
    fs::File,
    io::{self, BufReader, BufWriter, Write},
    path::Path,
};
use tiff::{
    ColorType,
    decoder::{Decoder, DecodingResult, Limits, ifd::Value as TagValue},
    encoder::{Rational, TiffEncoder, TiffValue, colortype},
    tags::{Tag, Type},
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const MAX_PIXELS: usize = 36_000_000;
const MAX_SAMPLES: usize = MAX_PIXELS * 4;

#[derive(Clone, Debug)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub channels: usize,
    pub depth: u8,
    pub pixels: Vec<f32>,
    pub icc: Option<Vec<u8>>,
    pub resolution: Option<(u16, [u32; 2], [u32; 2])>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

impl Frame {
    pub fn read(path: &Path) -> Result<Self> {
        let mut limits = Limits::default();
        limits.decoding_buffer_size = MAX_SAMPLES * 4;
        let mut decoder = Decoder::new(BufReader::new(File::open(path)?))?.with_limits(limits);
        let (width, height) = decoder.dimensions()?;
        let area = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| invalid("Image size overflow"))?;
        if area == 0 || area > MAX_PIXELS {
            return Err(invalid("Native proof supports images up to 36 MP").into());
        }
        let (channels, depth) = match decoder.colortype()? {
            ColorType::RGB(d) if matches!(d, 8 | 16 | 32) => (3, d),
            ColorType::RGBA(d) if matches!(d, 8 | 16 | 32) => (4, d),
            other => return Err(invalid(format!("Unsupported TIFF layout: {other:?}")).into()),
        };
        if channels == 4 && decoder.get_tag_u16_vec(Tag::ExtraSamples)? != [2] {
            return Err(
                invalid("Only straight/unassociated alpha is supported by this proof").into(),
            );
        }
        let icc = decoder
            .find_tag(Tag::IccProfile)?
            .map(|value| value.into_u8_vec())
            .transpose()?;
        let resolution = match (
            decoder.find_tag(Tag::XResolution)?,
            decoder.find_tag(Tag::YResolution)?,
        ) {
            (None, None) => None,
            (Some(TagValue::Rational(xn, xd)), Some(TagValue::Rational(yn, yd)))
                if xd != 0 && yd != 0 =>
            {
                Some((
                    decoder
                        .find_tag_unsigned::<u16>(Tag::ResolutionUnit)?
                        .unwrap_or(2),
                    [xn, xd],
                    [yn, yd],
                ))
            }
            _ => return Err(invalid("Unsupported or invalid TIFF resolution metadata").into()),
        };
        let pixels = match decoder.read_image()? {
            DecodingResult::U8(v) if depth == 8 => {
                v.into_iter().map(|x| f32::from(x) / 255.0).collect()
            }
            DecodingResult::U16(v) if depth == 16 => {
                v.into_iter().map(|x| f32::from(x) / 65535.0).collect()
            }
            DecodingResult::F32(v) if depth == 32 => v,
            _ => {
                return Err(
                    invalid("Unsupported TIFF sample format (32-bit must be float)").into(),
                );
            }
        };
        if pixels.len() != area * channels {
            return Err(invalid("Unexpected TIFF sample count").into());
        }
        if decoder.more_images() {
            return Err(invalid("Multipage/pyramid TIFF is outside the native proof scope").into());
        }
        Ok(Self {
            width,
            height,
            channels,
            depth,
            pixels,
            icc,
            resolution,
        })
    }

    /// Create a new exchange file. Existing paths are never overwritten.
    pub fn write(&self, path: &Path) -> Result<()> {
        let area = (self.width as usize)
            .checked_mul(self.height as usize)
            .ok_or_else(|| invalid("Size overflow"))?;
        if area == 0
            || area > MAX_PIXELS
            || self.pixels.len() != area * self.channels
            || !matches!(self.channels, 3 | 4)
            || !matches!(self.depth, 8 | 16 | 32)
        {
            return Err(invalid("Invalid frame layout").into());
        }
        pcraft::atomic_new(path, |file| self.write_tiff(file))
    }
    fn write_tiff(&self, file: &mut File) -> Result<()> {
        let mut writer = BufWriter::new(file);
        let mut encoder = TiffEncoder::new(&mut writer)?;
        macro_rules! write {
            ($kind:ty,$pixels:expr) => {{
                let mut image = encoder.new_image::<$kind>(self.width, self.height)?;
                if self.channels == 4 {
                    image.encoder().write_tag(Tag::ExtraSamples, &[2u16][..])?;
                }
                if let Some(icc) = &self.icc {
                    image
                        .encoder()
                        .write_tag(Tag::IccProfile, ProfileBytes(icc))?;
                }
                if let Some((unit, x, y)) = self.resolution {
                    image.encoder().write_tag(Tag::ResolutionUnit, unit)?;
                    image
                        .encoder()
                        .write_tag(Tag::XResolution, Rational { n: x[0], d: x[1] })?;
                    image
                        .encoder()
                        .write_tag(Tag::YResolution, Rational { n: y[0], d: y[1] })?;
                }
                image.write_data($pixels)?;
            }};
        }
        match (self.channels, self.depth) {
            (3, 8) => write!(
                colortype::RGB8,
                &self
                    .pixels
                    .iter()
                    .map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8)
                    .collect::<Vec<_>>()
            ),
            (4, 8) => write!(
                colortype::RGBA8,
                &self
                    .pixels
                    .iter()
                    .map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8)
                    .collect::<Vec<_>>()
            ),
            (3, 16) => write!(
                colortype::RGB16,
                &self
                    .pixels
                    .iter()
                    .map(|x| (x.clamp(0.0, 1.0) * 65535.0).round() as u16)
                    .collect::<Vec<_>>()
            ),
            (4, 16) => write!(
                colortype::RGBA16,
                &self
                    .pixels
                    .iter()
                    .map(|x| (x.clamp(0.0, 1.0) * 65535.0).round() as u16)
                    .collect::<Vec<_>>()
            ),
            (3, 32) => write!(colortype::RGB32Float, &self.pixels),
            (4, 32) => write!(colortype::RGBA32Float, &self.pixels),
            _ => return Err(invalid("Unsupported frame depth").into()),
        }
        writer.flush()?;
        writer.get_ref().sync_all()?;
        Ok(())
    }
}

struct ProfileBytes<'a>(&'a [u8]);
impl TiffValue for ProfileBytes<'_> {
    const BYTE_LEN: u8 = 1;
    const FIELD_TYPE: Type = Type::UNDEFINED;
    fn count(&self) -> usize {
        self.0.len()
    }
    fn data(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(self.0)
    }
}

pub fn fixture(width: u32, height: u32, depth: u8, icc: Option<Vec<u8>>) -> Frame {
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            let n = (((x * 17 + y * 31 + x * y * 7) % 101) as f32 / 100.0 - 0.5) * 0.05;
            let mut p = [
                0.64 + n,
                0.44 + n,
                0.33 + n,
                if (x + y) % 19 == 0 {
                    0.0
                } else {
                    0.25 + ((x + y) % 7) as f32 * 0.1
                },
            ];
            if depth == 32 && x == 5 && y == 5 {
                p = [1.5, 0.4, 0.3, 1.0];
            }
            let scale = if depth == 8 { 255.0 } else { 65535.0 };
            if depth != 32 {
                for c in &mut p {
                    *c = (*c * scale).round() / scale;
                }
            }
            pixels.extend_from_slice(&p);
        }
    }
    Frame {
        width,
        height,
        channels: 4,
        depth,
        pixels,
        icc,
        resolution: Some((2, [300, 1], [300, 1])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn high_precision_tiff_round_trip_preserves_samples_and_profile_bytes() {
        let directory =
            std::env::temp_dir().join(format!("natural-skin-native-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        for depth in [8, 16, 32] {
            let input = fixture(23, 17, depth, Some(vec![1, 2, 3, 4, 5]));
            let path = directory.join(format!("{depth}.tif"));
            let _ = std::fs::remove_file(&path);
            input.write(&path).unwrap();
            let output = Frame::read(&path).unwrap();
            assert_eq!(input.pixels, output.pixels);
            assert_eq!(input.icc, output.icc);
            assert_eq!(input.depth, output.depth);
            assert_eq!(input.resolution, output.resolution);
            assert!(input.write(&path).is_err());
            std::fs::remove_file(path).unwrap();
        }
        std::fs::remove_dir(directory).unwrap();
    }
}
