//! Bounded, single-raster .pcraft exchange. Never flattens the source surface:
//! samples outside the canvas and edge-tile padding survive the round trip.
use crate::{Frame, Result, invalid};
use ruzstd::{
    decoding::StreamingDecoder,
    encoding::{CompressionLevel, compress_to_vec},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const TILE: usize = 256;
const MAX_ARCHIVE: u64 = 768 << 20;
const MAX_TOTAL: usize = 768 << 20;
const MAX_ENTRIES: usize = 4096;
const MAX_MANIFEST: usize = 2 << 20;
const MAX_BLOB: usize = 16 << 20;
const MAX_TILE: usize = TILE * TILE * 16;
const MAX_WINDOW: u64 = 8 << 20;
type Coordinates = (i32, i32);

pub struct PcraftDocument {
    pub frame: Frame,
    manifest: Value,
    archive: Option<ZipArchive<File>>,
    tiles: BTreeMap<Coordinates, String>,
    default: Vec<u8>,
    sample_bytes: usize,
}

impl PcraftDocument {
    pub fn layer_name(&self) -> Option<&str> {
        self.manifest["document"]["layers"][0]["name"].as_str()
    }
}

// Bound directory allocation before the ZIP crate builds its index. PhotoCraft
// writes classic single-disk ZIP; ZIP64 and self-extracting bundles are excluded.
fn preflight_zip(file: &mut File) -> Result<()> {
    let len = file.metadata()?.len();
    let tail_len = len.min(65535 + 22) as usize;
    if tail_len < 22 {
        return Err(invalid("Truncated pcraft ZIP").into());
    }
    file.seek(SeekFrom::End(-(tail_len as i64)))?;
    let mut tail = vec![0; tail_len];
    file.read_exact(&mut tail)?;
    let get16 = |p: usize| u16::from_le_bytes([tail[p], tail[p + 1]]);
    let get32 = |p: usize| u32::from_le_bytes([tail[p], tail[p + 1], tail[p + 2], tail[p + 3]]);
    let at = (0..=tail_len - 22)
        .rev()
        .find(|&p| {
            tail[p..p + 4] == [0x50, 0x4b, 0x05, 0x06]
                && p + 22 + get16(p + 20) as usize == tail_len
        })
        .ok_or("Missing pcraft ZIP directory")?;
    let count = get16(at + 10) as usize;
    let directory_size = get32(at + 12) as u64;
    let directory_offset = get32(at + 16) as u64;
    if count > MAX_ENTRIES
        || count == u16::MAX as usize
        || get16(at + 4) != 0
        || get16(at + 6) != 0
        || get16(at + 8) as usize != count
        || directory_size > MAX_ENTRIES as u64 * 1024
        || directory_offset + directory_size != len - tail_len as u64 + at as u64
    {
        return Err(invalid("Unsupported/oversized pcraft ZIP directory").into());
    }
    file.seek(SeekFrom::Start(0))?;
    Ok(())
}

fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key]
        .as_str()
        .ok_or_else(|| invalid(format!("Missing/invalid pcraft {key}")).into())
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex(s: &str) -> Result<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid("Invalid pcraft hex").into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(Into::into))
        .collect()
}
fn object_name(name: &str) -> bool {
    name.strip_prefix("tiles/")
        .or_else(|| name.strip_prefix("blobs/"))
        .and_then(|s| s.strip_suffix(".zst"))
        .is_some_and(valid_hash)
}
fn valid_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn zip_bytes(archive: &mut ZipArchive<File>, name: &str, max: usize) -> Result<Vec<u8>> {
    let mut entry = archive.by_name(name)?;
    if entry.encrypted() || entry.size() > max as u64 || entry.compressed_size() > max as u64 {
        return Err(invalid("pcraft entry exceeds budget or is encrypted").into());
    }
    let mut data = Vec::new();
    entry.by_ref().take(max as u64 + 1).read_to_end(&mut data)?;
    if data.len() > max || data.len() as u64 != entry.size() {
        return Err(invalid("pcraft ZIP size mismatch").into());
    }
    Ok(data)
}
fn object(
    archive: &mut ZipArchive<File>,
    name: &str,
    max: usize,
    budget: &mut usize,
) -> Result<Vec<u8>> {
    let compressed = zip_bytes(archive, name, max + 65536)?;
    let mut decoder =
        StreamingDecoder::new_with_max_window_size(compressed.as_slice(), MAX_WINDOW)?;
    let mut data = Vec::new();
    decoder
        .by_ref()
        .take(max as u64 + 1)
        .read_to_end(&mut data)?;
    if data.len() > max || !decoder.get_ref().is_empty() {
        return Err(invalid("pcraft object size/trailing-frame violation").into());
    }
    *budget = budget
        .checked_add(data.len())
        .filter(|n| *n <= MAX_TOTAL)
        .ok_or_else(|| invalid("pcraft decompression budget exceeded"))?;
    let expected = name
        .rsplit('/')
        .next()
        .and_then(|s| s.strip_suffix(".zst"))
        .ok_or_else(|| invalid("Invalid object path"))?;
    if blake3::hash(&data).to_hex().as_str() != expected {
        return Err(invalid("pcraft BLAKE3 mismatch").into());
    }
    Ok(data)
}
fn decode_sample(bytes: &[u8], depth: u8) -> f32 {
    match depth {
        8 => bytes[0] as f32 / 255.0,
        16 => u16::from_le_bytes([bytes[0], bytes[1]]) as f32 / 65535.0,
        _ => f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
    }
}
fn encode_sample(value: f32, depth: u8, out: &mut [u8]) {
    match depth {
        8 => out[0] = (value.clamp(0.0, 1.0) * 255.0).round() as u8,
        16 => {
            out.copy_from_slice(&((value.clamp(0.0, 1.0) * 65535.0).round() as u16).to_le_bytes())
        }
        _ => out.copy_from_slice(&value.to_le_bytes()),
    }
}

