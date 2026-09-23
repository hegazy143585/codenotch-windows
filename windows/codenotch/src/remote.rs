//! Shared loop for providers that read usage over HTTP with a credential another tool holds
//! (W-07). Each adapter supplies only `fetch`; this module owns the rules every one of them must
//! follow the same way:
//!   - a 429 backs off 60 s × 2^n (cap 15 min), `Retry-After` only raises it, and the deadline is
//!     persisted so neither a tray refresh nor a restart can poll into the limit
//!   - a failure never invents a number: the last reading stays, marked stale (or offline)
//!   - no credential and no trace of the tool = "absent" (the cell stays hidden)
//!   - one provider's thread never touches another's slot

use crate::usage::{now_ms, LimitWindow, UsageSnapshot};
use crate::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager};

pub enum Fetch {
    Ok { windows: Vec<LimitWindow>, note: String },
    /// Signed in, but nothing is metered (e.g. an unlimited plan); `note` says why
    Nothing(String),
    NeedsAuth(String),
    /// Server-suggested wait in seconds (0 = none given)
    RateLimited(u64),
    Offline(String),
    Other(String),
    /// No credential and no sign the tool is installed
    Absent,
}

pub struct Spec {
    pub id: &'static str,
    /// Snapshot file next to config.json
    pub file: &'static str,
    pub poll_secs: u64,
    pub fetch: fn() -> Fetch,
    pub refresh: &'static AtomicBool,
}

const BACKOFF_BASE_SECS: u64 = 60;
const BACKOFF_CAP_SECS: u64 = 900;

pub fn backoff_secs(consecutive: u32, retry_after: u64) -> u64 {
    BACKOFF_BASE_SECS.saturating_mul(1u64 << consecutive.min(4)).clamp(BACKOFF_BASE_SECS, BACKOFF_CAP_SECS).max(retry_after)
}

/// One fetch result applied to the previous snapshot. Pure: this is the whole policy, tested below.
/// `consecutive_429` is the count before this result.
pub fn apply(prev: &UsageSnapshot, r: Fetch, now: u64, consecutive_429: u32) -> UsageSnapshot {
    let mut s = prev.clone();
    s.offline = false;
    let keep_stale = |s: &mut UsageSnapshot, note: String| {
        s.status = if s.windows.is_empty() { "error" } else { "stale" }.into();
        s.note = note;
    };
    match r {
        Fetch::Ok { windows, note } => {
            s.status = "ok".into();
            s.windows = windows;
            s.note = note;
            s.fetched_at = now;
            s.backoff_until = 0;
        }
        Fetch::Nothing(note) => {
            s.status = "none".into();
            s.windows.clear();
            s.note = note;
            s.fetched_at = now;
            s.backoff_until = 0;
        }
        Fetch::NeedsAuth(note) => {
            s.status = "needsAuth".into();
            s.note = note;
        }
        Fetch::RateLimited(ra) => {
            let wait = backoff_secs(consecutive_429, ra);
            s.backoff_until = now + wait * 1000;
            keep_stale(&mut s, format!("Rate limited — retrying in {wait}s"));
        }
        Fetch::Offline(e) => {
            s.offline = true;
            keep_stale(&mut s, format!("Offline — {e}"));
        }
        Fetch::Other(e) => keep_stale(&mut s, e),
        Fetch::Absent => {
            return UsageSnapshot { status: "absent".into(), ..Default::default() };
        }
    }
    s
}

fn path(file: &str) -> std::path::PathBuf {
    crate::config::config_path().with_file_name(file)
}

/// The last reading from disk, labelled stale until a fresh one arrives (stale beats blank)
pub fn load_persisted(file: &str) -> UsageSnapshot {
    std::fs::read_to_string(path(file))
        .ok()
        .and_then(|t| serde_json::from_str::<UsageSnapshot>(&t).ok())
        .map(|mut s| {
            if !s.windows.is_empty() {
                s.status = "stale".into();
            }
            s
        })
        .unwrap_or_default()
}

fn persist(file: &str, s: &UsageSnapshot) {
    if let Ok(t) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(path(file), t);
    }
}

pub fn start(app: AppHandle, spec: &'static Spec) {
    std::thread::spawn(move || {
        crate::activity::lower_thread_priority();
        let mut consecutive_429: u32 = 0;
        loop {
            let prev = app.state::<AppState>().usage.get(spec.id).lock().unwrap_or_else(|e| e.into_inner()).clone();
            let now = now_ms();
            // Inside a backoff window nothing is sent, whatever asked for the refresh
            if prev.backoff_until > now {
                sleep(spec, ((prev.backoff_until - now) / 1000).max(1), false);
                continue;
            }
            let r = (spec.fetch)();
            let limited = matches!(r, Fetch::RateLimited(_));
            let next = apply(&prev, r, now_ms(), consecutive_429);
            consecutive_429 = if limited { consecutive_429 + 1 } else { 0 };
            if limited {
                crate::applog(&format!("{}: rate limited, next attempt in {}s", spec.id, (next.backoff_until.saturating_sub(now_ms())) / 1000));
            }
            let absent = next.status == "absent";
            *app.state::<AppState>().usage.get(spec.id).lock().unwrap_or_else(|e| e.into_inner()) = next.clone();
            if !absent {
                persist(spec.file, &next);
            }
            crate::providers::publish(&app);
            let active = crate::activity::provider_active(&app, spec.id);
            sleep(spec, if absent { 600 } else if active { spec.poll_secs.min(60) } else { spec.poll_secs }, true);
        }
    });
}

