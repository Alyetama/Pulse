//! "Launch at Login" implemented with a per-user LaunchAgent plist.
//!
//! We avoid the ObjC `SMAppService` bridge and instead write a standard
//! LaunchAgent to `~/Library/LaunchAgents`, then (un)load it with `launchctl`.
//! This is robust for a personal, ad-hoc-signed app and needs no entitlements.

use std::path::PathBuf;
use std::process::Command;

const LABEL: &str = "com.alyetama.pulse";

fn plist_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join("Library/LaunchAgents").join(format!("{LABEL}.plist")))
}

/// Path to the executable to relaunch. When running inside `Pulse.app` this is
/// `.../Pulse.app/Contents/MacOS/Pulse`, which is exactly what we want at login.
fn exe_path() -> Option<String> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(str::to_owned))
}

/// Reflects reality: is the LaunchAgent currently installed on disk?
pub fn is_enabled() -> bool {
    plist_path().map(|p| p.exists()).unwrap_or(false)
}

/// Enable or disable auto-launch. Returns Ok on success.
pub fn set_enabled(enable: bool) -> Result<(), String> {
    let path = plist_path().ok_or("no home directory")?;
    if enable {
        let exe = exe_path().ok_or("cannot resolve executable path")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
    <key>LimitLoadToSessionType</key>
    <string>Aqua</string>
</dict>
</plist>
"#
        );
        std::fs::write(&path, plist).map_err(|e| e.to_string())?;
        // Best-effort load so it's registered now (ignore failure if already loaded).
        let _ = Command::new("launchctl").arg("load").arg(&path).output();
        Ok(())
    } else {
        let _ = Command::new("launchctl").arg("unload").arg(&path).output();
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
