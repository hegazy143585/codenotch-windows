//! Claude usage adapter (official), implemented from the upstream Codenotch's documented behaviour.
//! Endpoint: GET https://api.anthropic.com/api/oauth/usage
//! Headers: Authorization: Bearer <token>; anthropic-beta: oauth-2025-04-20; 15 s timeout
//! Rules (upstream's discipline):
//!   - the credential comes from Claude Code's own store (Windows: ~/.claude/.credentials.json), read only
//!   - 401/403 → re-read the credential once and retry (Claude Code may have just refreshed the token) → still failing means needsAuth
//!   - 429 → back off 60 s × 2^n capped at 15 min, Retry-After only raises it; the deadline is persisted
//!   - never invent a percentage on failure: keep the last reading marked stale, and the UI shows how old it is
//! Reply (snake_case): { limits:[{kind,percent,resets_at}], five_hour:{utilization,resets_at}, seven_day:{...} }
//! limits is the forward-compatible main shape; five_hour/seven_day are merged in as a fallback (a window that just rolled over disappears from limits).

use crate::notes;
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

const ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";
/// While Claude is working the ring must move with the session, not five minutes behind it. On top
/// of the poll, every finished turn (hook `Stop`, watcher "done", or the activity row disappearing)
/// asks for an immediate reading — that is when the number actually changes.
const POLL_ACTIVE_SECS: u64 = 30;
const POLL_IDLE_SECS: u64 = 300;
/// Event-triggered refreshes can arrive in bursts (several quick turns); the API is asked at most
/// once per this gap regardless, and a burst collapses into one fetch after it.
const MIN_GAP_SECS: u64 = 20;
const BACKOFF_BASE_SECS: u64 = 60;
const BACKOFF_CAP_SECS: u64 = 900;

