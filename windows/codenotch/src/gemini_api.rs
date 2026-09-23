//! Gemini API (W-07, ported from macOS `GeminiAPIProvider`): tokens spent against a bare
//! `GEMINI_API_KEY`, added up from the logs the calling tools keep for themselves.
//!
//! Google publishes no usage endpoint for an API key, so there is nothing to call and no key is
//! ever read. The three local sources, each read only if its tool left state on disk:
//!   - Gemini CLI: `~/.gemini/tmp/<project>/chats/*.jsonl`, one `gemini` record per call; the same id
//!     is written twice (start, then with `tokens`), so records are keyed by id, later line wins.
//!     `tokens.total` already includes input + output + thoughts + tool.
//!   - OpenCode: `~/.local/share/opencode/opencode.db`, assistant messages with `providerID = google`
//!     (not `google-vertex`, which bills a GCP project). `tokens.total`, else the five components.
//!   - Hermes: `~/.hermes/state.db`, `session_model_usage` rows with `billing_provider = gemini`,
//!     bucketed by `last_seen`; reasoning is already inside output, so it is not added again.
//! Months and days are local time. There is no limit, so the windows are counts, not percentages.

use crate::usage::{now_ms, LimitWindow, UsageSnapshot};
use crate::AppState;
use chrono::{Datelike, Local, TimeZone};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager};

pub const ID: &str = "gemini-api";
const POLL_SECS: u64 = 300;

static REFRESH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn request_refresh() {
    REFRESH.store(true, std::sync::atomic::Ordering::Relaxed);
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TokenUsage {
    pub month: i64,
    pub today: i64,
    pub calls: i64,
}

impl TokenUsage {
    fn add(self, o: TokenUsage) -> TokenUsage {
        TokenUsage { month: self.month + o.month, today: self.today + o.today, calls: self.calls + o.calls }
    }
}

/// (ms epoch of the call, tokens, calls)
type Entry = (u64, i64, i64);

fn local_day(ms: u64) -> Option<(i32, u32, u32)> {
    Local.timestamp_millis_opt(ms as i64).single().map(|d| (d.year(), d.month(), d.day()))
}

/// Local midnight on the first of `now`'s month, ms epoch
pub fn start_of_month(now: u64) -> u64 {
    let Some((y, m, _)) = local_day(now) else { return 0 };
    Local.with_ymd_and_hms(y, m, 1, 0, 0, 0).earliest().map(|d| d.timestamp_millis().max(0) as u64).unwrap_or(0)
}

fn start_of_next_month(now: u64) -> Option<u64> {
    let (y, m, _) = local_day(now)?;
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    Local.with_ymd_and_hms(ny, nm, 1, 0, 0, 0).earliest().map(|d| d.timestamp_millis().max(0) as u64)
}

fn start_of_tomorrow(now: u64) -> Option<u64> {
    let d = Local.timestamp_millis_opt(now as i64).single()?.date_naive().succ_opt()?;
    Local.from_local_datetime(&d.and_hms_opt(0, 0, 0)?).earliest().map(|d| d.timestamp_millis().max(0) as u64)
}

/// The one place a timestamp turns into a window. A call with no tokens was aborted before the
/// model answered; it cost nothing and is not a call.
pub fn bucket(entries: &[Entry], now: u64) -> TokenUsage {
    let Some((y, m, d)) = local_day(now) else { return TokenUsage::default() };
    let mut u = TokenUsage::default();
    for &(at, tokens, calls) in entries {
        if tokens <= 0 {
            continue;
        }
        let Some((ey, em, ed)) = local_day(at) else { continue };
        if (ey, em) != (y, m) {
            continue;
        }
        u.month += tokens;
        u.calls += calls;
        if ed == d {
            u.today += tokens;
        }
    }
    u
}

fn home() -> Option<PathBuf> {
    dirs::home_dir()
}

fn parse_ts(s: &str) -> Option<u64> {
    chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp_millis().max(0) as u64)
}

