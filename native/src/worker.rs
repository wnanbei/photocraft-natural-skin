//! One background worker, one pending request, generation-checked publication.
use crate::{
    Frame, Result,
    color::{ColorPipeline, DisplayTransform},
    exchange::ImageDocument,
    processing::{self, Crop},
};
use natural_skin_core::{NativePreview, NativeStage};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::Instant,
};

pub enum Job {
    Load(PathBuf),
    Preview(Value),
    Detail(Crop, Value),
    Export(PathBuf, Value),
}
#[derive(Clone)]
pub struct PreviewPixels {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}
pub enum Event {
    Progress {
        id: u64,
        stage: String,
        completed: usize,
        total: usize,
    },
    Loaded {
        id: u64,
        width: u32,
        height: u32,
        depth: u8,
        original: PreviewPixels,
        label: String,
        colour: String,
    },
    Rendered {
        id: u64,
        pixels: PreviewPixels,
        cache_hit: bool,
        original: Option<PreviewPixels>,
        milliseconds: f64,
    },
    Exported {
        id: u64,
        path: PathBuf,
        milliseconds: f64,
        processing_ms: f64,
        write_ms: f64,
        tile_buffer_bytes: usize,
    },
    Failed {
        id: u64,
        message: String,
    },
}
impl Event {
    pub fn id(&self) -> u64 {
        match self {
            Self::Progress { id, .. }
            | Self::Loaded { id, .. }
            | Self::Rendered { id, .. }
            | Self::Exported { id, .. }
            | Self::Failed { id, .. } => *id,
        }
    }
}
struct Queue {
    pending: Option<(u64, Job)>,
    committing: bool,
    shutdown: bool,
}
struct Shared {
    generation: AtomicU64,
    queue: Mutex<Queue>,
    wake: Condvar,
}
pub struct Worker {
    shared: Arc<Shared>,
    pub events: Receiver<Event>,
}
impl Worker {
    pub fn new(repaint: impl Fn() + Send + 'static) -> Self {
        let shared = Arc::new(Shared {
            generation: AtomicU64::new(0),
            queue: Mutex::new(Queue {
                pending: None,
                committing: false,
                shutdown: false,
            }),
            wake: Condvar::new(),
        });
        let (sender, events) = mpsc::channel();
        let state = shared.clone();
        thread::spawn(move || {
            let send = |event| {
                let _ = sender.send(event);
                repaint();
            };
            let mut session = None;
            loop {
                let next = {
                    let mut queue = state.queue.lock().unwrap_or_else(|e| e.into_inner());
                    while queue.pending.is_none() && !queue.shutdown {
                        queue = state.wake.wait(queue).unwrap_or_else(|e| e.into_inner());
                    }
                    if queue.shutdown {
                        break;
                    }
                    queue.pending.take()
                };
                if let Some((id, job)) = next {
                    let result = run(&state, id, job, &mut session, &send);
                    state
                        .queue
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .committing = false;
                    if let Err(e) = result
                        && state.generation.load(Ordering::Acquire) == id
                    {
                        send(Event::Failed {
                            id,
                            message: e.to_string(),
                        });
                    }
                }
            }
        });
        Self { shared, events }
    }
    pub fn submit(&self, job: Job) -> Option<u64> {
        let mut queue = self.shared.queue.lock().unwrap_or_else(|e| e.into_inner());
        if queue.committing {
            return None;
        }
        let id = self.shared.generation.fetch_add(1, Ordering::AcqRel) + 1;
        queue.pending = Some((id, job));
        self.shared.wake.notify_one();
        Some(id)
    }
    /// False means the completed file is already being committed. A cancellation
    /// never claims success after that point; wait for Exported/Failed instead.
    pub fn cancel(&self) -> bool {
        let mut queue = self.shared.queue.lock().unwrap_or_else(|e| e.into_inner());
        if queue.committing {
            return false;
        }
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        queue.pending = None;
        true
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let mut q = self.shared.queue.lock().unwrap_or_else(|e| e.into_inner());
        q.shutdown = true;
        q.pending = None;
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        self.shared.wake.notify_one();
    }
}
struct Session {
    document: ImageDocument,
    preview: NativePreview,
    width: usize,
    height: usize,
    scale: f64,
    cms: ColorPipeline,
}
fn current(shared: &Shared, id: u64) -> bool {
    shared.generation.load(Ordering::Acquire) == id
}
fn run(
    shared: &Shared,
    id: u64,
    job: Job,
    session: &mut Option<Session>,
    send: &impl Fn(Event),
) -> Result<()> {
    let start = Instant::now();
    let last_progress = std::cell::RefCell::new((String::new(), Instant::now()));
    let progress = |stage: String, completed, total| {
        if current(shared, id) {
            let mut last = last_progress.borrow_mut();
            if last.0 == stage
                && !(total > 0 && completed == total)
                && last.1.elapsed().as_millis() < 33
            {
                return true;
            }
            *last = (stage.clone(), Instant::now());
            send(Event::Progress {
                id,
                stage,
                completed,
                total,
            });
            true
        } else {
            false
        }
    };
    match job {
        Job::Load(path) => {
            *session = None;
            progress("Reading image".into(), 0, 0);
            let document = ImageDocument::read(&path)?;
            let label = match &document {
                ImageDocument::Pcraft(doc) => doc.layer_name().unwrap_or("Pixel layer").to_owned(),
                ImageDocument::Tiff(_) => path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            };
            if !current(shared, id) {
                return Ok(());
            }
            progress("Preparing preview".into(), 0, 0);
            let frame = document.frame();
            let cms = ColorPipeline::new(frame.icc.as_deref())?;
            let (source, width, height, scale) = proxy(frame);
            let source = cms.to_working(&source, frame.channels)?;
            let original = display_pixels(&source, width, height, frame.channels);
            let preview = NativePreview::new(source, width, height, frame.channels)
                .map_err(|e| format!("{e:?}"))?;
            let (full_width, full_height, depth) = (frame.width, frame.height, frame.depth);
            let colour = cms.description.clone();
            *session = Some(Session {
                document,
                preview,
                width,
                height,
                scale,
                cms,
            });
            if current(shared, id) {
                send(Event::Loaded {
                    id,
                    width: full_width,
                    height: full_height,
                    depth,
                    original,
                    label,
                    colour,
                });
            }
        }
        Job::Preview(mut parameters) => {
            let session = session.as_mut().ok_or("Open an image first")?;
            // Radius is specified in full-resolution pixels. The fit preview is
            // approximate; final export always runs on the immutable full image.
            if let Some(radius) = parameters["Radius"].as_f64() {
                parameters["Radius"] =
                    serde_json::json!((radius * session.scale).round().clamp(1.0, 20.0));
            }
            let display = DisplayTransform::from_path(
                parameters["_displayProfile"]
                    .as_str()
                    .map(std::path::Path::new),
            )?;
            let bytes = serde_json::to_vec(&parameters)?;
            let (pixels, report) = session
                .preview
                .render(&bytes, |p| {
                    progress(stage(p.stage).into(), p.completed, p.total)
                })
                .map_err(|e| format!("{e:?}"))?;
            if current(shared, id) {
                send(Event::Rendered {
                    id,
                    pixels: display_pixels(
                        &display.convert(&pixels, session.document.frame().channels)?,
                        session.width,
                        session.height,
                        session.document.frame().channels,
                    ),
                    cache_hit: report.cache_hit,
                    original: Some(display_pixels(
                        &display
                            .convert(session.preview.source(), session.document.frame().channels)?,
                        session.width,
                        session.height,
                        session.document.frame().channels,
                    )),
                    milliseconds: start.elapsed().as_secs_f64() * 1000.0,
                });
            }
        }
        Job::Detail(crop, parameters) => {
            let session = session.as_ref().ok_or("Open an image first")?;
            let frame = session.document.frame();
            let display = DisplayTransform::from_path(
                parameters["_displayProfile"]
                    .as_str()
                    .map(std::path::Path::new),
            )?;
            let (pixels, _) =
                processing::render_crop(frame, crop, &parameters, &session.cms, |p| {
                    progress(stage(p.stage).into(), p.completed, p.total)
                })?;
            let source = processing::copy_crop(frame, crop)?;
            let original = session.cms.to_working(&source, frame.channels)?;
            let pixels = if parameters["Show_Mask"].as_bool() == Some(true) {
                pixels
            } else {
                session.cms.to_working(&pixels, frame.channels)?
            };
            if current(shared, id) {
                send(Event::Rendered {
                    id,
                    pixels: display_pixels(
                        &display.convert(&pixels, frame.channels)?,
                        crop.width,
                        crop.height,
                        frame.channels,
                    ),
                    original: Some(display_pixels(
                        &display.convert(&original, frame.channels)?,
                        crop.width,
                        crop.height,
                        frame.channels,
                    )),
                    cache_hit: false,
                    milliseconds: start.elapsed().as_secs_f64() * 1000.0,
                });
            }
        }
        Job::Export(path, parameters) => {
            let session = session.as_mut().ok_or("Open an image first")?;
            let frame = session.document.frame();
            let (mut working, report) =
                processing::process(frame, &parameters, &session.cms, |p| {
                    progress(stage(p.stage).into(), p.completed, p.total)
                })?;
            let processing_ms = start.elapsed().as_secs_f64() * 1000.0;
            {
                let mut queue = shared.queue.lock().unwrap_or_else(|e| e.into_inner());
                if !current(shared, id) {
                    return Ok(());
                }
                queue.committing = true;
            }
            progress("Writing result".into(), 0, 0);
            std::mem::swap(&mut working, &mut session.document.frame_mut().pixels);
            let write_start = Instant::now();
            let result = session.document.write(&path);
            // Always restore the immutable source, including on write failure.
            std::mem::swap(&mut working, &mut session.document.frame_mut().pixels);
            result?;
            send(Event::Exported {
                id,
                path,
                milliseconds: start.elapsed().as_secs_f64() * 1000.0,
                processing_ms,
                write_ms: write_start.elapsed().as_secs_f64() * 1000.0,
                tile_buffer_bytes: report.tile_buffer_bytes,
            });
        }
    }
    Ok(())
}
fn stage(s: NativeStage) -> &'static str {
    match s {
        NativeStage::Analyze => "Analyzing skin",
        NativeStage::PrepareCache => "Preparing filter",
        NativeStage::Filter => "Smoothing skin",
        NativeStage::Tone => "Adjusting skin tone",
        NativeStage::Mask => "Showing skin mask",
        NativeStage::Mix => "Mixing preview",
        NativeStage::Complete => "Preparing result",
    }
}
fn display_pixels(pixels: &[f32], width: usize, height: usize, channels: usize) -> PreviewPixels {
    let mut rgba = Vec::with_capacity(width * height * 4);
    for pixel in pixels.chunks_exact(channels) {
        for &sample in &pixel[..3] {
            rgba.push(if sample.is_finite() {
                (sample.clamp(0.0, 1.0) * 255.0).round() as u8
            } else {
                0
            });
        }
        rgba.push(if channels == 4 {
            (pixel[3].clamp(0.0, 1.0) * 255.0).round() as u8
        } else {
            255
        });
    }
    PreviewPixels {
        width,
        height,
        rgba,
    }
}
fn proxy(frame: &Frame) -> (Vec<f32>, usize, usize, f64) {
    let scale = (1024.0 / frame.width.max(frame.height) as f64).min(1.0);
    let width = (frame.width as f64 * scale).round().max(1.0) as usize;
    let height = (frame.height as f64 * scale).round().max(1.0) as usize;
    if scale == 1.0 {
        return (frame.pixels.clone(), width, height, scale);
    }
    // Area averaging with alpha-weighted RGB avoids fringes from hidden pixels.
    let channels = frame.channels;
    let mut pixels = vec![0.0; width * height * channels];
    for y in 0..height {
        for x in 0..width {
            let (left, right) = (
                x as f64 * frame.width as f64 / width as f64,
                (x + 1) as f64 * frame.width as f64 / width as f64,
            );
            let (top, bottom) = (
                y as f64 * frame.height as f64 / height as f64,
                (y + 1) as f64 * frame.height as f64 / height as f64,
            );
            let mut sum = [0.0f64; 3];
            let mut weight = 0.0;
            let mut area = 0.0;
            for sy in top.floor() as usize..bottom.ceil() as usize {
                for sx in left.floor() as usize..right.ceil() as usize {
                    if sx >= frame.width as usize || sy >= frame.height as usize {
                        continue;
                    }
                    let coverage = (right.min(sx as f64 + 1.0) - left.max(sx as f64))
                        * (bottom.min(sy as f64 + 1.0) - top.max(sy as f64));
                    let p =
                        &frame.pixels[(sy * frame.width as usize + sx) * channels..][..channels];
                    area += coverage;
                    if !p[..3].iter().all(|s| s.is_finite()) {
                        continue;
                    }
                    let alpha = if channels == 4 && p[3].is_finite() {
                        p[3].clamp(0.0, 1.0) as f64
                    } else if channels == 4 {
                        0.0
                    } else {
                        1.0
                    };
                    let w = alpha * coverage;
                    weight += w;
                    for c in 0..3 {
                        sum[c] += p[c] as f64 * w;
                    }
                }
            }
            let at = (y * width + x) * channels;
            if weight > 0.0 {
                for c in 0..3 {
                    pixels[at + c] = (sum[c] / weight) as f32;
                }
            }
            if channels == 4 {
                pixels[at + 3] = (weight / area) as f32;
            }
        }
    }
    (pixels, width, height, scale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{fixture, pcraft::PcraftDocument};
    use serde_json::json;
    fn wait(worker: &Worker, id: u64) -> Event {
        loop {
            let event = worker
                .events
                .recv_timeout(std::time::Duration::from_secs(10))
                .unwrap();
            if event.id() == id && !matches!(event, Event::Progress { .. }) {
                return event;
            }
        }
    }
    #[test]
    fn worker_replaces_pending_previews_and_exports_from_immutable_source() {
        let directory =
            std::env::temp_dir().join(format!("natural-skin-worker-{}", std::process::id()));
        fs_cleanup(&directory);
        std::fs::create_dir(&directory).unwrap();
        let input = directory.join("input.pcraft");
        let frame = fixture(31, 19, 16, None);
        PcraftDocument::from_frame(frame.clone(), "Worker")
            .unwrap()
            .write(&input)
            .unwrap();
        let worker = Worker::new(|| {});
        let id = worker.submit(Job::Load(input.clone())).unwrap();
        assert!(matches!(wait(&worker, id), Event::Loaded { .. }));
        for strength in [20, 70, 90] {
            worker.submit(Job::Preview(json!({"Strength":strength})));
        }
        let id = worker.submit(Job::Preview(json!({"Strength":0}))).unwrap();
        match wait(&worker, id) {
            Event::Rendered { pixels, .. } => {
                assert_eq!(pixels.rgba, display_pixels(&frame.pixels, 31, 19, 4).rgba)
            }
            _ => panic!("Expected final preview"),
        }
        let output = directory.join("retouched.pcraft");
        let id = worker
            .submit(Job::Export(output.clone(), json!({})))
            .unwrap();
        assert!(matches!(wait(&worker, id), Event::Exported { .. }));
        assert_ne!(
            PcraftDocument::read(&output).unwrap().frame.pixels,
            frame.pixels
        );
        let id = worker
            .submit(Job::Export(output.clone(), json!({})))
            .unwrap();
        assert!(matches!(wait(&worker, id), Event::Failed { .. }));
        let identity = directory.join("identity.pcraft");
        let id = worker
            .submit(Job::Export(identity.clone(), json!({"Strength":0})))
            .unwrap();
        assert!(matches!(wait(&worker, id), Event::Exported { .. }));
        assert_eq!(
            PcraftDocument::read(&identity).unwrap().frame.pixels,
            frame.pixels
        );
        assert_eq!(
            PcraftDocument::read(&input).unwrap().frame.pixels,
            frame.pixels
        );
        drop(worker);
        fs_cleanup(&directory);
    }
    fn fs_cleanup(path: &std::path::Path) {
        let _ = std::fs::remove_dir_all(path);
    }
    #[test]
    fn cancellation_during_export_never_publishes_or_changes_source() {
        let dir = std::env::temp_dir().join(format!("natural-skin-cancel-{}", std::process::id()));
        fs_cleanup(&dir);
        std::fs::create_dir(&dir).unwrap();
        let input = dir.join("source.pcraft");
        let output = dir.join("cancelled.pcraft");
        let source = fixture(31, 19, 16, None);
        PcraftDocument::from_frame(source.clone(), "Cancel")
            .unwrap()
            .write(&input)
            .unwrap();
        let worker = Worker::new(|| {});
        worker.shared.generation.store(1, Ordering::Release);
        let mut session = None;
        run(&worker.shared, 1, Job::Load(input), &mut session, &|_| {}).unwrap();
        let result = run(
            &worker.shared,
            1,
            Job::Export(output.clone(), json!({})),
            &mut session,
            &|event| {
                if matches!(event,Event::Progress{ref stage,..} if stage=="Smoothing skin") {
                    assert!(worker.cancel());
                }
                assert!(!matches!(event, Event::Exported { .. }));
            },
        );
        assert!(result.is_err());
        assert!(!output.exists());
        assert_eq!(session.unwrap().document.frame().pixels, source.pixels);
        drop(worker);
        fs_cleanup(&dir);
    }

    #[test]
    fn proxy_averages_alpha_without_hidden_colour_fringes() {
        let mut frame = fixture(2048, 2, 32, None);
        for (i, p) in frame.pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            p.copy_from_slice(if i % 2 == 0 {
                &[0.6, 0.4, 0.3, 1.0]
            } else {
                &[0.0, 0.0, 1.0, 0.0]
            });
        }
        let (small, w, h, scale) = proxy(&frame);
        assert_eq!((w, h), (1024, 1));
        assert_eq!(scale, 0.5);
        for p in small.as_chunks::<4>().0 {
            assert_eq!(p, &[0.6, 0.4, 0.3, 0.5]);
        }
    }
}
