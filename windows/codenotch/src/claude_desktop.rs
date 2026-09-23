//! Claude Desktop's own usage samples: the third Claude source, and on a machine where Claude is
//! used through the desktop app rather than the terminal the only one that answers.
//!
//! The API path needs the OAuth token Claude Code keeps in `~/.claude/.credentials.json`, which
//! only the standalone `claude` command ever renews — someone who works in Claude Desktop has a
//! current subscription, a Desktop window showing 8% of the session used, and a token that expired
//! a week ago (the API answers it with a 429 and an hour-long Retry-After, not a 401). Upstream's
//! answer on macOS is Desktop's Chromium HTTP cache; on Windows that cache holds no usage entry,
//! but Desktop keeps something better: it appends `{t, org, u:{fh, sd}}` — five-hour and seven-day
//! percentages, the same two numbers its own usage panel shows — to
//! `%APPDATA%\Claude\plan-usage-history.json` every 15 minutes while it runs. No token, no network,
//! no subprocess, no write of any kind: read the file when its mtime moves, take the newest sample.
//!
//! Arbitration is by timestamp, not by source: a sample only replaces the snapshot when it is newer
//! than what is there, and a successful API fetch (stamped "now") wins over any sample until the
//! next one lands. The API's failure branches leave a live sample alone (see `usage::desktop_live`),
//! so the ring is never dimmed to "stale" by the CLI token being refused while Desktop's numbers
//! are current.

use crate::usage::{self, LimitWindow};
use crate::AppState;
use serde::Serialize;
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

/// mtime check cadence; the file itself moves every 15 minutes
const POLL: Duration = Duration::from_secs(5);
/// Desktop samples every 15 min; a reading older than this is no longer shown as live
pub const FRESH_MS: u64 = 35 * 60_000;

pub fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|c| c.join("Claude").join("plan-usage-history.json"))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sample {
    /// ms epoch, Desktop's clock
    pub t: u64,
    pub org: String,
    /// percent, 0–100
    pub five_hour: f64,
    pub seven_day: f64,
}

/// Newest sample in the file. The array is appended in time order, but max(t) is taken anyway —
/// a Desktop that re-syncs history could reorder it, and a wrong "newest" would move the ring
/// backwards.
pub fn latest() -> Option<Sample> {
    let text = std::fs::read_to_string(path()?).ok()?;
    parse_latest(&text)
}

pub fn parse_latest(text: &str) -> Option<Sample> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let mut best: Option<Sample> = None;
    for s in v.get("samples")?.as_array()? {
        let (Some(t), Some(org), Some(u)) = (s.get("t").and_then(|x| x.as_u64()), s.get("org").and_then(|x| x.as_str()), s.get("u")) else {
            continue; // one malformed sample is not a reason to drop the whole file
        };
        let (Some(fh), Some(sd)) = (u.get("fh").and_then(|x| x.as_f64()), u.get("sd").and_then(|x| x.as_f64())) else {
            continue;
        };
        if best.as_ref().map(|b| t > b.t).unwrap_or(true) {
            best = Some(Sample { t, org: org.to_string(), five_hour: fh, seven_day: sd });
        }
    }
    best
}

/// The same two windows the API path produces (ids included), so a reading from either source
/// lands in the same place on the card. Reset times are not in the samples: None, honestly.
pub fn windows(s: &Sample) -> Vec<LimitWindow> {
    let w = |id: &str, label: &str, pct: f64| LimitWindow {
        id: id.into(),
        label: label.into(),
        used: (pct / 100.0).clamp(0.0, 1.0),
        resets_at: None,
        count: None,
        derived: false,
    };
    vec![w("session", "Current session", s.five_hour), w("weekly_all", "Weekly (all models)", s.seven_day)]
}

fn mtime_ms(p: &std::path::Path) -> u64 {
    std::fs::metadata(p)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// For doctor
pub fn probe() -> String {
    let Some(p) = path() else { return "Claude Desktop samples: no config dir".into() };
    if !p.is_file() {
        return format!("Claude Desktop samples: {} not found (Desktop not installed, or never signed in)", p.display());
    }
    match latest() {
        Some(s) => {
            let age = crate::usage::now_ms().saturating_sub(s.t) / 60_000;
            format!("Claude Desktop samples: {} | newest {} min ago: session {}%, weekly {}%, org {}", p.display(), age, s.five_hour, s.seven_day, s.org)
        }
        None => format!("Claude Desktop samples: {} exists but holds no readable sample", p.display()),
    }
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        crate::activity::lower_thread_priority();
        let mut last_mtime: u64 = 0;
        let mut logged_org = false;
        loop {
            let Some(p) = path() else {
                std::thread::sleep(Duration::from_secs(60));
                continue;
            };
            let mtime = mtime_ms(&p);
            if mtime != 0 && mtime != last_mtime {
                last_mtime = mtime;
                if let Some(s) = latest() {
                    // Account separation: Desktop is signed into exactly one account. If Claude Code's
                    // credential names a different organization, these are another account's numbers
                    // and must never reach this ring. No credential at all means Desktop is the only
                    // account there is.
                    let org_ok = !matches!(usage::credential_org(), Some(o) if o != s.org);
                    if !org_ok {
                        if !logged_org {
                            crate::applog("claude desktop samples: organization differs from Claude Code's credential; ignored");
                            logged_org = true;
                        }
                    } else {
                        let newer = {
                            let st = app.state::<AppState>();
                            let u = st.usage.get("claude").lock().unwrap();
                            u.fetched_at < s.t
                        };
                        if newer {
                            usage::set_and_broadcast(&app, |u| {
                                u.status = "ok".into();
                                u.windows = windows(&s);
                                u.fetched_at = s.t;
                                u.source = "desktop".into();
                                u.note = "Live via Claude Desktop (it samples every 15 min)".into();
                            });
                            crate::applog(&format!("claude desktop sample: session {}% weekly {}% at {}", s.five_hour, s.seven_day, s.t));
                        }
                    }
                }
            }
            std::thread::sleep(POLL);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"{"version":2,"samples":[
        {"t":1789647645991,"org":"87c39f79","u":{"fh":8,"sd":7}},
        {"t":1789651245886,"org":"87c39f79","u":{"fh":12,"sd":9}},
        {"t":1789650345599,"org":"87c39f79","u":{"fh":8,"sd":7}},
        {"t":"bad","org":"87c39f79","u":{"fh":99,"sd":99}}
    ]}"#;

    #[test]
    fn newest_sample_wins_regardless_of_order_and_bad_rows_are_skipped() {
        let s = parse_latest(FILE).expect("a sample");
        assert_eq!(s.t, 1789651245886);
        assert_eq!(s.five_hour, 12.0);
        assert_eq!(s.seven_day, 9.0);
        assert_eq!(s.org, "87c39f79");
    }

    #[test]
    fn windows_carry_the_api_ids_as_fractions() {
        let s = parse_latest(FILE).unwrap();
        let w = windows(&s);
        assert_eq!(w[0].id, "session");
        assert!((w[0].used - 0.12).abs() < 1e-9);
        assert_eq!(w[1].id, "weekly_all");
        assert!((w[1].used - 0.09).abs() < 1e-9);
        assert!(w[0].resets_at.is_none());
    }

    #[test]
    fn empty_or_malformed_files_yield_nothing() {
        assert!(parse_latest("").is_none());
        assert!(parse_latest(r#"{"version":2,"samples":[]}"#).is_none());
        assert!(parse_latest(r#"{"version":2}"#).is_none());
    }
}