fn sleep(spec: &Spec, secs: u64, interruptible: bool) {
    for _ in 0..secs {
        if interruptible && spec.refresh.swap(false, Ordering::Relaxed) {
            return;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

// ---------------- HTTP ----------------

pub fn agent() -> ureq::Agent {
    let b = ureq::AgentBuilder::new().timeout(Duration::from_secs(15)).redirects(0);
    match native_tls::TlsConnector::builder().build() {
        Ok(tls) => b.tls_connector(std::sync::Arc::new(tls)).build(),
        Err(_) => b.build(),
    }
}

/// Maps a ureq result to the shared outcomes; `ok` turns a 2xx body into the provider's result
pub fn call(resp: Result<ureq::Response, ureq::Error>, auth_note: &str, ok: impl FnOnce(serde_json::Value) -> Fetch) -> Fetch {
    match resp {
        Ok(r) => match r.into_json::<serde_json::Value>() {
            Ok(v) => ok(v),
            Err(e) => Fetch::Other(format!("unreadable reply ({e})")),
        },
        Err(ureq::Error::Status(401 | 403, _)) => Fetch::NeedsAuth(auth_note.into()),
        Err(ureq::Error::Status(429, r)) => Fetch::RateLimited(r.header("retry-after").and_then(|s| s.trim().parse().ok()).unwrap_or(0)),
        Err(ureq::Error::Status(code, _)) => Fetch::Other(format!("HTTP {code}")),
        Err(e) if crate::usage::is_offline(&e) => Fetch::Offline(format!("{e}")),
        Err(e) => Fetch::Other(format!("{e}")),
    }
}

// ---------------- Windows Credential Manager (keys the user gives Codenotch itself) ----------------

/// A generic credential's secret as text (UTF-8, or UTF-16LE as other writers store it)
#[cfg(windows)]
pub fn cred_read(target: &str) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Security::Credentials::{CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC};
    let t: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
    let mut p: *mut CREDENTIALW = std::ptr::null_mut();
    unsafe {
        if CredReadW(PCWSTR(t.as_ptr()), CRED_TYPE_GENERIC, 0, &mut p).is_err() || p.is_null() {
            return None;
        }
        let c = &*p;
        let blob = if c.CredentialBlobSize > 0 && !c.CredentialBlob.is_null() {
            std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec()
        } else {
            Vec::new()
        };
        CredFree(p as *const core::ffi::c_void);
        decode_secret(&blob)
    }
}
#[cfg(not(windows))]
pub fn cred_read(_target: &str) -> Option<String> {
    None
}

/// Store a secret for this user (Credential Manager, local persistence). Never written to disk by us.
#[cfg(windows)]
pub fn cred_write(target: &str, secret: &str) -> Result<(), String> {
    use windows::core::PWSTR;
    use windows::Win32::Security::Credentials::{CredWriteW, CREDENTIALW, CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC};
    let mut t: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
    let mut user: Vec<u16> = "codenotch".encode_utf16().chain(std::iter::once(0)).collect();
    let mut blob = secret.as_bytes().to_vec();
    let c = CREDENTIALW {
        Flags: CRED_FLAGS(0),
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(t.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: PWSTR(user.as_mut_ptr()),
        ..Default::default()
    };
    unsafe { CredWriteW(&c, 0) }.map_err(|e| format!("could not store the key ({e})"))
}
#[cfg(not(windows))]
pub fn cred_write(_target: &str, _secret: &str) -> Result<(), String> {
    Err("not supported on this platform".into())
}

/// Remove a stored secret; a missing one is not an error
#[cfg(windows)]
pub fn cred_delete(target: &str) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC};
    let t: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
    match unsafe { CredDeleteW(PCWSTR(t.as_ptr()), CRED_TYPE_GENERIC, 0) } {
        Ok(()) => Ok(()),
        Err(_) if cred_read(target).is_none() => Ok(()),
        Err(e) => Err(format!("could not remove the key ({e})")),
    }
}
#[cfg(not(windows))]
pub fn cred_delete(_target: &str) -> Result<(), String> {
    Ok(())
}

pub(crate) fn decode_secret(blob: &[u8]) -> Option<String> {
    let text = match std::str::from_utf8(blob) {
        Ok(t) if !t.contains('\0') => t.to_string(),
        _ => String::from_utf16_lossy(&blob.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>()),
    };
    let t = text.trim_matches('\0').trim().to_string();
    (!t.is_empty()).then_some(t)
}

// ---------------- small readers shared by the adapters ----------------

pub fn home_json(parts: &[&str]) -> Option<serde_json::Value> {
    let mut p = dirs::home_dir()?;
    for part in parts {
        p = p.join(part);
    }
    read_json(&p)
}

pub fn read_json(p: &std::path::Path) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
}

