//! Cursor usage adapter, implemented from the upstream Codenotch's documented behaviour.
//!
//! Data path (same trade-off as upstream: borrow the editor's own session):
//!   1. Credential: the editor keeps its sign-in in the global state database it inherited from
//!      VS Code, `%APPDATA%\Cursor\User\globalStorage\state.vscdb` (SQLite, table ItemTable(key,value)):
//!      `cursorAuth/accessToken` + `cursorAuth/stripeMembershipAuthId`, joined into the cookie
//!      `WorkosCursorSessionToken=<authId>::<token>`. Non-secret identity cache:
//!      `cursorAuth/cachedEmail`, `cursorAuth/stripeMembershipType` (only the plan is shown).
//!   2. Endpoint: `GET https://cursor.com/api/usage-summary` (Cookie + Accept: application/json, 15 s).
//!      Reply: { billingCycleEnd, membershipType, isUnlimited,
//!              individualUsage: { plan: { totalPercentUsed, apiPercentUsed, used, limit, breakdown },
//!                                 onDemand: { enabled, used, limit } } }
//!      Cursor meters a percentage of the allowance, not requests: the dashboard's
//!      "Included usage · N% used" is totalPercentUsed. On the free plan used/limit are always 0
//!      (the allowance arrives as breakdown.bonus), so reading used/limit would report 10 % as 0 %.
//!      0 is a reading, not a gap (upstream's lesson). "API usage" is listed separately when
//!      apiPercentUsed > 0; "On demand" when onDemand has a real limit.
//!
//! SQLite opening rule: `mode=ro` first (it sees the token the editor just rotated into the WAL),
//! then `immutable=1` (once the editor has exited and the -shm is gone, mode=ro fails to open; by
//! then the WAL has been checkpointed, so ignoring it costs nothing).
//! Read only, never written; token values never reach logs, events or the UI.

use crate::notes;
use crate::usage::{LimitWindow, UsageSnapshot};
use crate::AppState;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

const ENDPOINT: &str = "https://cursor.com/api/usage-summary";
const POLL_SECS: u64 = 300;
const POLL_ACTIVE_SECS: u64 = 60; // while a run is in flight: usage moves with every run

static REFRESH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn request_refresh() {
    REFRESH.store(true, std::sync::atomic::Ordering::Relaxed);
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Windows: %APPDATA%\Cursor\User\globalStorage\state.vscdb (macOS: ~/Library/Application Support/Cursor/...)
pub fn store_url() -> Option<PathBuf> {
    dirs::config_dir().map(|c| c.join("Cursor").join("User").join("globalStorage").join("state.vscdb"))
}

fn store_path() -> PathBuf {
    crate::config::config_path().with_file_name("cursor.json")
}

pub fn load_persisted() -> UsageSnapshot {
    std::fs::read_to_string(store_path())
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

fn persist(s: &UsageSnapshot) {
    if let Ok(t) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(store_path(), t);
    }
}

pub fn present() -> bool {
    store_url().map(|p| p.is_file()).unwrap_or(false)
}

// ---------------- SQLite, read only ----------------

/// mode=ro first, immutable=1 as the fallback (see the module doc)
fn open_ro(path: &std::path::Path) -> Option<rusqlite::Connection> {
    use rusqlite::OpenFlags;
    if !path.is_file() {
        return None;
    }
    if let Ok(c) = rusqlite::Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        // Actually verify that reads work (with the -shm missing, open can succeed and the first query fail)
        if c.prepare("SELECT 1 FROM ItemTable LIMIT 1").and_then(|mut s| s.query([]).map(|_| ())).is_ok() {
            return Some(c);
        }
    }
    // Only the URI form takes immutable=1; a Windows path becomes file:///C:/... with \ → /
    let mut uri = String::from("file:///");
    uri.push_str(&path.to_string_lossy().replace('\\', "/").trim_start_matches('/').replace('#', "%23").replace('?', "%3F"));
    uri.push_str("?immutable=1");
    rusqlite::Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()
}

fn item(conn: &rusqlite::Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM ItemTable WHERE key = ?1", [key], |r| r.get::<_, String>(0))
        .ok()
        .filter(|s| !s.is_empty())
}

struct Creds {
    cookie: String,
    plan: Option<String>,
}

