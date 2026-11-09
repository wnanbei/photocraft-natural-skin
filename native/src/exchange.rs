//! File-based exchange used by both the CLI and the future GUI worker.
use crate::{Frame, Result, pcraft::PcraftDocument};
use std::path::Path;

pub enum ImageDocument {
    Tiff(Frame),
    Pcraft(Box<PcraftDocument>),
}
fn is_pcraft(path: &Path) -> bool {
    path.extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("pcraft"))
}
impl ImageDocument {
    pub fn read(path: &Path) -> Result<Self> {
        if is_pcraft(path) {
            Ok(Self::Pcraft(Box::new(PcraftDocument::read(path)?)))
        } else {
            Ok(Self::Tiff(Frame::read(path)?))
        }
    }
    pub fn frame(&self) -> &Frame {
        match self {
            Self::Tiff(f) => f,
            Self::Pcraft(p) => &p.frame,
        }
    }
    pub fn frame_mut(&mut self) -> &mut Frame {
        match self {
            Self::Tiff(f) => f,
            Self::Pcraft(p) => &mut p.frame,
        }
    }
    pub fn write(&mut self, path: &Path) -> Result<()> {
        if is_pcraft(path) {
            match self {
                Self::Pcraft(p) => p.write(path),
                Self::Tiff(f) => PcraftDocument::from_frame(f.clone(), "Natural Skin")?.write(path),
            }
        } else {
            // TIFF conversion intentionally includes only the canvas. A pcraft
            // output is required to retain off-canvas tiles and document metadata.
            self.frame().write(path)
        }
    }
}
