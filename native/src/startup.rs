//! Optional per-user login startup. Called only by an explicit settings action.
use crate::Result;
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::process::Command;

pub struct LoginStartup {
    executable: PathBuf,
    data: PathBuf,
    #[cfg(target_os = "macos")]
    file: PathBuf,
}
impl LoginStartup {
    pub fn new(data: &Path) -> Result<Self> {
        Ok(Self {
            executable: std::env::current_exe()?,
            data: data.to_path_buf(),
            #[cfg(target_os = "macos")]
            file: PathBuf::from(std::env::var_os("HOME").ok_or("Missing home directory")?)
                .join("Library/LaunchAgents/io.github.wnanbei.natural-skin.plist"),
        })
    }
    pub fn enabled(&self) -> Result<bool> {
        #[cfg(target_os = "macos")]
        {
            Ok(self.file.try_exists()? && std::fs::read_to_string(&self.file)? == self.plist()?)
        }
        #[cfg(target_os = "windows")]
        {
            let output = command()
                .args(["query", KEY, "/v", "NaturalSkinCompanion"])
                .output()?;
            Ok(output.status.success()
                && String::from_utf8_lossy(&output.stdout).contains(&self.command_value()?))
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            Ok(false)
        }
    }
    pub fn set_enabled(&self, enabled: bool) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            if enabled {
                std::fs::create_dir_all(
                    self.file.parent().ok_or("Missing LaunchAgents directory")?,
                )?;
                let temp = self
                    .file
                    .with_extension(format!("{}.tmp", std::process::id()));
                std::fs::write(&temp, self.plist()?)?;
                std::fs::rename(temp, &self.file)?;
            } else if self.file.try_exists()? {
                std::fs::remove_file(&self.file)?;
            }
            Ok(())
        }
        #[cfg(target_os = "windows")]
        {
            let mut cmd = command();
            if enabled {
                cmd.args([
                    "add",
                    KEY,
                    "/v",
                    "NaturalSkinCompanion",
                    "/t",
                    "REG_SZ",
                    "/d",
                    &self.command_value()?,
                    "/f",
                ]);
            } else {
                // An absent value already means disabled.
                if !command()
                    .args(["query", KEY, "/v", "NaturalSkinCompanion"])
                    .output()?
                    .status
                    .success()
                {
                    return Ok(());
                }
                cmd.args(["delete", KEY, "/v", "NaturalSkinCompanion", "/f"]);
            }
            if !cmd.output()?.status.success() {
                return Err("Could not update per-user login startup".into());
            }
            Ok(())
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = enabled;
            Err("Login startup is supported on macOS and Windows".into())
        }
    }
    #[cfg(target_os = "windows")]
    fn command_value(&self) -> Result<String> {
        let exe = self
            .executable
            .to_str()
            .ok_or("Non-Unicode executable path")?;
        let data = self.data.to_str().ok_or("Non-Unicode data path")?;
        // Quotes cannot occur in a Windows filename. Backslashes before the
        // closing quote must be doubled for CommandLineToArgvW semantics.
        let quote = |s: &str| {
            format!(
                "\"{}{}\"",
                s,
                "\\".repeat(s.chars().rev().take_while(|c| *c == '\\').count())
            )
        };
        Ok(format!(
            "{} --background --data-dir {}",
            quote(exe),
            quote(data)
        ))
    }
    #[cfg(target_os = "macos")]
    fn plist(&self) -> Result<String> {
        Ok(format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>io.github.wnanbei.natural-skin</string><key>ProgramArguments</key><array><string>{}</string><string>--background</string><string>--data-dir</string><string>{}</string></array><key>RunAtLoad</key><true/></dict></plist>\n",
            xml(&self.executable)?,
            xml(&self.data)?
        ))
    }
}
#[cfg(target_os = "windows")]
const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(target_os = "windows")]
fn command() -> Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new("reg.exe");
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    cmd
}
#[cfg(target_os = "macos")]
fn xml(path: &Path) -> Result<String> {
    Ok(path
        .to_str()
        .ok_or("Non-Unicode startup path")?
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;"))
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    #[test]
    fn login_startup_round_trip_in_isolated_directory() {
        let dir = std::env::temp_dir().join(format!("natural-skin-login-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let startup = LoginStartup {
            executable: PathBuf::from("/Applications/A & B/Natural Skin"),
            data: PathBuf::from("/tmp/测试 & state"),
            file: dir.join("startup.plist"),
        };
        assert!(!startup.enabled().unwrap());
        startup.set_enabled(true).unwrap();
        assert!(startup.enabled().unwrap());
        assert!(
            std::fs::read_to_string(&startup.file)
                .unwrap()
                .contains("A &amp; B")
        );
        startup.set_enabled(false).unwrap();
        assert!(!startup.enabled().unwrap());
        std::fs::remove_dir(dir).unwrap();
    }
}