/// A non-empty string: an empty key is a request that cannot succeed
pub fn s(v: Option<&serde_json::Value>) -> Option<String> {
    v.and_then(|x| x.as_str()).map(str::trim).filter(|x| !x.is_empty()).map(String::from)
}

pub fn iso_ms(v: Option<&serde_json::Value>) -> Option<u64> {
    v.and_then(|x| x.as_str()).and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map(|d| d.timestamp_millis().max(0) as u64)
}

pub fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(used: f64) -> LimitWindow {
        LimitWindow { id: "w".into(), label: "W".into(), used, ..Default::default() }
    }

    fn with_reading() -> UsageSnapshot {
        UsageSnapshot { status: "ok".into(), windows: vec![w(0.3)], fetched_at: 10, ..Default::default() }
    }

    #[test]
    fn a_reading_replaces_the_old_one_and_clears_backoff() {
        let prev = UsageSnapshot { backoff_until: 5, ..with_reading() };
        let s = apply(&prev, Fetch::Ok { windows: vec![w(0.6)], note: "Pro".into() }, 100, 2);
        assert_eq!((s.status.as_str(), s.windows[0].used, s.fetched_at, s.backoff_until), ("ok", 0.6, 100, 0));
    }

    #[test]
    fn failures_keep_the_last_reading_and_never_invent_one() {
        let s = apply(&with_reading(), Fetch::Other("HTTP 500".into()), 100, 0);
        assert_eq!((s.status.as_str(), s.windows[0].used, s.fetched_at), ("stale", 0.3, 10));
        let s = apply(&UsageSnapshot::default(), Fetch::Other("HTTP 500".into()), 100, 0);
        assert_eq!(s.status, "error");
        assert!(s.windows.is_empty());
    }

    #[test]
    fn offline_is_flagged_and_cleared_by_the_next_reading() {
        let s = apply(&with_reading(), Fetch::Offline("dns".into()), 100, 0);
        assert!(s.offline);
        assert_eq!(s.status, "stale");
        let s = apply(&s, Fetch::Ok { windows: vec![w(0.1)], note: String::new() }, 200, 0);
        assert!(!s.offline);
    }

    #[test]
    fn rate_limits_back_off_exponentially_with_retry_after_as_a_floor() {
        let s = apply(&with_reading(), Fetch::RateLimited(0), 1_000, 0);
        assert_eq!(s.backoff_until, 1_000 + 60_000);
        let s = apply(&with_reading(), Fetch::RateLimited(0), 1_000, 3);
        assert_eq!(s.backoff_until, 1_000 + 480_000);
        let s = apply(&with_reading(), Fetch::RateLimited(0), 1_000, 9);
        assert_eq!(s.backoff_until, 1_000 + 900_000, "capped at 15 min");
        let s = apply(&with_reading(), Fetch::RateLimited(3_600), 1_000, 0);
        assert_eq!(s.backoff_until, 1_000 + 3_600_000);
        assert_eq!(s.windows[0].used, 0.3);
    }

    #[test]
    fn needs_auth_keeps_windows_for_context_but_says_sign_in() {
        let s = apply(&with_reading(), Fetch::NeedsAuth("sign in".into()), 100, 0);
        assert_eq!(s.status, "needsAuth");
    }

    #[test]
    fn secrets_decode_from_utf8_or_utf16() {
        assert_eq!(decode_secret(b"key-1 ").as_deref(), Some("key-1"));
        let u16: Vec<u8> = "key-2".encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
        assert_eq!(decode_secret(&u16).as_deref(), Some("key-2"));
        assert_eq!(decode_secret(b""), None);
    }

    /// Real Credential Manager round trip under a throwaway target, removed at the end
    #[cfg(windows)]
    #[test]
    fn credential_manager_round_trip() {
        let t = format!("codenotch-test:{}", std::process::id());
        assert_eq!(cred_read(&t), None);
        cred_write(&t, "secret-1").unwrap();
        assert_eq!(cred_read(&t).as_deref(), Some("secret-1"));
        cred_write(&t, "secret-2").unwrap();
        assert_eq!(cred_read(&t).as_deref(), Some("secret-2"), "overwrite");
        cred_delete(&t).unwrap();
        assert_eq!(cred_read(&t), None);
        cred_delete(&t).unwrap(); // deleting a missing one is fine
    }

    #[test]
    fn absent_resets_everything() {
        let s = apply(&with_reading(), Fetch::Absent, 100, 0);
        assert_eq!(s.status, "absent");
        assert!(s.windows.is_empty());
    }
}