impl PcraftDocument {
    /// Only one plain visible raster layer is accepted. Other documents must be
    /// captured as a dedicated supported layer snapshot before processing.
    pub fn read(path: &Path) -> Result<Self> {
        let mut file = File::open(path)?;
        if file.metadata()?.len() > MAX_ARCHIVE {
            return Err(invalid("pcraft archive exceeds 768 MiB").into());
        }
        preflight_zip(&mut file)?;
        let mut archive = ZipArchive::new(file)?;
        if archive.has_duplicate_names() {
            return Err(invalid("Duplicate pcraft ZIP names").into());
        }
        archive.validate_header_consistency()?;
        if archive.len() > MAX_ENTRIES {
            return Err(invalid("Too many pcraft entries").into());
        }
        let mut names = HashSet::new();
        for i in 0..archive.len() {
            let f = archive.by_index(i)?;
            let name = f.name()?;
            let name = name.as_ref();
            if !names.insert(name.to_owned())
                || !(object_name(name)
                    || matches!(
                        name,
                        "manifest.json"
                            | "thumb.png"
                            | "composite/preview.png"
                            | "composite/full.png"
                    ))
            {
                return Err(invalid("Duplicate or unsupported pcraft entry").into());
            }
        }
        let raw = zip_bytes(&mut archive, "manifest.json", MAX_MANIFEST)?;
        let manifest: Value = serde_json::from_slice(&raw)?;
        let mut result = Self::from_manifest(manifest, Some(archive))?;
        let mut budget = 0;
        let refs: Vec<_> = result.tiles.iter().map(|(k, v)| (*k, v.clone())).collect();
        for (position, hash) in refs {
            let raw = object(
                result.archive.as_mut().ok_or("Missing archive")?,
                &format!("tiles/{hash}.zst"),
                MAX_TILE,
                &mut budget,
            )?;
            if raw.len() != TILE * TILE * result.frame.channels * result.sample_bytes {
                return Err(invalid("pcraft tile length mismatch").into());
            }
            result.copy_tile_to_frame(position, &raw);
        }
        // Check all blobs, not just ICC: metadata can reference other objects.
        let blobs: Vec<_> = names
            .into_iter()
            .filter(|s| s.starts_with("blobs/"))
            .collect();
        let profile = result.manifest["document"]["icc_profile"]
            .as_str()
            .map(|h| format!("blobs/{h}.zst"));
        for name in blobs {
            let data = object(
                result.archive.as_mut().ok_or("Missing archive")?,
                &name,
                MAX_BLOB,
                &mut budget,
            )?;
            if profile.as_deref() == Some(&name) {
                result.frame.icc = Some(data);
            }
        }
        if profile.is_some() && result.frame.icc.is_none() {
            return Err(invalid("Missing pcraft ICC blob").into());
        }
        Ok(result)
    }