/// Re-read every time: the editor rotates the token, and holding on to an old value signs us out
fn read_credentials() -> Option<Creds> {
    let path = store_url()?;
    let conn = open_ro(&path)?;
    let token = item(&conn, "cursorAuth/accessToken")?;
    let auth_id = item(&conn, "cursorAuth/stripeMembershipAuthId")?;
    let plan = item(&conn, "cursorAuth/stripeMembershipType");
    Some(Creds { cookie: format!("WorkosCursorSessionToken={auth_id}::{token}"), plan })
}

/// For doctor: contains no secret values
pub fn probe() -> String {
    let Some(p) = store_url() else { return "Cursor: cannot locate %APPDATA%".into() };
    if !p.is_file() {
        return format!("Cursor: {} not found (not installed, or not signed in)", p.display());
    }
    match read_credentials() {
        Some(c) => format!(
            "Cursor: session borrowed (cookie {} chars, plan={})",
            c.cookie.len(),
            c.plan.unwrap_or_else(|| "?".into())
        ),
        None => format!("Cursor: {} exists but cursorAuth/* could not be read (editor not signed in, or SQLite failed to open)", p.display()),
    }
}

// ---------------- Parsing ----------------

fn pct(v: Option<&serde_json::Value>) -> Option<f64> {
    v.and_then(|x| x.as_f64()).map(|p| (p / 100.0).clamp(0.0, 1.0))
}

fn parse_iso(v: Option<&serde_json::Value>) -> Option<u64> {
    v.and_then(|x| x.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.timestamp_millis().max(0) as u64)
}

/// usage-summary → (windows, note). When there are no windows the note says why (Unlimited / free plan without an allowance)
pub fn parse_summary(v: &serde_json::Value) -> (Vec<LimitWindow>, Vec<notes::NotePart>) {
    let resets_at = parse_iso(v.get("billingCycleEnd"));
    let usage = v.get("individualUsage").cloned().unwrap_or(serde_json::Value::Null);
    let plan = usage.get("plan").cloned().unwrap_or(serde_json::Value::Null);
    let mut out = Vec::new();
    // Headline = the dashboard number; 0 is a reading too
    if let Some(total) = pct(plan.get("totalPercentUsed")) {
        out.push(LimitWindow { id: "included".into(), label: "Included usage".into(), used: total, resets_at, ..Default::default() });
    }
    if let Some(api) = pct(plan.get("apiPercentUsed")) {
        if api > 0.0 {
            out.push(LimitWindow { id: "api".into(), label: "API usage".into(), used: api, resets_at, ..Default::default() });
        }
    }
    if let Some(od) = usage.get("onDemand") {
        let enabled = od.get("enabled").and_then(|x| x.as_bool()).unwrap_or(false);
        let limit = od.get("limit").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let used = od.get("used").and_then(|x| x.as_f64());
        if enabled && limit > 0.0 {
            if let Some(u) = used {
                out.push(LimitWindow {
                    id: "on_demand".into(),
                    label: "On demand".into(),
                    used: (u / limit).clamp(0.0, 1.0),
                    resets_at, ..Default::default()
                });
            }
        }
    }
    if !out.is_empty() {
        return (out, Vec::new());
    }
    let membership = v.get("membershipType").and_then(|x| x.as_str());
    let unlimited = v.get("isUnlimited").and_then(|x| x.as_bool()) == Some(true);
    let note = match (unlimited, membership) {
        (true, Some(m)) => notes::p("nCursorUnlimited", &[m]),
        (true, None) => notes::c("nCursorUnlimitedAny"),
        (false, Some(m)) => notes::p("nCursorNothing", &[m]),
        (false, None) => notes::c("nCursorNothingAny"),
    };
    (out, vec![note])
}

enum FetchErr {
    NeedsAuth,
    Offline(String),
    Other(String),
}