static REFRESH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Immediate refresh from the tray or a command
pub fn request_refresh() {
    REFRESH.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Sleep in slices so request_refresh can interrupt it
fn sleep_interruptible(total_secs: u64) {
    for _ in 0..total_secs {
        if REFRESH.swap(false, std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LimitWindow {
    pub id: String,
    pub label: String,
    /// 0.0–1.0 (fraction used)
    pub used: f64,
    /// Reset time, ms epoch (None = unknown)
    pub resets_at: Option<u64>,
    /// The reset time has passed since this reading was taken: the percentage belongs to a window that
    /// is over (set by providers::list, never by a provider)
    #[serde(default)]
    pub expired: bool,
    /// Pure count window (no published denominator, e.g. Antigravity's requests today) — the cell shows ~N and the ring draws only its track
    #[serde(default)]
    pub count: Option<i64>,
    /// What `count` counts when it is not requests ("tokens"); empty = requests
    #[serde(default)]
    pub unit: String,
    /// The number is ours, not the vendor's (upstream fidelity=.derived) — the card adds a ~ prefix
    #[serde(default)]
    pub derived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageSnapshot {
    /// ok | stale | needsAuth | backoff | error
    pub status: String,
    pub windows: Vec<LimitWindow>,
    pub fetched_at: u64,
    pub note: String,
    #[serde(default)]
    pub backoff_until: u64,
    /// Fingerprint (not the value) of the token the last 429 was for; a different credential on disk
    /// ends the backoff early. Persisted with the deadline so a restart cannot lose the pairing.
    #[serde(default)]
    pub limited_sig: String,
    /// Which path produced the current windows: "api" (the OAuth endpoint) or "desktop" (Claude
    /// Desktop's own 15-minute samples). The page uses it to judge staleness by the source's cadence.
    #[serde(default)]
    pub source: String,
    /// The last attempt failed to reach the network (DNS / connect / socket), as opposed to the
    /// service answering with an error. Lets the card say "offline" instead of "error".
    #[serde(default)]
    pub offline: bool,
    /// `note` as codes + arguments the card translates (W-23). Always set together with `note`
    /// through `set_note`; empty for readings persisted before the codes existed (the card then
    /// shows `note` as it is).
    #[serde(default)]
    pub note_parts: Vec<crate::notes::NotePart>,
}

impl UsageSnapshot {
    /// Sets the note from its parts; `note` becomes their English rendering
    pub fn set_note(&mut self, parts: Vec<crate::notes::NotePart>) {
        self.note = crate::notes::render(&parts);
        self.note_parts = parts;
    }

    pub fn clear_note(&mut self) {
        self.note.clear();
        self.note_parts.clear();
    }
}

/// A transport failure that means "no network", not "the service refused"
pub fn is_offline(e: &ureq::Error) -> bool {
    matches!(e, ureq::Error::Transport(t) if matches!(t.kind(), ureq::ErrorKind::Dns | ureq::ErrorKind::ConnectionFailed | ureq::ErrorKind::Io))
}

/// Whether the snapshot currently holds a Claude Desktop sample recent enough to show as live. The
/// API's failure branches consult this: the CLI token being refused must not dim numbers that
/// Desktop refreshed minutes ago.
pub fn desktop_live(u: &UsageSnapshot) -> bool {
    u.source == "desktop" && now_ms().saturating_sub(u.fetched_at) < crate::claude_desktop::FRESH_MS
}

/// FNV-1a of the token, hex. Enough to tell "same credential" from "Claude Code refreshed it"
/// without ever writing the secret itself into the snapshot or the store.
fn token_sig(token: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in token.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

fn store_path() -> std::path::PathBuf {
    crate::config::config_path().with_file_name("usage.json")
}

pub fn load_persisted() -> UsageSnapshot {
    std::fs::read_to_string(store_path())
        .ok()
        .and_then(|t| serde_json::from_str::<UsageSnapshot>(&t).ok())
        .map(|mut s| {
            if !s.windows.is_empty() {
                s.status = "stale".into(); // an old reading after a restart is labelled as such
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

/// Reads Claude Code's OAuth credential. Returns (token, expired hint).
fn read_credentials() -> Option<(String, bool)> {
    let home = dirs::home_dir()?;
    for name in [".credentials.json", "credentials.json"] {
        let p = home.join(".claude").join(name);
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let oauth = v.get("claudeAiOauth").unwrap_or(&v);
        if let Some(tok) = oauth.get("accessToken").and_then(|x| x.as_str()) {
            let expired = oauth
                .get("expiresAt")
                .and_then(|x| x.as_f64())
                .map(|ms| (ms as u64) <= now_ms())
                .unwrap_or(false);
            return Some((tok.to_string(), expired));
        }
    }
    None
}

/// The credential's `expiresAt` (ms epoch), for the token renewer's gate and after-check
pub fn credential_expiry() -> Option<u64> {
    let home = dirs::home_dir()?;
    for name in [".credentials.json", "credentials.json"] {
        let Ok(text) = std::fs::read_to_string(home.join(".claude").join(name)) else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
        let oauth = v.get("claudeAiOauth").unwrap_or(&v);
        if oauth.get("accessToken").and_then(|x| x.as_str()).is_some() {
            return oauth.get("expiresAt").and_then(|x| x.as_f64()).map(|ms| ms as u64);
        }
    }
    None
}

/// The organization Claude Code's credential belongs to, if it records one. Used to keep Claude
/// Desktop's samples off this ring when Desktop is signed into a different account.
pub fn credential_org() -> Option<String> {
    let home = dirs::home_dir()?;
    for name in [".credentials.json", "credentials.json"] {
        let Ok(text) = std::fs::read_to_string(home.join(".claude").join(name)) else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
        let oauth = v.get("claudeAiOauth").unwrap_or(&v);
        if let Some(org) = oauth.get("organizationUuid").and_then(|x| x.as_str()) {
            if !org.is_empty() {
                return Some(org.to_string());
            }
        }
    }
    None
}

/// For doctor: credential probe report (prints no secret values)
pub fn probe_credentials() -> String {
    match read_credentials() {
        Some((tok, expired)) => format!(
            "credential: found (token {} chars, {})",
            tok.len(),
            if expired { "expired — Claude Code refreshes it on its next use" } else { "valid" }
        ),
        None => "credential: ~/.claude/.credentials.json not found (needsAuth; the desktop app may use another store — signing in once with the Claude Code CLI creates it)".into(),
    }
}

fn parse_reset(v: &serde_json::Value) -> Option<u64> {
    v.as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.timestamp_millis().max(0) as u64)
}

fn label_for(kind: &str) -> String {
    match kind {
        "session" => "Current session".into(),
        "seven_day" | "weekly_all" => "Weekly (all models)".into(),
        "seven_day_opus" | "weekly_opus" => "Weekly (Opus)".into(),
        "weekly_scoped" => "Weekly (model-scoped)".into(),
        other => {
            // Forward compatibility: an unknown kind gets a readable label
            let mut s = other.replace('_', " ");
            if let Some(c) = s.get_mut(0..1) {
                c.make_ascii_uppercase();
            }
            s
        }
    }
}

fn parse_response(v: &serde_json::Value) -> Vec<LimitWindow> {
    let mut out: Vec<LimitWindow> = Vec::new();
    if let Some(arr) = v.get("limits").and_then(|x| x.as_array()) {
        for l in arr {
            let Some(kind) = l.get("kind").and_then(|x| x.as_str()) else {
                continue;
            };
            let Some(pct) = l.get("percent").and_then(|x| x.as_f64()) else {
                continue;
            };
            let resets = l.get("resets_at").and_then(parse_reset);
            if resets.is_none() {
                continue; // upstream rule: a window without a reset time is not shown
            }
            out.push(LimitWindow {
                id: kind.to_string(),
                label: label_for(kind),
                used: (pct / 100.0).clamp(0.0, 1.0),
                resets_at: resets, ..Default::default()
            });
        }
    }
    // Fallback merge: a window that just rolled over disappears from limits while the named field remains.
    // In practice the kinds in limits are weekly_all/weekly_scoped, not seven_day — deduplicating by id
    // alone would add the seven_day fallback a second time (the card showed "Weekly all" and
    // "Weekly (all models)" as twins). Three dedupe rules: id alias / same resets_at and percentage / same label.
    let aliases: [(&str, &str, &[&str]); 2] = [
        ("five_hour", "session", &["session", "five_hour"]),
        ("seven_day", "seven_day", &["seven_day", "weekly_all", "weekly"]),
    ];
    for (field, id, alias) in aliases {
        let Some(w) = v.get(field) else { continue };
        let Some(u) = w.get("utilization").and_then(|x| x.as_f64()) else { continue };
        let used = (u / 100.0).clamp(0.0, 1.0);
        let resets_at = w.get("resets_at").and_then(parse_reset);
        let label = label_for(id);
        let dup = out.iter().any(|x| {
            alias.contains(&x.id.as_str())
                || x.label == label
                || (resets_at.is_some()
                    && x.resets_at.map(|r| r / 1000) == resets_at.map(|r| r / 1000)
                    && (x.used - used).abs() < 0.005)
        });
        if dup {
            continue;
        }
        out.push(LimitWindow { id: id.into(), label, used, resets_at, ..Default::default() });
    }
    // session always comes first (upstream display order)
    out.sort_by_key(|w| if w.id == "session" { 0 } else { 1 });
    out
}

enum FetchErr {
    NeedsAuth,
    RateLimited(u64), // suggested wait in seconds (the Retry-After before the floor is applied)
    Offline(String),
    Other(String),
}

fn http_agent() -> ureq::Agent {
    match native_tls::TlsConnector::builder().build() {
        Ok(tls) => ureq::AgentBuilder::new()
            .tls_connector(Arc::new(tls))
            .timeout(Duration::from_secs(15))
            .build(),
        Err(_) => ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(15))
            .build(),
    }
}

fn fetch_once(token: &str) -> Result<Vec<LimitWindow>, FetchErr> {
    let resp = http_agent()
        .get(ENDPOINT)
        .set("Authorization", &format!("Bearer {token}"))
        .set("anthropic-beta", "oauth-2025-04-20")
        .call();
    match resp {
        Ok(r) => {
            let v: serde_json::Value = r
                .into_json()
                .map_err(|e| FetchErr::Other(format!("parse: {e}")))?;
            Ok(parse_response(&v))
        }
        Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _)) => {
            Err(FetchErr::NeedsAuth)
        }
        Err(ureq::Error::Status(429, r)) => {
            let ra = r
                .header("retry-after")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            Err(FetchErr::RateLimited(ra))
        }
        Err(ureq::Error::Status(code, _)) => Err(FetchErr::Other(format!("HTTP {code}"))),
        Err(e) if is_offline(&e) => Err(FetchErr::Offline(format!("{e}"))),
        Err(e) => Err(FetchErr::Other(format!("{e}"))),
    }
}

fn backoff_secs(consecutive: u32, retry_after_floor: u64) -> u64 {
    let exp = BACKOFF_BASE_SECS.saturating_mul(1u64 << consecutive.min(4));
    exp.clamp(BACKOFF_BASE_SECS, BACKOFF_CAP_SECS).max(retry_after_floor)
}

pub fn set_and_broadcast(app: &AppHandle, mutate: impl FnOnce(&mut UsageSnapshot)) {
    let st = app.state::<AppState>();
    let snap = {
        let mut u = st.usage.get("claude").lock().unwrap();
        mutate(&mut u);
        u.clone()
    };
    persist(&snap);
    crate::providers::publish(app);
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        // Broadcast the persisted old reading at startup (stale beats blank)
        {
            crate::providers::publish(&app);
        }
        let mut consecutive_429: u32 = 0;
        let mut last_attempt: u64 = 0;
        loop {
            // No requests inside the backoff window
            let (bu, limited_sig) = {
                let st = app.state::<AppState>();
                let u = st.usage.get("claude").lock().unwrap();
                (u.backoff_until, u.limited_sig.clone())
            };
            let now = now_ms();
            if bu > now {
                // The 429 belonged to a specific token (typically an expired one the API refuses with
                // a long Retry-After). Once Claude Code writes a new credential, waiting out the rest
                // of that window is pointless: leave it at once and fetch with the fresh token.
                if !limited_sig.is_empty() {
                    if let Some((t, _)) = read_credentials() {
                        if token_sig(&t) != limited_sig {
                            set_and_broadcast(&app, |u| {
                                u.backoff_until = 0;
                                u.limited_sig.clear();
                                u.set_note(vec![notes::c("nCredRefreshed")]);
                            });
                            continue;
                        }
                    }
                }
                sleep_interruptible(((bu - now) / 1000).clamp(1, 30));
                continue;
            }
            // Minimum gap between requests, whatever asked for them (poll, tray, or a finished turn)
            let since = now.saturating_sub(last_attempt) / 1000;
            if last_attempt > 0 && since < MIN_GAP_SECS {
                sleep_interruptible(MIN_GAP_SECS - since);
                continue;
            }
            last_attempt = now;
            match read_credentials() {
                None => set_and_broadcast(&app, |u| {
                    if !desktop_live(u) {
                        u.status = "needsAuth".into();
                        u.set_note(vec![notes::c("nClaudeNoCred")]);
                    }
                }),
                Some((token, expired)) => {
                    // On 401/403 re-read the credential and retry once (Claude Code may have just refreshed it)
                    let result = match fetch_once(&token) {
                        Err(FetchErr::NeedsAuth) => match read_credentials() {
                            Some((t2, _)) if t2 != token => fetch_once(&t2),
                            _ => Err(FetchErr::NeedsAuth),
                        },
                        other => other,
                    };
                    let auth_note = notes::c(if expired { "nClaudeExpired" } else { "nClaudeRejected" });
                    match result {
                        Ok(windows) => {
                            consecutive_429 = 0;
                            set_and_broadcast(&app, |u| {
                                u.status = "ok".into();
                                u.windows = windows;
                                u.fetched_at = now_ms();
                                u.source = "api".into();
                                u.clear_note();
                                u.offline = false;
                                u.backoff_until = 0;
                                u.limited_sig.clear();
                            });
                        }
                        Err(FetchErr::NeedsAuth) => set_and_broadcast(&app, |u| {
                            if !desktop_live(u) {
                                u.status = "needsAuth".into();
                                u.set_note(vec![auth_note]);
                            }
                        }),
                        Err(FetchErr::RateLimited(ra)) => {
                            consecutive_429 += 1;
                            let wait = backoff_secs(consecutive_429 - 1, ra);
                            let sig = token_sig(&token);
                            // An expired token gets a 429 with an hour-long Retry-After rather than a
                            // 401, so "rate limited" alone would send the user looking in the wrong place
                            let note = if expired { notes::c("nClaudeExpiredLimited") } else { notes::p("nRateLimited", &[&wait.to_string()]) };
                            set_and_broadcast(&app, |u| {
                                // A live Desktop sample is not made stale by the API refusing the CLI token
                                if !desktop_live(u) {
                                    if !u.windows.is_empty() {
                                        u.status = "stale".into();
                                    }
                                    u.set_note(vec![note]);
                                }
                                u.backoff_until = now_ms() + wait * 1000;
                                u.limited_sig = sig;
                            });
                        }
                        Err(FetchErr::Offline(msg)) => set_and_broadcast(&app, |u| {
                            if !desktop_live(u) {
                                u.status = if u.windows.is_empty() { "error" } else { "stale" }.into();
                                u.offline = true;
                                u.set_note(vec![notes::p("nOffline", &[&msg])]);
                            }
                        }),
                        Err(FetchErr::Other(msg)) => set_and_broadcast(&app, |u| {
                            u.offline = false;
                            if !desktop_live(u) {
                                if u.windows.is_empty() {
                                    u.status = "error".into();
                                } else {
                                    u.status = "stale".into();
                                }
                                u.set_note(vec![notes::text(msg)]);
                            }
                        }),
                    }
                }
            }
            // 60 s while a session is active, 300 s otherwise (upstream throttling discipline)
            let active = {
                let st = app.state::<AppState>();
                let sessions = {
                    let store = st.store.lock().unwrap();
                    !store.snapshot("en", "en", false).sessions.is_empty()
                };
                // Cloud sessions and pushed events never reach the store; they show up as activity rows
                let rows = st.activity.lock().unwrap().iter().any(|a| a.provider == "claude");
                sessions || rows
            };
            sleep_interruptible(if active {
                POLL_ACTIVE_SECS
            } else {
                POLL_IDLE_SECS
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> serde_json::Value {
        let text = match name {
            "current" => include_str!("../fixtures/claude_oauth_usage.json"),
            _ => include_str!("../fixtures/claude_oauth_usage_legacy.json"),
        };
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn limits_are_parsed_session_first_and_the_fallback_does_not_duplicate() {
        let w = parse_response(&fixture("current"));
        let ids: Vec<&str> = w.iter().map(|x| x.id.as_str()).collect();
        // no_reset has no reset time and is dropped; five_hour/seven_day duplicate session/weekly_all
        assert_eq!(ids, vec!["session", "weekly_all", "weekly_opus"]);
        assert_eq!(w[0].label, "Current session");
        assert_eq!(w[1].label, "Weekly (all models)");
        assert!((w[0].used - 0.42).abs() < 1e-9);
        assert!((w[1].used - 0.175).abs() < 1e-9);
        assert_eq!(w[0].resets_at, Some(1_790_193_600_000));
    }

    #[test]
    fn the_legacy_shape_alone_still_yields_windows_and_clamps() {
        let w = parse_response(&fixture("legacy"));
        assert_eq!(w.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), vec!["session", "seven_day"]);
        assert_eq!(w[0].used, 1.0, "130 % is clamped");
        assert!((w[1].used - 0.6).abs() < 1e-9);
    }

    #[test]
    fn an_empty_or_foreign_reply_yields_nothing() {
        assert!(parse_response(&serde_json::json!({})).is_empty());
        assert!(parse_response(&serde_json::json!({"limits": "nope"})).is_empty());
    }

    #[test]
    fn unknown_kinds_get_a_readable_label() {
        assert_eq!(label_for("monthly_extra"), "Monthly extra");
    }

    #[test]
    fn the_token_fingerprint_never_contains_the_token() {
        let sig = token_sig("sk-ant-oat01-secret");
        assert_eq!(sig.len(), 16);
        assert!(!sig.contains("secret"));
        assert_ne!(sig, token_sig("sk-ant-oat01-other"));
    }

    #[test]
    fn backoff_doubles_and_caps_and_respects_retry_after() {
        assert_eq!(backoff_secs(0, 0), 60);
        assert_eq!(backoff_secs(1, 0), 120);
        assert_eq!(backoff_secs(9, 0), 900);
        assert_eq!(backoff_secs(0, 3600), 3600);
    }
}
