//! Experimental file handoff: a PhotoCraft action writes request.pcraft; a
//! resident companion snapshots and validates it. No editor control port needed.
use crate::{Result, invalid, pcraft::PcraftDocument};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TrySendError},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const MAX_BYTES: u64 = 768 << 20;
const SETTLE: Duration = Duration::from_millis(200);
#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp {
    length: u64,
    modified: SystemTime,
}
fn stamp(path: &Path) -> Result<Stamp> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.file_type().is_file() || meta.len() > MAX_BYTES {
        return Err(invalid("Capture must be a regular pcraft file within 768 MiB").into());
    }
    Ok(Stamp {
        length: meta.len(),
        modified: meta.modified()?,
    })
}
fn unique() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
pub enum HandoffEvent {
    Receiving,
    Captured(PathBuf),
    Rejected(String),
}
pub struct Inbox {
    root: PathBuf,
    session: PathBuf,
    seen: Option<Stamp>,
    pending: Option<(Stamp, Instant)>,
    sequence: u64,
    _lock: File,
}
impl Inbox {
    pub fn new(root: &Path) -> Result<Self> {
        fs::create_dir_all(root)?;
        let root = root.canonicalize()?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(root.join(".natural-skin.lock"))?;
        lock.try_lock().map_err(|_| {
            invalid("Another Natural Skin listener already owns this exchange folder")
        })?;
        let session = root.join(format!("session-{}-{}", std::process::id(), unique()));
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = fs::DirBuilder::new();
            builder.mode(0o700);
            builder
        };
        #[cfg(not(unix))]
        let builder = fs::DirBuilder::new();
        builder.create(&session)?;
        let seen = match stamp(&root.join("request.pcraft")) {
            Ok(s) => Some(s),
            Err(e)
                if e.downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
            {
                None
            }
            Err(e) => return Err(e),
        };
        let result = Self {
            root,
            session,
            seen,
            pending: None,
            sequence: 0,
            _lock: lock,
        };
        result.status("listening", None)?;
        Ok(result)
    }
    fn status(&self, phase: &str, path: Option<&Path>) -> Result<()> {
        let value = serde_json::json!({"phase":phase,"pid":std::process::id(),"snapshot":path,"session":self.session});
        // This trace belongs only to this private listener session, not the input.
        fs::write(
            self.session.join("status.json"),
            serde_json::to_vec(&value)?,
        )?;
        Ok(())
    }
    pub fn poll(&mut self) -> Result<Option<PathBuf>> {
        self.poll_notifying(|| {})
    }
    fn poll_notifying(&mut self, notify: impl FnOnce()) -> Result<Option<PathBuf>> {
        let input = self.root.join("request.pcraft");
        let now = match stamp(&input) {
            Ok(s) => s,
            Err(e)
                if e.downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
            {
                self.pending = None;
                return Ok(None);
            }
            Err(e) => return Err(e),
        };
        if Some(now) == self.seen {
            return Ok(None);
        }
        match self.pending {
            Some((s, time)) if s == now => {
                if time.elapsed() < SETTLE {
                    return Ok(None);
                }
            }
            _ => {
                self.pending = Some((now, Instant::now()));
                return Ok(None);
            }
        }
        notify();
        self.sequence += 1;
        let snapshot = self
            .session
            .join(format!("capture-{}.pcraft", self.sequence));
        let result = (|| -> Result<bool> {
            let mut from = File::open(&input)?.take(MAX_BYTES + 1);
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut out = options.open(&snapshot)?;
            let size = std::io::copy(&mut from, &mut out)?;
            out.flush()?;
            if size > MAX_BYTES {
                return Err(invalid("Capture grew beyond its budget").into());
            }
            if stamp(&input)? != now {
                return Ok(false);
            }
            // Work only on a private immutable snapshot, never the editor's file.
            let _document = PcraftDocument::read(&snapshot)?;
            Ok(stamp(&input)? == now)
        })();
        if !matches!(result, Ok(true)) {
            let _ = fs::remove_file(&snapshot);
        }
        match result {
            Ok(true) => {
                self.seen = Some(now);
                self.pending = None;
                self.status("captured", Some(&snapshot))?;
                Ok(Some(snapshot))
            }
            Ok(false) => {
                self.pending = None;
                Ok(None)
            }
            Err(e) => {
                self.seen = Some(now);
                self.pending = None;
                let _ = self.status("rejected", None);
                Err(e)
            }
        }
    }
}
pub struct Watcher {
    pub events: Receiver<HandoffEvent>,
    stop: Arc<AtomicBool>,
}
impl Watcher {
    pub fn new(root: &Path, repaint: impl Fn() + Send + 'static) -> Result<Self> {
        let mut inbox = Inbox::new(root)?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let (sender, events) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut last_error = String::new();
            let mut pending = None;
            while !flag.load(Ordering::Acquire) {
                let event = match inbox.poll_notifying(|| {
                    if sender.try_send(HandoffEvent::Receiving).is_ok() {
                        repaint();
                    }
                }) {
                    Ok(Some(path)) => {
                        last_error.clear();
                        Some(HandoffEvent::Captured(path))
                    }
                    Ok(None) => None,
                    Err(e) => {
                        let msg = e.to_string();
                        if msg == last_error {
                            None
                        } else {
                            last_error = msg.clone();
                            Some(HandoffEvent::Rejected(msg))
                        }
                    }
                };
                if let Some(event) = event {
                    // At most one delivered and one pending capture. A busy UI
                    // must not accumulate an unbounded queue of image paths.
                    if let Some(HandoffEvent::Captured(path)) = pending.replace(event) {
                        let _ = fs::remove_file(path);
                    }
                }
                if let Some(event) = pending.take() {
                    match sender.try_send(event) {
                        Ok(()) => repaint(),
                        Err(TrySendError::Full(event)) => pending = Some(event),
                        Err(TrySendError::Disconnected(_)) => break,
                    }
                }
                thread::sleep(Duration::from_millis(80));
            }
        });
        Ok(Self { events, stop })
    }
}
impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