    fn from_manifest(manifest: Value, archive: Option<ZipArchive<File>>) -> Result<Self> {
        if manifest["format_version"] != 1 {
            return Err(invalid("Unsupported pcraft format version (only 1 is supported)").into());
        }
        let doc = &manifest["document"];
        let width = u32::try_from(doc["size"]["width"].as_u64().ok_or("Invalid width")?)?;
        let height = u32::try_from(doc["size"]["height"].as_u64().ok_or("Invalid height")?)?;
        let area = u64::from(width) * u64::from(height);
        if area == 0 || area > 36_000_000 {
            return Err(invalid("pcraft canvas limit: 36 MP").into());
        }
        let layers = doc["layers"].as_array().ok_or("Missing layers")?;
        if doc["mode"] != "Rgb"
            || layers.len() != 1
            || !doc["selection"].is_null()
            || !doc["quick_mask"].is_null()
            || doc["channels"].as_array().is_none_or(|a| !a.is_empty())
            || !doc["timeline"].is_null()
        {
            return Err(invalid(
                "Use one RGB pixel layer without a selection, extra channels or timeline",
            )
            .into());
        }
        let layer = &layers[0];
        let nonempty = |v: &Value| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty());
        if layer["content"]["kind"] != "raster"
            || layer["visible"] != true
            || layer["blend"] != "Normal"
            || layer["opacity"].as_f64() != Some(1.0)
            || layer["fill_opacity"].as_f64() != Some(1.0)
            || layer["clipped"] != false
            || !layer["mask"].is_null()
            || !layer["vector_mask"].is_null()
            || nonempty(&layer["effects"]["items"])
            || !layer["fill_cache"].is_null()
            || !layer["frame"].is_null()
            || !layer["video"].is_null()
            || layer["excluded_channels"].as_u64().unwrap_or(0) != 0
            || !layer["blend_if"].is_null()
            || !layer["advanced"].is_null()
        {
            return Err(invalid("Unsupported pcraft layer appearance/content").into());
        }
        if let Some(hash) = doc["icc_profile"].as_str() {
            if !valid_hash(hash) {
                return Err(invalid("Invalid ICC hash").into());
            }
        } else if !doc["icc_profile"].is_null() {
            return Err(invalid("Invalid ICC reference").into());
        }
        let surface = &layer["content"]["surface"];
        let depth = match text(&surface["format"], "sample")? {
            "U8" => 8,
            "U16" => 16,
            "F32" => 32,
            _ => return Err(invalid("Unsupported sample type").into()),
        };
        if surface["format"]["mode"] != "Rgb" || doc["depth"] != surface["format"]["sample"] {
            return Err(invalid("pcraft surface/document format mismatch").into());
        }
        let channels = if surface["format"]["alpha"]
            .as_bool()
            .ok_or("Missing alpha layout")?
        {
            4
        } else {
            3
        };
        let sample_bytes = depth as usize / 8;
        let padded = (width as usize)
            .div_ceil(TILE)
            .checked_mul((height as usize).div_ceil(TILE))
            .ok_or("Tile grid overflow")?;
        if padded > MAX_ENTRIES - 2 || padded * TILE * TILE * channels * sample_bytes > MAX_TOTAL {
            return Err(invalid("pcraft padded tile grid exceeds budget").into());
        }
        let default = unhex(text(surface, "default")?)?;
        if default.len() != channels * sample_bytes {
            return Err(invalid("pcraft default pixel length mismatch").into());
        }
        let dpi = doc["resolution_dpi"]
            .as_f64()
            .filter(|n| n.is_finite() && *n > 0.0 && *n <= 1_000_000.0)
            .ok_or("Invalid pcraft DPI")?;
        let resolution = Some((
            2,
            [(dpi * 1000.0).round() as u32, 1000],
            [(dpi * 1000.0).round() as u32, 1000],
        ));
        let mut pixels = Vec::new();
        pixels.try_reserve_exact(area as usize * channels)?;
        for _ in 0..area {
            for sample in default.chunks_exact(sample_bytes) {
                pixels.push(decode_sample(sample, depth));
            }
        }
        let refs = surface["tiles"].as_array().ok_or("Missing surface tiles")?;
        if refs.len() > MAX_ENTRIES {
            return Err(invalid("Too many pcraft tiles").into());
        }
        let mut tiles = BTreeMap::new();
        for tile in refs {
            let x = i32::try_from(tile["tx"].as_i64().ok_or("Invalid tile x")?)?;
            let y = i32::try_from(tile["ty"].as_i64().ok_or("Invalid tile y")?)?;
            let hash = text(tile, "hash")?;
            if !valid_hash(hash) || tiles.insert((x, y), hash.to_owned()).is_some() {
                return Err(invalid("Invalid/duplicate pcraft tile reference").into());
            }
        }
        Ok(Self {
            frame: Frame {
                width,
                height,
                channels,
                depth,
                pixels,
                icc: None,
                resolution,
            },
            manifest,
            archive,
            tiles,
            default,
            sample_bytes,
        })
    }

    pub fn from_frame(frame: Frame, name: &str) -> Result<Self> {
        let sample = match frame.depth {
            8 => "U8",
            16 => "U16",
            32 => "F32",
            _ => return Err(invalid("Unsupported frame depth").into()),
        };
        if !matches!(frame.channels, 3 | 4)
            || frame.pixels.len() != frame.width as usize * frame.height as usize * frame.channels
        {
            return Err(invalid("Invalid frame layout").into());
        }
        let dpi = match frame.resolution {
            None => 72.0,
            Some((2, x, _)) if x[1] != 0 => x[0] as f64 / x[1] as f64,
            Some((3, x, _)) if x[1] != 0 => x[0] as f64 / x[1] as f64 * 2.54,
            _ => return Err(invalid("Unsupported frame resolution").into()),
        };
        if let Some((_, x, y)) = frame.resolution
            && u64::from(x[0]) * u64::from(y[1]) != u64::from(y[0]) * u64::from(x[1])
        {
            return Err(invalid("pcraft requires square DPI").into());
        }
        let manifest = json!({"format_version":1,"generator":format!("Natural Skin {}",env!("CARGO_PKG_VERSION")),"thumbnail":null,"composite":null,"document":{
            "id":1,"name":name,"size":{"width":frame.width,"height":frame.height},"resolution_dpi":dpi,"mode":"Rgb","depth":sample,"icc_profile":null,
            "layers":[{"id":2,"name":"Natural Skin","visible":true,"locks":{"transparency":false,"pixels":false,"position":false,"artboard":false,"all":false},"blend":"Normal","opacity":1.0,"fill_opacity":1.0,"clipped":false,"mask":null,"vector_mask":null,"effects":{"enabled":true,"items":[],"psd_raw":null,"reference":null},"label":"None","content":{"kind":"raster","surface":{"format":{"mode":"Rgb","sample":sample,"alpha":frame.channels==4},"default":hex(&vec![0;frame.channels*(frame.depth as usize/8)]),"tiles":[]}},"psd_blocks":[],"psd_id":null,"fill_cache":null,"link_group":null,"video":null}],
            "channels":[],"guides":{"horizontal":[],"vertical":[]},"ruler_origin":[0.0,0.0],"selection":null,"metadata":{"xmp":null,"exif":null,"psd_resources":[],"psd_global_blocks":[]},"global_light":{"angle":120.0,"altitude":30.0},"paths":[],"work_path":null,"clipping_path":null,"quick_mask":null,"patterns":[],"color_table":null,"duotone":null,"layer_comps":[],"last_applied_comp":null,"last_document_state":null,"variables":{"defs":[],"data_sets":[],"active":null},"timeline":null
        }});
        let mut doc = Self::from_manifest(manifest, None)?;
        doc.frame = frame;
        Ok(doc)
    }

    fn copy_tile_to_frame(&mut self, (tx, ty): Coordinates, raw: &[u8]) {
        let stride = self.frame.channels * self.sample_bytes;
        for row in 0..TILE {
            let y = i64::from(ty) * TILE as i64 + row as i64;
            if !(0..i64::from(self.frame.height)).contains(&y) {
                continue;
            }
            for col in 0..TILE {
                let x = i64::from(tx) * TILE as i64 + col as i64;
                if !(0..i64::from(self.frame.width)).contains(&x) {
                    continue;
                }
                let at =
                    (y as usize * self.frame.width as usize + x as usize) * self.frame.channels;
                for c in 0..self.frame.channels {
                    let p = (row * TILE + col) * stride + c * self.sample_bytes;
                    self.frame.pixels[at + c] =
                        decode_sample(&raw[p..p + self.sample_bytes], self.frame.depth);
                }
            }
        }
    }

    /// Writes a new bundle through a same-directory temporary file and publishes
    /// it without replacing an existing path. Stale flattened previews are omitted.
    pub fn write(&mut self, path: &Path) -> Result<()> {
        let doc = &self.manifest["document"];
        let expected_depth = match doc["depth"].as_str() {
            Some("U8") => 8,
            Some("U16") => 16,
            Some("F32") => 32,
            _ => return Err(invalid("Invalid pcraft depth").into()),
        };
        if self.frame.depth != expected_depth
            || doc["size"]["width"].as_u64() != Some(u64::from(self.frame.width))
            || doc["size"]["height"].as_u64() != Some(u64::from(self.frame.height))
            || self.frame.depth as usize / 8 != self.sample_bytes
            || self.frame.channels * self.sample_bytes != self.default.len()
            || self.frame.pixels.len()
                != self.frame.width as usize * self.frame.height as usize * self.frame.channels
        {
            return Err(invalid("pcraft frame geometry/layout changed").into());
        }
        atomic_new(path, |file| self.write_to(file))
    }
    fn write_to(&mut self, file: &mut File) -> Result<()> {
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o600);
        let mut emitted = HashSet::new();
        let mut budget = 0;
        let mut manifest = self.manifest.clone();
        manifest["generator"] = json!(format!("Natural Skin {}", env!("CARGO_PKG_VERSION")));
        manifest["thumbnail"] = Value::Null;
        manifest["composite"] = Value::Null;
        if let Some(m) = manifest.as_object_mut() {
            m.remove("full_composite");
        }
        if let Some(archive) = self.archive.as_mut() {
            let names: Vec<_> = archive
                .file_names()
                .map(|n| n.map(|n| n.into_owned()))
                .collect::<std::result::Result<_, _>>()?;
            let blobs = names.into_iter().filter(|n| n.starts_with("blobs/"));
            for name in blobs {
                let data = object(archive, &name, MAX_BLOB, &mut budget)?;
                if emitted.insert(name.clone()) {
                    write_object(&mut zip, &name, &data, options)?;
                }
            }
        }
        if let Some(icc) = &self.frame.icc {
            if icc.len() > MAX_BLOB {
                return Err(invalid("ICC exceeds budget").into());
            }
            let hash = blake3::hash(icc).to_hex().to_string();
            let name = format!("blobs/{hash}.zst");
            if emitted.insert(name.clone()) {
                write_object(&mut zip, &name, icc, options)?;
            }
            manifest["document"]["icc_profile"] = json!(hash);
        } else {
            manifest["document"]["icc_profile"] = Value::Null;
        }
        let mut refs = Vec::new();
        let mut coordinates: std::collections::BTreeSet<_> = self.tiles.keys().copied().collect();
        for ty in 0..(self.frame.height as usize).div_ceil(TILE) {
            for tx in 0..(self.frame.width as usize).div_ceil(TILE) {
                coordinates.insert((tx as i32, ty as i32));
            }
        }
        for (tx, ty) in coordinates {
            let mut raw = if let Some(hash) = self.tiles.get(&(tx, ty)) {
                object(
                    self.archive.as_mut().ok_or("Missing tile source")?,
                    &format!("tiles/{hash}.zst"),
                    MAX_TILE,
                    &mut budget,
                )?
            } else {
                self.default.repeat(TILE * TILE)
            };
            let stride = self.frame.channels * self.sample_bytes;
            if raw.len() != TILE * TILE * stride {
                return Err(invalid("pcraft tile changed length").into());
            }
            for row in 0..TILE {
                let y = i64::from(ty) * TILE as i64 + row as i64;
                if !(0..i64::from(self.frame.height)).contains(&y) {
                    continue;
                }
                for col in 0..TILE {
                    let x = i64::from(tx) * TILE as i64 + col as i64;
                    if !(0..i64::from(self.frame.width)).contains(&x) {
                        continue;
                    }
                    let at =
                        (y as usize * self.frame.width as usize + x as usize) * self.frame.channels;
                    for c in 0..self.frame.channels {
                        let p = (row * TILE + col) * stride + c * self.sample_bytes;
                        encode_sample(
                            self.frame.pixels[at + c],
                            self.frame.depth,
                            &mut raw[p..p + self.sample_bytes],
                        );
                    }
                }
            }
            let hash = blake3::hash(&raw).to_hex().to_string();
            let name = format!("tiles/{hash}.zst");
            if emitted.insert(name.clone()) {
                write_object(&mut zip, &name, &raw, options)?;
            }
            refs.push(json!({"tx":tx,"ty":ty,"hash":hash}));
        }
        manifest["document"]["layers"][0]["content"]["surface"]["tiles"] = json!(refs);
        let bytes = serde_json::to_vec(&manifest)?;
        if bytes.len() > MAX_MANIFEST {
            return Err(invalid("Output manifest exceeds budget").into());
        }
        zip.start_file("manifest.json", options)?;
        zip.write_all(&bytes)?;
        let file = zip.finish()?;
        if emitted.len() + 1 > MAX_ENTRIES || file.metadata()?.len() > MAX_ARCHIVE {
            return Err(invalid("Output pcraft exceeds archive budget").into());
        }
        file.sync_all()?;
        Ok(())
    }
}
fn write_object(
    zip: &mut ZipWriter<&mut File>,
    name: &str,
    raw: &[u8],
    options: SimpleFileOptions,
) -> Result<()> {
    let data = compress_to_vec(raw, CompressionLevel::Fastest);
    zip.start_file(name, options)?;
    zip.write_all(&data)?;
    Ok(())
}
pub(crate) fn atomic_new(path: &Path, write: impl FnOnce(&mut File) -> Result<()>) -> Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    if path.try_exists()? {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "Output already exists",
        )
        .into());
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temp = parent.join(format!(
        ".natural-skin-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp)?;
    let result = write(&mut file).and_then(|()| fs::hard_link(&temp, path).map_err(Into::into));
    drop(file);
    let cleanup = fs::remove_file(&temp);
    result?;
    cleanup?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;
    use std::path::PathBuf;

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            static SEQUENCE: AtomicU64 = AtomicU64::new(0);
            let p = std::env::temp_dir().join(format!(
                "natural-skin-pcraft-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn bits(f: &Frame) -> Vec<u32> {
        f.pixels.iter().map(|n| n.to_bits()).collect()
    }
    fn bundle(path: &Path, manifest: &Value, objects: &[(String, Vec<u8>)]) {
        let mut zip = ZipWriter::new(File::create(path).unwrap());
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("manifest.json", options).unwrap();
        zip.write_all(&serde_json::to_vec(manifest).unwrap())
            .unwrap();
        for (name, raw) in objects {
            zip.start_file(name, options).unwrap();
            zip.write_all(&compress_to_vec(raw.as_slice(), CompressionLevel::Fastest))
                .unwrap();
        }
        zip.finish().unwrap();
    }
    #[test]
    fn rgb_rgba_integer_float_round_trip_across_tile_boundary() {
        let dir = Directory::new();
        for depth in [8, 16, 32] {
            for channels in [3, 4] {
                let mut frame = fixture(257, 3, depth, Some(vec![1, 3, 7, 11]));
                if channels == 3 {
                    frame.pixels = frame
                        .pixels
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .flat_map(|p| p[..3].to_vec())
                        .collect();
                    frame.channels = 3;
                }
                if depth == 32 {
                    frame.pixels[1] = f32::from_bits(0x7fc01234);
                    frame.pixels[2] = -0.125;
                }
                let expected = bits(&frame);
                let file = dir.path(&format!("{depth}-{channels}.pcraft"));
                PcraftDocument::from_frame(frame.clone(), "Precision")
                    .unwrap()
                    .write(&file)
                    .unwrap();
                let read = PcraftDocument::read(&file).unwrap();
                assert_eq!(bits(&read.frame), expected);
                assert_eq!(read.frame.icc, frame.icc);
                assert_eq!(read.frame.depth, depth);
                assert_eq!(read.frame.channels, channels);
                assert_eq!(read.manifest["document"]["resolution_dpi"], 300.0);
                assert_eq!(read.tiles.len(), 2);
            }
        }
    }
    #[test]
    fn preserves_off_canvas_coordinates_padding_defaults_and_metadata() {
        let dir = Directory::new();
        let mut base = PcraftDocument::from_frame(fixture(3, 2, 32, None), "Offset").unwrap();
        base.manifest["document"]["metadata"]["xmp"] = json!("Keep this exact metadata");
        let tile: Vec<u8> = (0..TILE * TILE)
            .flat_map(|_| {
                [0.75f32, 0.25, 0.125, 0.0]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
            })
            .collect();
        let hash = blake3::hash(&tile).to_hex().to_string();
        base.manifest["document"]["layers"][0]["content"]["surface"]["default"] =
            json!(hex(&[0.5f32, 0.4, 0.3, 0.8]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>()));
        base.manifest["document"]["layers"][0]["content"]["surface"]["tiles"] = json!([{"tx":-1,"ty":0,"hash":hash},{"tx":0,"ty":0,"hash":hash},{"tx":2,"ty":-2,"hash":hash}]);
        let input = dir.path("source.pcraft");
        bundle(
            &input,
            &base.manifest,
            &[(format!("tiles/{hash}.zst"), tile.clone())],
        );
        let source_bytes = fs::read(&input).unwrap();
        let mut read = PcraftDocument::read(&input).unwrap();
        assert_eq!(&read.frame.pixels[..4], &[0.75, 0.25, 0.125, 0.0]);
        read.frame.pixels[0] = 0.625;
        let output = dir.path("result.pcraft");
        read.write(&output).unwrap();
        let mut result = PcraftDocument::read(&output).unwrap();
        assert_eq!(result.frame.pixels[0], 0.625);
        assert_eq!(result.tiles[&(-1, 0)], hash);
        assert_eq!(result.tiles[&(2, -2)], hash);
        let changed_hash = result.tiles[&(0, 0)].clone();
        let raw = object(
            result.archive.as_mut().unwrap(),
            &format!("tiles/{changed_hash}.zst"),
            MAX_TILE,
            &mut 0,
        )
        .unwrap();
        assert_eq!(&raw[4..], &tile[4..]);
        assert_eq!(result.default, read.default);
        assert_eq!(
            result.manifest["document"]["metadata"],
            read.manifest["document"]["metadata"]
        );
        assert_eq!(fs::read(input).unwrap(), source_bytes);
    }
    #[test]
    fn reads_sparse_default_without_tiles() {
        let dir = Directory::new();
        let mut base = PcraftDocument::from_frame(fixture(2, 3, 16, None), "Sparse").unwrap();
        base.manifest["document"]["layers"][0]["content"]["surface"]["default"] =
            json!("ffff008000000000");
        let path = dir.path("sparse.pcraft");
        bundle(&path, &base.manifest, &[]);
        let mut read = PcraftDocument::read(&path).unwrap();
        assert_eq!(read.frame.pixels[0], 1.0);
        assert_eq!(read.frame.pixels[3], 0.0);
        let output = dir.path("sparse-out.pcraft");
        read.write(&output).unwrap();
        assert_eq!(
            bits(&PcraftDocument::read(&output).unwrap().frame),
            bits(&read.frame)
        );
    }
    #[test]
    fn refuses_newer_complex_oversized_and_duplicate_tile_manifests() {
        let base = PcraftDocument::from_frame(fixture(2, 2, 32, None), "Validation")
            .unwrap()
            .manifest;
        let mut cases = Vec::new();
        let mut m = base.clone();
        m["format_version"] = json!(2);
        cases.push(m);
        let mut m = base.clone();
        m["document"]["layers"][0]["mask"] = json!({});
        cases.push(m);
        let mut m = base.clone();
        m["document"]["size"] = json!({"width":1,"height":36_000_000});
        cases.push(m);
        let mut m = base.clone();
        m["document"]["layers"][0]["content"]["surface"]["tiles"] =
            json!([{"tx":0,"ty":0,"hash":"../bad"}]);
        cases.push(m);
        let mut m = base.clone();
        let h = "a".repeat(64);
        m["document"]["layers"][0]["content"]["surface"]["tiles"] =
            json!([{"tx":0,"ty":0,"hash":h},{"tx":0,"ty":0,"hash":h}]);
        cases.push(m);
        for m in cases {
            assert!(PcraftDocument::from_manifest(m, None).is_err());
        }
    }
    #[test]
    fn rejects_hash_mismatch_missing_objects_and_oversized_decoded_tiles() {
        let dir = Directory::new();
        let mut base = PcraftDocument::from_frame(fixture(2, 2, 32, None), "Corruption")
            .unwrap()
            .manifest;
        let hash = "a".repeat(64);
        base["document"]["layers"][0]["content"]["surface"]["tiles"] =
            json!([{"tx":0,"ty":0,"hash":hash}]);
        let path = dir.path("bad.pcraft");
        bundle(
            &path,
            &base,
            &[(format!("tiles/{hash}.zst"), vec![0; MAX_TILE])],
        );
        assert!(
            PcraftDocument::read(&path)
                .err()
                .unwrap()
                .to_string()
                .contains("BLAKE3")
        );
        let path = dir.path("missing.pcraft");
        bundle(&path, &base, &[]);
        assert!(PcraftDocument::read(&path).is_err());
        let path = dir.path("large.pcraft");
        bundle(
            &path,
            &base,
            &[(format!("tiles/{hash}.zst"), vec![0; MAX_TILE + 1])],
        );
        assert!(PcraftDocument::read(&path).is_err());
    }
    #[test]
    fn rejects_duplicate_zip_names_and_huge_directory_before_indexing() {
        let dir = Directory::new();
        let path = dir.path("duplicates.pcraft");
        let base = PcraftDocument::from_frame(fixture(2, 2, 32, None), "ZIP").unwrap();
        let payload = serde_json::to_vec(&base.manifest).unwrap();
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        for name in ["manifest.json", "xanifest.json"] {
            zip.start_file(
                name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(&payload).unwrap();
        }
        zip.finish().unwrap();
        let mut bytes = fs::read(&path).unwrap();
        for i in 0..bytes.len() - 13 {
            if bytes[i..i + 13] == *b"xanifest.json" {
                bytes[i] = b'm';
            }
        }
        fs::write(&path, bytes).unwrap();
        assert!(
            PcraftDocument::read(&path)
                .err()
                .unwrap()
                .to_string()
                .contains("Duplicate")
        );
        let forged = dir.path("directory.pcraft");
        let mut bytes = vec![0u8; 22];
        bytes[..4].copy_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
        bytes[8..12].fill(255);
        fs::write(&forged, bytes).unwrap();
        assert!(
            PcraftDocument::read(&forged)
                .err()
                .unwrap()
                .to_string()
                .contains("directory")
        );
    }

    #[test]
    fn failed_write_publishes_nothing_and_never_overwrites() {
        let dir = Directory::new();
        let mut doc = PcraftDocument::from_frame(fixture(2, 2, 32, None), "Atomic").unwrap();
        doc.frame.depth = 9;
        assert!(doc.write(&dir.path("invalid-depth.pcraft")).is_err());
        doc.frame.depth = 32;
        let existing = dir.path("existing.pcraft");
        fs::write(&existing, b"keep").unwrap();
        assert!(doc.write(&existing).is_err());
        assert_eq!(fs::read(existing).unwrap(), b"keep");
        doc.manifest["document"]["metadata"]["xmp"] = json!("x".repeat(MAX_MANIFEST));
        let failed = dir.path("failed.pcraft");
        assert!(doc.write(&failed).is_err());
        assert!(!failed.exists());
        assert!(
            !fs::read_dir(&dir.0).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp"))
        );
    }
}
