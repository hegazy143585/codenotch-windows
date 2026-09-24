//! Auto-update (W-10) through `tauri-plugin-updater`: the app reads `latest.json` from the newest
//! GitHub release of hegazy143585/codenotch-windows and accepts an installer only if its minisign
//! signature matches the public key in `tauri.conf.json`. The private key never ships; CI signs
//! with the `TAURI_SIGNING_PRIVATE_KEY` secret (see RELEASE.md).
//!
//! Nothing is installed without the user: a found update is offered in the tray and the settings
//! window, and only a click downloads and runs it (NSIS, passive, per user). The check can be
//! switched off in settings. A failed check is logged, never shown as an error on the notch.

use crate::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

const FIRST_CHECK_SECS: u64 = 60;
pub const CHECK_EVERY_SECS: u64 = 6 * 60 * 60;

static PENDING: Mutex<Option<Update>> = Mutex::new(None);
static CHECK_NOW: AtomicBool = AtomicBool::new(false);
/// Last result in words, for the settings window ("Up to date", "Checking failed: …")
static LAST: Mutex<String> = Mutex::new(String::new());

pub fn pending_version() -> Option<String> {
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|u| u.version.clone())
}

pub fn last_result() -> String {
    LAST.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn request_check() {
    CHECK_NOW.store(true, Ordering::Relaxed);
}

fn enabled(app: &AppHandle) -> bool {
    app.state::<AppState>().cfg.lock().unwrap_or_else(|e| e.into_inner()).update_check
}

fn check(app: &AppHandle) {
    let r = tauri::async_runtime::block_on(async {
        let updater = app.updater().map_err(|e| e.to_string())?;
        updater.check().await.map_err(|e| e.to_string())
    });
    let words = match r {
        Ok(Some(u)) => {
            let v = u.version.clone();
            crate::applog(&format!("update: {v} available (running {})", env!("CARGO_PKG_VERSION")));
            let new = pending_version().as_deref() != Some(v.as_str());
            *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = Some(u);
            if new {
                let _ = app.emit("notice", format!("Codenotch {v} is available — tray → Install update"));
            }
            format!("Version {v} is available")
        }
        Ok(None) => {
            *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = None;
            "Up to date".to_string()
        }
        Err(e) => {
            crate::applog(&format!("update check failed: {e}"));
            format!("Could not check: {e}")
        }
    };
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = words;
    crate::tray::refresh_menu(app);
    let _ = app.emit_to(crate::settings::LABEL, "settings", ());
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let mut wait = FIRST_CHECK_SECS;
        loop {
            for _ in 0..wait {
                if CHECK_NOW.swap(false, Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
            wait = CHECK_EVERY_SECS;
            if enabled(&app) {
                check(&app);
            }
        }
    });
}

/// Tray / settings: download the pending update, verify it, and run its installer. On Windows the
/// installer closes this process itself.
pub fn install(app: &AppHandle) {
    let Some(u) = PENDING.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
        let _ = app.emit("notice", "No update is waiting — checking now");
        request_check();
        return;
    };
    let a = app.clone();
    std::thread::spawn(move || {
        let _ = a.emit("notice", format!("Downloading Codenotch {}…", u.version));
        let r = tauri::async_runtime::block_on(u.download_and_install(|_, _| {}, || {}));
        if let Err(e) = r {
            crate::applog(&format!("update install failed: {e}"));
            let _ = a.emit("notice", format!("Update failed: {e}"));
        }
    });
}

#[cfg(test)]
mod tests {
    /// The shipped public key must be a real minisign key, and updates must come over HTTPS from
    /// the owner's repository — a typo here would make every update fail verification.
    #[test]
    fn the_updater_config_carries_a_minisign_key_and_the_owners_release_feed() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let up = &conf["plugins"]["updater"];
        let key = up["pubkey"].as_str().expect("pubkey");
        let decoded = crate::antigravity::b64_decode(key).expect("base64");
        let text = String::from_utf8(decoded).unwrap();
        assert!(text.starts_with("untrusted comment: minisign public key"), "{text}");
        let endpoints = up["endpoints"].as_array().unwrap();
        assert!(!endpoints.is_empty());
        for e in endpoints {
            assert!(e.as_str().unwrap().starts_with("https://github.com/hegazy143585/codenotch-windows/releases/"));
        }
    }

    /// The 0.4.0 release shipped latest.json with version ".4.0" because the workflow sliced the
    /// tag one character too far. The manifest version must come from tauri.conf.json, checked
    /// against the tag, so the updater always gets a valid semver.
    #[test]
    fn the_release_manifest_takes_its_version_from_the_app_config() {
        let wf = include_str!("../../../.github/workflows/windows.yml");
        assert!(!wf.contains(".Substring("), "the version must not be cut out of the tag name");
        assert!(wf.contains("codenotch/tauri.conf.json"), "read the version from the app config");
        assert!(wf.contains("-ne \"win-v$version\""), "a tag that does not match the version must fail the release");
    }

    #[test]
    fn checks_are_spaced_hours_apart() {
        assert!(super::CHECK_EVERY_SECS >= 3600);
    }
}