#[derive(Clone)]
pub struct HostTarget {
    path: PathBuf,
}
impl HostTarget {
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Explicit launch configuration only. Never derive an executable from image
    /// metadata or the watched request file.
    pub fn new(path: &Path) -> Result<Self> {
        let path = path.canonicalize()?;
        #[cfg(target_os = "macos")]
        if !path.is_dir()
            || !path
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("app"))
        {
            return Err(invalid("Choose a PhotoCraft .app bundle").into());
        }
        #[cfg(target_os = "windows")]
        if !path.is_file()
            || !path
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("exe"))
        {
            return Err(invalid("Choose PhotoCraft.exe").into());
        }
        Ok(Self { path })
    }
    pub fn open(&self, result: &Path) -> Result<()> {
        let result = result.canonicalize()?;
        if !result.is_file()
            || !result
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("pcraft"))
        {
            return Err(invalid("Return only a completed pcraft file").into());
        }
        #[cfg(target_os = "macos")]
        {
            let mut child = Command::new("/usr/bin/open")
                .arg("-a")
                .arg(&self.path)
                .arg(&result)
                .spawn()?;
            let start = Instant::now();
            loop {
                if let Some(status) = child.try_wait()? {
                    return if status.success() {
                        Ok(())
                    } else {
                        Err(invalid(format!("PhotoCraft open request failed: {status}")).into())
                    };
                }
                if start.elapsed() > Duration::from_secs(8) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(invalid(
                        "Open request timed out; result is saved, do not retry automatically",
                    )
                    .into());
                }
                thread::sleep(Duration::from_millis(25));
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            Command::new(&self.path).arg(result).spawn()?;
            Ok(())
        }
    }
}
/// Install via PhotoCraft File > Scripts > Browse. Its native JSON action store
/// retains pcraft paths that Photoshop's .atn export cannot represent.
pub fn install_action_script(root: &Path, name: &str) -> Result<serde_json::Value> {
    let root = root.canonicalize()?;
    if !root.is_dir() || name.trim().is_empty() {
        return Err(invalid("Choose an exchange folder and action name").into());
    }
    let steps = [
        (
            "layer.duplicate",
            serde_json::json!({"document":"new","documentName":"Natural Skin Exchange"}),
        ),
        (
            "file.saveACopy",
            serde_json::json!({"path":root.join("request.pcraft")}),
        ),
        ("file.close", serde_json::json!({"save":"discard"})),
    ];
    let mut script = vec![
        serde_json::json!(["actions.record",{"name":name,"set":"Natural Skin Native"}]),
        serde_json::json!(["actions.stop", {}]),
    ];
    for (id, params) in steps {
        script.push(serde_json::json!(["actions.insert",{"action":name,"id":id,"params":params}]));
    }
    script.push(serde_json::json!(["actions.setColor",{"action":name,"color":"green"}]));
    Ok(serde_json::json!(script))
}

pub fn result_path(input: &Path) -> Result<PathBuf> {
    Ok(input
        .parent()
        .ok_or("Missing capture directory")?
        .join(format!("natural-skin-result-{}.pcraft", unique())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;
    struct Dir(PathBuf);
    impl Dir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "natural-skin-inbox-{}-{}",
                std::process::id(),
                unique()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn capture(path: &Path) {
        PcraftDocument::from_frame(fixture(3, 2, 16, None), "Snapshot")
            .unwrap()
            .write(path)
            .unwrap();
    }
    #[test]
    fn inbox_ignores_old_input_and_accepts_new_immutable_snapshot_once() {
        let dir = Dir::new();
        let input = dir.0.join("request.pcraft");
        capture(&input);
        let mut inbox = Inbox::new(&dir.0).unwrap();
        assert!(inbox.poll().unwrap().is_none());
        fs::remove_file(&input).unwrap();
        capture(&input);
        assert!(inbox.poll().unwrap().is_none());
        inbox.pending.as_mut().unwrap().1 = Instant::now() - SETTLE;
        let snapshot = inbox.poll().unwrap().unwrap();
        let bytes = fs::read(&snapshot).unwrap();
        assert!(inbox.poll().unwrap().is_none());
        fs::write(&input, b"changed").unwrap();
        assert_eq!(fs::read(snapshot).unwrap(), bytes);
    }
    #[test]
    fn rejects_incomplete_capture_and_prevents_duplicate_listener() {
        let dir = Dir::new();
        let mut inbox = Inbox::new(&dir.0).unwrap();
        assert!(Inbox::new(&dir.0).is_err());
        fs::write(dir.0.join("request.pcraft"), b"partial ZIP").unwrap();
        assert!(inbox.poll().unwrap().is_none());
        inbox.pending.as_mut().unwrap().1 = Instant::now() - SETTLE;
        assert!(inbox.poll().is_err());
        assert!(!inbox.session.join("capture-1.pcraft").exists());
        assert!(inbox.poll().unwrap().is_none());
    }
    #[cfg(unix)]
    #[test]
    fn rejects_symlink_requests() {
        let dir = Dir::new();
        std::os::unix::fs::symlink("missing", dir.0.join("request.pcraft")).unwrap();
        assert!(Inbox::new(&dir.0).is_err());
    }
}