/// Calls in one Gemini CLI chat recording
pub fn cli_calls(text: &str) -> Vec<Entry> {
    let mut answered: HashMap<String, (u64, i64)> = HashMap::new();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if v.get("type").and_then(|x| x.as_str()) != Some("gemini") {
            continue;
        }
        let (Some(id), Some(tokens)) = (v.get("id").and_then(|x| x.as_str()), v.get("tokens")) else { continue };
        let Some(at) = v.get("timestamp").and_then(|x| x.as_str()).and_then(parse_ts) else { continue };
        let total = tokens.get("total").and_then(|x| x.as_i64()).unwrap_or(0);
        answered.insert(id.to_string(), (at, total)); // later line wins: it carries the reported usage
    }
    answered.into_values().map(|(at, t)| (at, t, 1)).collect()
}

/// None = Gemini CLI has no state on disk (a different answer from "it spent nothing")
pub fn read_cli(root: &Path, now: u64) -> Option<TokenUsage> {
    let projects = std::fs::read_dir(root).ok()?;
    let month_start = start_of_month(now);
    let mut entries = Vec::new();
    for project in projects.flatten() {
        let Ok(chats) = std::fs::read_dir(project.path().join("chats")) else { continue };
        for f in chats.flatten() {
            let p = f.path();
            if p.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            // Append-only: a file last written before this month cannot contribute
            let modified = f.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok());
            if matches!(modified, Some(d) if (d.as_millis() as u64) < month_start) {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&p) {
                entries.extend(cli_calls(&text));
            }
        }
    }
    Some(bucket(&entries, now))
}

fn open_ro(db: &Path) -> Option<rusqlite::Connection> {
    if !db.is_file() {
        return None;
    }
    rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX).ok()
}

pub fn read_opencode(db: &Path, now: u64) -> Option<TokenUsage> {
    let conn = open_ro(db)?;
    let sql = "SELECT time_created,
                      json_extract(data, '$.tokens.total'), json_extract(data, '$.tokens.input'),
                      json_extract(data, '$.tokens.output'), json_extract(data, '$.tokens.reasoning'),
                      json_extract(data, '$.tokens.cache.read'), json_extract(data, '$.tokens.cache.write')
               FROM message
               WHERE json_extract(data, '$.role') = 'assistant'
                 AND json_extract(data, '$.providerID') = 'google'
                 AND time_created >= ?1";
    let mut entries = Vec::new();
    // A database from an OpenCode version without this table still means OpenCode is installed
    if let Ok(mut stmt) = conn.prepare(sql) {
        let rows = stmt.query_map([start_of_month(now) as i64], |r| {
            let at: i64 = r.get(0)?;
            let n = |i: usize| r.get::<_, Option<i64>>(i).map(|x| x.unwrap_or(0));
            let total = n(1)?;
            let tokens = if total > 0 { total } else { n(2)? + n(3)? + n(4)? + n(5)? + n(6)? };
            Ok((at.max(0) as u64, tokens, 1))
        });
        if let Ok(rows) = rows {
            entries.extend(rows.flatten());
        }
    }
    Some(bucket(&entries, now))
}

pub fn read_hermes(db: &Path, now: u64) -> Option<TokenUsage> {
    let conn = open_ro(db)?;
    let sql = "SELECT last_seen, input_tokens + cache_read_tokens + cache_write_tokens + output_tokens, api_call_count
               FROM session_model_usage
               WHERE billing_provider = 'gemini' AND last_seen >= ?1";
    let mut entries = Vec::new();
    if let Ok(mut stmt) = conn.prepare(sql) {
        let rows = stmt.query_map([(start_of_month(now) / 1000) as f64], |r| {
            let secs: f64 = r.get(0)?;
            Ok(((secs * 1000.0).max(0.0) as u64, r.get::<_, Option<i64>>(1)?.unwrap_or(0), r.get::<_, Option<i64>>(2)?.unwrap_or(0)))
        });
        if let Ok(rows) = rows {
            entries.extend(rows.flatten());
        }
    }
    Some(bucket(&entries, now))
}

pub struct Paths {
    pub cli: PathBuf,
    pub opencode: PathBuf,
    pub hermes: PathBuf,
}

impl Paths {
    fn default() -> Option<Paths> {
        let h = home()?;
        Some(Paths {
            cli: h.join(".gemini").join("tmp"),
            opencode: h.join(".local").join("share").join("opencode").join("opencode.db"),
            hermes: h.join(".hermes").join("state.db"),
        })
    }
}

