//! Per-user desktop state and file-based single-instance activation.
//! No TCP listener and no editor preferences are involved.
use crate::{Result, handoff::install_action_script, invalid};
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
const MAX_STATE: u64 = 64 << 10;
pub const ACTION_NAME: &str = "Natural Skin Send";

#[derive(Clone)]
pub struct Paths {
    pub root: PathBuf,
    pub exchange: PathBuf,
    pub installer: PathBuf,
}
impl Paths {
    pub fn default_root() -> Result<PathBuf> {
        #[cfg(target_os = "macos")]
        let root = PathBuf::from(std::env::var_os("HOME").ok_or("Missing home directory")?)
            .join("Library/Application Support/Natural Skin");
        #[cfg(target_os = "windows")]
        let root = PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("Missing LOCALAPPDATA")?)
            .join("Natural Skin");
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let root = PathBuf::from(std::env::var_os("XDG_DATA_HOME").unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                .join(".local/share")
                .into_os_string()
        }))
        .join("natural-skin");
        Ok(root)
    }
    pub fn new(root: &Path) -> Result<Self> {
        let existed = root.try_exists()?;
        fs::create_dir_all(root)?;
        #[cfg(unix)]
        if !existed {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
        }
        #[cfg(not(unix))]
        let _ = existed;
        let root = root.canonicalize()?;
        let exchange = root.join("exchange");
        fs::create_dir_all(&exchange)?;
        Ok(Self {
            installer: root.join("Install Natural Skin.json"),
            root,
            exchange,
        })
    }
    pub fn generate_installer(&self) -> Result<()> {
        // Only our generated file is replaced. Reimporting an already installed
        // action is intentionally rejected by PhotoCraft instead of editing it.
        atomic_json(
            &self.installer,
            &install_action_script(&self.exchange, ACTION_NAME)?,
        )
    }
    pub fn load(&self) -> Result<Preferences> {
        let path = self.root.join("settings.json");
        if !path.try_exists()? {
            return Ok(Preferences::default());
        }
        let value = read_json(&path)?;
        if value["version"] != 1 {
            return Err(invalid("Unsupported settings version; keep the file and use a compatible Natural Skin version").into());
        }
        Ok(Preferences {
            host: optional_path(&value, "host")?,
            listening: value["listening"]
                .as_bool()
                .ok_or("Invalid listening setting")?,
            last_result: optional_path(&value, "lastResult")?,
        })
    }
    pub fn save(&self, prefs: &Preferences) -> Result<()> {
        atomic_json(
            &self.root.join("settings.json"),
            &json!({
                "version":1,"host":prefs.host,"listening":prefs.listening,
                "lastResult":prefs.last_result,
            }),
        )
    }
}
fn optional_path(value: &Value, key: &str) -> Result<Option<PathBuf>> {
    match &value[key] {
        Value::Null => Ok(None),
        Value::String(s) if !s.is_empty() && Path::new(s).is_absolute() => Ok(Some(s.into())),
        _ => Err(invalid(format!("Invalid {key} path in settings")).into()),
    }
}
#[derive(Clone, Default)]
pub struct Preferences {
    pub host: Option<PathBuf>,
    pub listening: bool,
    pub last_result: Option<PathBuf>,
}
fn read_json(path: &Path) -> Result<Value> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(invalid("Desktop state must be a regular file").into());
    }
    let mut data = Vec::new();
    File::open(path)?
        .take(MAX_STATE + 1)
        .read_to_end(&mut data)?;
    if data.len() > MAX_STATE as usize {
        return Err(invalid("Desktop state exceeds 64 KiB").into());
    }
    Ok(serde_json::from_slice(&data)?)
}
fn temporary(path: &Path) -> PathBuf {
    path.with_extension(format!(
        "{}-{}.tmp",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
}
pub(crate) fn atomic_json(path: &Path, value: &Value) -> Result<()> {
    let temp = temporary(path);
    let result = (|| -> Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(&serde_json::to_vec_pretty(value)?)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub enum Start {
    Primary(Instance),
    Forwarded,
}
pub struct Activation {
    pub input: Option<PathBuf>,
}
pub struct Instance {
    root: PathBuf,
    _lock: File,
}
impl Instance {
    pub fn start(paths: &Paths, input: Option<&Path>, background: bool) -> Result<Start> {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(paths.root.join("instance.lock"))?;
        match lock.try_lock() {
            Ok(()) => {
                let _ = fs::remove_file(paths.root.join("activation.json"));
                Ok(Start::Primary(Self {
                    root: paths.root.clone(),
                    _lock: lock,
                }))
            }
            Err(std::fs::TryLockError::WouldBlock) => {
                if !background || input.is_some() {
                    let input = input.map(Path::canonicalize).transpose()?;
                    atomic_json(&paths.root.join("activation.json"), &json!({"input":input}))?;
                }
                Ok(Start::Forwarded)
            }
            Err(e) => Err(e.into()),
        }
    }
    pub fn poll(&self) -> Result<Option<Activation>> {
        let path = self.root.join("activation.json");
        let intake = temporary(&path);
        match fs::rename(&path, &intake) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
            Ok(()) => (),
        }
        let result = read_json(&intake).and_then(|v| {
            Ok(Activation {
                input: optional_path(&v, "input")?,
            })
        });
        let _ = fs::remove_file(intake);
        result.map(Some)
    }
}
pub fn discover_host() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    #[cfg(target_os = "macos")]
    {
        candidates.push(PathBuf::from("/Applications/PhotoCraft.app"));
        if let Some(home) = std::env::var_os("HOME") {
            candidates.push(PathBuf::from(home).join("Applications/PhotoCraft.app"));
        }
    }
    #[cfg(target_os = "windows")]
    for base in ["LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(base) {
            let root = PathBuf::from(root);
            candidates.push(root.join("PhotoCraft/PhotoCraft.exe"));
            candidates.push(root.join("Programs/PhotoCraft/PhotoCraft.exe"));
        }
    }
    candidates.into_iter().find(|p| p.exists())
}

pub fn show_folder(path: &Path) -> Result<()> {
    let path = path.canonicalize()?;
    if !path.is_dir() {
        return Err(invalid("Folder no longer exists").into());
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/open")
            .arg(path)
            .spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer.exe")
            .arg(path)
            .spawn()?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn paths() -> Paths {
        Paths::new(&std::env::temp_dir().join(format!(
            "natural-skin-settings-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )))
        .unwrap()
    }
    #[test]
    fn persists_settings_and_keeps_invalid_state_for_recovery() {
        let p = paths();
        assert!(!p.load().unwrap().listening);
        let prefs = Preferences {
            host: Some(p.root.join("PhotoCraft.app")),
            listening: true,
            last_result: Some(p.root.join("result.pcraft")),
        };
        p.save(&prefs).unwrap();
        p.save(&prefs).unwrap();
        assert_eq!(p.load().unwrap().host, prefs.host);
        p.generate_installer().unwrap();
        assert!(
            fs::read_to_string(&p.installer)
                .unwrap()
                .contains("file.saveACopy")
        );
        fs::write(p.root.join("settings.json"), b"{broken").unwrap();
        assert!(p.load().is_err());
        assert_eq!(fs::read(p.root.join("settings.json")).unwrap(), b"{broken");
        fs::remove_dir_all(p.root).unwrap();
    }
    #[test]
    fn second_instance_activates_owner_and_lock_recovers_after_exit() {
        let p = paths();
        let Start::Primary(owner) = Instance::start(&p, None, false).unwrap() else {
            panic!()
        };
        assert!(matches!(
            Instance::start(&p, None, true).unwrap(),
            Start::Forwarded
        ));
        assert!(owner.poll().unwrap().is_none());
        let input = p.root.join("image.pcraft");
        fs::write(&input, b"fixture").unwrap();
        assert!(matches!(
            Instance::start(&p, Some(&input), false).unwrap(),
            Start::Forwarded
        ));
        assert_eq!(
            owner.poll().unwrap().unwrap().input,
            Some(input.canonicalize().unwrap())
        );
        assert!(owner.poll().unwrap().is_none());
        drop(owner);
        assert!(matches!(
            Instance::start(&p, None, false).unwrap(),
            Start::Primary(_)
        ));
        fs::remove_dir_all(p.root).unwrap();
    }
}