fn fetch_once(cookie: &str) -> Result<serde_json::Value, FetchErr> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(15)).build();
    match agent.get(ENDPOINT).set("Cookie", cookie).set("Accept", "application/json").call() {
        Ok(r) => r.into_json::<serde_json::Value>().map_err(|e| FetchErr::Other(format!("parse: {e}"))),
        Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _)) => Err(FetchErr::NeedsAuth),
        Err(ureq::Error::Status(code, _)) => Err(FetchErr::Other(format!("HTTP {code}"))),
        Err(e) if crate::usage::is_offline(&e) => Err(FetchErr::Offline(format!("{e}"))),
        Err(e) => Err(FetchErr::Other(format!("{e}"))),
    }
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn read_once(prev: &UsageSnapshot) -> UsageSnapshot {
    let mut snap = prev.clone();
    snap.offline = false;
    let Some(creds) = read_credentials() else {
        snap.status = "needsAuth".into();
        snap.set_note(vec![notes::c("nCursorSignIn")]);
        return snap;
    };
    match fetch_once(&creds.cookie) {
        Ok(v) => {
            let (windows, note) = parse_summary(&v);
            snap.fetched_at = now_ms();
            if windows.is_empty() {
                snap.status = "none".into();
                snap.windows.clear();
                snap.set_note(note);
            } else {
                snap.status = "ok".into();
                snap.windows = windows;
                snap.set_note(match (&creds.plan, v.get("membershipType").and_then(|x| x.as_str())) {
                    (_, Some(m)) => vec![notes::text(cap(m)), notes::p("nVia", &["Cursor"])],
                    (Some(p), None) => vec![notes::text(cap(p)), notes::p("nVia", &["Cursor"])],
                    _ => vec![],
                });
            }
        }
        Err(FetchErr::NeedsAuth) => {
            snap.status = "needsAuth".into();
            snap.set_note(vec![notes::c("nCursorRejected")]);
        }
        Err(FetchErr::Offline(msg)) => {
            snap.status = if snap.windows.is_empty() { "error" } else { "stale" }.into();
            snap.offline = true;
            snap.set_note(vec![notes::p("nOffline", &[&msg])]);
        }
        Err(FetchErr::Other(msg)) => {
            // Stale beats invented: keep the old reading, marked stale
            snap.status = if snap.windows.is_empty() { "error" } else { "stale" }.into();
            snap.set_note(vec![notes::text(msg)]);
        }
    }
    snap
}

fn broadcast(app: &AppHandle, snap: UsageSnapshot) {
    let st = app.state::<AppState>();
    *st.usage.get("cursor").lock().unwrap() = snap.clone();
    persist(&snap);
    crate::providers::publish(app);
}

fn sleep_interruptible(secs: u64) {
    for _ in 0..secs {
        if REFRESH.swap(false, std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        {
            crate::providers::publish(&app);
        }
        if !present() {
            broadcast(&app, UsageSnapshot { status: "absent".into(), ..Default::default() });
            loop {
                sleep_interruptible(600); // Cursor is not installed: look again every 10 minutes
                if present() {
                    break;
                }
            }
        }
        loop {
            let prev = {
                let st = app.state::<AppState>();
                let s = st.usage.get("cursor").lock().unwrap().clone();
                s
            };
            let snap = read_once(&prev);
            if snap.status == "error" || snap.status == "stale" {
                crate::applog(&format!("cursor: {}", snap.note));
            }
            broadcast(&app, snap);
            sleep_interruptible(if crate::activity::provider_active(&app, "cursor") { POLL_ACTIVE_SECS } else { POLL_SECS });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(text: &str) -> serde_json::Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn a_pro_summary_yields_included_api_and_on_demand() {
        let (w, note) = parse_summary(&fixture(include_str!("../fixtures/cursor_usage_summary_pro.json")));
        assert!(note.is_empty());
        assert_eq!(w.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), vec!["included", "api", "on_demand"]);
        assert!((w[0].used - 0.617).abs() < 1e-9);
        assert!((w[2].used - 0.25).abs() < 1e-9, "5 of 20 on-demand");
        assert!(w.iter().all(|x| x.resets_at == Some(1_790_812_800_000)));
    }

    #[test]
    fn an_unlimited_plan_has_no_windows_and_says_why() {
        let (w, note) = parse_summary(&fixture(include_str!("../fixtures/cursor_usage_summary_unlimited.json")));
        assert!(w.is_empty());
        assert_eq!(notes::render(&note), "Unlimited on the enterprise plan — nothing to meter");
        assert_eq!(note, vec![notes::p("nCursorUnlimited", &["enterprise"])]);
    }

    #[test]
    fn an_empty_reply_is_not_invented_into_a_reading() {
        let (w, note) = parse_summary(&serde_json::json!({}));
        assert!(w.is_empty());
        assert_eq!(notes::render(&note), "This plan has nothing for Cursor to meter yet", "no made-up plan name");

    }
}