/// (tool name, usage) for every tool with state on disk, in a fixed order
pub fn sources(p: &Paths, now: u64) -> Vec<(&'static str, TokenUsage)> {
    [
        ("Gemini CLI", read_cli(&p.cli, now)),
        ("OpenCode", read_opencode(&p.opencode, now)),
        ("Hermes", read_hermes(&p.hermes, now)),
    ]
    .into_iter()
    .filter_map(|(n, u)| u.map(|u| (n, u)))
    .collect()
}

/// The whole visible surface, pure so it can be tested without a disk
pub fn snapshot(sources: &[(&str, TokenUsage)], now: u64) -> UsageSnapshot {
    if sources.is_empty() {
        return UsageSnapshot { status: "absent".into(), ..Default::default() };
    }
    let total = sources.iter().fold(TokenUsage::default(), |a, (_, u)| a.add(*u));
    let tokens = |id: &str, label: String, count: i64, resets_at: Option<u64>| LimitWindow {
        id: id.into(),
        label,
        count: Some(count),
        unit: "tokens".into(),
        resets_at,
        derived: true,
        ..Default::default()
    };
    let mut windows = vec![
        tokens("month", "Tokens this month".into(), total.month, start_of_next_month(now)),
        tokens("today", "Tokens today".into(), total.today, start_of_tomorrow(now)),
    ];
    // Which log each number came from, so the headline can be checked
    for (name, u) in sources {
        windows.push(tokens("source", format!("{name} · this month"), u.month, None));
    }
    UsageSnapshot {
        status: "ok".into(),
        windows,
        fetched_at: now,
        note: format!("{} calls this month · billed per token, no limit · counted from local logs; your API key is never read", total.calls),
        source: "local".into(),
        ..Default::default()
    }
}

/// Local-only: nothing to persist, a fresh count is one disk read away
pub fn load_persisted() -> UsageSnapshot {
    UsageSnapshot { status: "absent".into(), ..Default::default() }
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        crate::activity::lower_thread_priority();
        loop {
            let now = now_ms();
            let snap = Paths::default().map(|p| snapshot(&sources(&p, now), now)).unwrap_or_else(load_persisted);
            {
                let st = app.state::<AppState>();
                *st.usage.get(ID).lock().unwrap_or_else(|e| e.into_inner()) = snap;
            }
            crate::providers::publish(&app);
            for _ in 0..POLL_SECS {
                if REFRESH.swap(false, std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    });
}

/// For doctor: paths only, no contents
pub fn probe() -> String {
    let Some(p) = Paths::default() else { return "Gemini API: no home directory".into() };
    let found = sources(&p, now_ms());
    format!(
        "Gemini API: {} (from Gemini CLI {}, OpenCode {}, Hermes {})",
        if found.is_empty() { "no local logs".to_string() } else { format!("{} source(s)", found.len()) },
        p.cli.display(),
        p.opencode.display(),
        p.hermes.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, m: u32, d: u32, h: u32) -> u64 {
        Local.with_ymd_and_hms(y, m, d, h, 0, 0).earliest().unwrap().timestamp_millis() as u64
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("codenotch-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn bucket_counts_this_month_and_today_in_local_time_and_skips_aborted_calls() {
        let now = at(2026, 9, 23, 15);
        let e = vec![(at(2026, 9, 23, 9), 100, 1), (at(2026, 9, 2, 9), 50, 1), (at(2026, 8, 31, 23), 999, 1), (at(2026, 9, 23, 10), 0, 1)];
        assert_eq!(bucket(&e, now), TokenUsage { month: 150, today: 100, calls: 2 });
    }

    #[test]
    fn cli_records_are_counted_once_per_id_with_the_later_line_winning() {
        let text = include_str!("../fixtures/gemini_cli_chat.jsonl");
        let mut c = cli_calls(text);
        c.sort();
        assert_eq!(c.len(), 2, "the start record and the usage record of one call are one call");
        assert_eq!(c.iter().map(|x| x.1).sum::<i64>(), 12627 + 900);
    }

    #[test]
    fn a_missing_tool_is_absent_not_zero() {
        let p = Paths { cli: tmp("none").join("nope"), opencode: tmp("none2").join("x.db"), hermes: tmp("none3").join("y.db") };
        assert!(sources(&p, now_ms()).is_empty());
        assert_eq!(snapshot(&[], now_ms()).status, "absent");
    }

    #[test]
    fn the_cli_reader_walks_projects_and_chats() {
        let root = tmp("cli");
        let chats = root.join("9d2c").join("chats");
        std::fs::create_dir_all(&chats).unwrap();
        let now = now_ms();
        let ts = chrono::DateTime::from_timestamp_millis(now as i64).unwrap().to_rfc3339();
        std::fs::write(chats.join("s.jsonl"), format!("{{\"sessionId\":\"x\"}}\n{{\"id\":\"a\",\"type\":\"gemini\",\"timestamp\":\"{ts}\",\"tokens\":{{\"total\":42}}}}\n")).unwrap();
        std::fs::write(chats.join("notes.txt"), "ignored").unwrap();
        assert_eq!(read_cli(&root, now), Some(TokenUsage { month: 42, today: 42, calls: 1 }));
    }

    #[test]
    fn opencode_counts_google_only_and_falls_back_to_components() {
        let dir = tmp("oc");
        let db = dir.join("opencode.db");
        let c = rusqlite::Connection::open(&db).unwrap();
        c.execute_batch("CREATE TABLE message(id TEXT, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT);").unwrap();
        let now = now_ms() as i64;
        let ins = |data: &str| c.execute("INSERT INTO message VALUES('m','s',?1,?1,?2)", rusqlite::params![now, data]).unwrap();
        ins(r#"{"role":"assistant","providerID":"google","tokens":{"total":96008}}"#);
        ins(r#"{"role":"assistant","providerID":"google","tokens":{"input":10,"output":5,"reasoning":1,"cache":{"read":4,"write":0}}}"#);
        ins(r#"{"role":"assistant","providerID":"google-vertex","tokens":{"total":5000}}"#);
        ins(r#"{"role":"user","providerID":"google","tokens":{"total":7}}"#);
        drop(c);
        assert_eq!(read_opencode(&db, now as u64), Some(TokenUsage { month: 96028, today: 96028, calls: 2 }));
    }

    #[test]
    fn hermes_counts_gemini_billing_without_adding_reasoning_twice() {
        let dir = tmp("hermes");
        let db = dir.join("state.db");
        let c = rusqlite::Connection::open(&db).unwrap();
        c.execute_batch("CREATE TABLE session_model_usage(session_id, model, billing_provider, api_call_count, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens, last_seen REAL);").unwrap();
        let secs = now_ms() as f64 / 1000.0;
        c.execute("INSERT INTO session_model_usage VALUES('s','gemini-2.5-pro','gemini',3,100,20,5,0,999,?1)", [secs]).unwrap();
        c.execute("INSERT INTO session_model_usage VALUES('s','gpt','openai',1,100,20,5,0,0,?1)", [secs]).unwrap();
        drop(c);
        assert_eq!(read_hermes(&db, now_ms()), Some(TokenUsage { month: 125, today: 125, calls: 3 }));
    }

    #[test]
    fn the_snapshot_names_each_source_and_never_invents_a_percentage() {
        let now = at(2026, 9, 23, 15);
        let s = snapshot(&[("Gemini CLI", TokenUsage { month: 1_500_000, today: 2_000, calls: 12 }), ("Hermes", TokenUsage { month: 10, today: 0, calls: 1 })], now);
        assert_eq!(s.status, "ok");
        let labels: Vec<&str> = s.windows.iter().map(|w| w.label.as_str()).collect();
        assert_eq!(labels, vec!["Tokens this month", "Tokens today", "Gemini CLI · this month", "Hermes · this month"]);
        assert_eq!(s.windows[0].count, Some(1_500_010));
        assert!(s.windows.iter().all(|w| w.used == 0.0 && w.unit == "tokens" && w.derived));
        assert_eq!(s.windows[0].resets_at, Some(at(2026, 10, 1, 0)));
        assert!(s.note.starts_with("13 calls"));
    }

}
