//! Ollama, local runtime (W-07, ported from macOS `OllamaLocalProvider`): which models the Ollama
//! server on this machine has loaded, from its documented `GET /api/ps`.
//!
//! A local server has no quota, so this provider shows no percentage: the card lists the loaded
//! models (size, GPU share, context) and the cell shows "—". Only loopback addresses are accepted —
//! `OLLAMA_HOST` pointing elsewhere is ignored — and redirects are refused, so monitoring can never
//! be moved to another host.

use crate::notes;
use crate::usage::{now_ms, UsageSnapshot};
use crate::AppState;
use std::time::Duration;
use tauri::{AppHandle, Manager};

pub const ID: &str = "ollama";
const DEFAULT_ADDRESS: &str = "127.0.0.1:11434";
const POLL_SECS: u64 = 60;
const ABSENT_POLL_SECS: u64 = 600;

static REFRESH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn request_refresh() {
    REFRESH.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// `OLLAMA_HOST` as Ollama accepts it (`host`, `host:port`, `http://host:port`), loopback only.
/// Returns `host:port`, or None when the value names another machine or cannot be read.
pub fn loopback_address(raw: &str) -> Option<String> {
    let s = raw.trim();
    let s = s.strip_prefix("http://").unwrap_or(s);
    if s.contains("://") || s.contains('@') || s.contains('?') || s.contains('#') {
        return None;
    }
    let s = s.trim_end_matches('/');
    if s.contains('/') {
        return None;
    }
    let (host, port) = match s.rsplit_once(':') {
        Some((h, _)) if !h.ends_with(']') && h.contains(':') => (s, "11434"), // bare IPv6 without port
        Some((h, p)) => (h, p),
        None => (s, "11434"),
    };
    let port: u16 = port.parse().ok().filter(|p| *p > 0)?;
    let host = match host.to_ascii_lowercase().as_str() {
        "localhost" | "127.0.0.1" | "" => "127.0.0.1".to_string(),
        "::1" | "[::1]" => "[::1]".to_string(),
        _ => return None,
    };
    Some(format!("{host}:{port}"))
}

fn address() -> String {
    std::env::var("OLLAMA_HOST").ok().and_then(|v| loopback_address(&v)).unwrap_or_else(|| DEFAULT_ADDRESS.into())
}

/// The Ollama desktop app installs per user under %LOCALAPPDATA%\Programs\Ollama
pub fn installed() -> bool {
    dirs::data_local_dir().map(|d| d.join("Programs").join("Ollama").join("ollama.exe").is_file()).unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub name: String,
    pub size: Option<i64>,
    pub size_vram: Option<i64>,
    pub context_length: Option<i64>,
    pub quantization: Option<String>,
}

/// `/api/ps` → loaded models, sorted by name. A reply that is not a model listing is an error, not
/// an empty list: some other service on the port must not read as "nothing loaded".
pub fn parse_ps(v: &serde_json::Value) -> Result<Vec<Model>, String> {
    let arr = v.get("models").and_then(|m| m.as_array()).ok_or("not an Ollama model listing")?;
    let mut out: Vec<Model> = Vec::new();
    for m in arr {
        let name = m.get("name").and_then(|x| x.as_str()).map(str::trim).unwrap_or("");
        if name.is_empty() || out.iter().any(|o| o.name == name) {
            return Err("not an Ollama model listing".into());
        }
        let n = |k: &str| m.get(k).and_then(|x| x.as_i64());
        if n("size").is_some_and(|x| x < 0) || n("size_vram").is_some_and(|x| x < 0) || n("context_length").is_some_and(|x| x <= 0) {
            return Err("not an Ollama model listing".into());
        }
        out.push(Model {
            name: name.to_string(),
            size: n("size"),
            size_vram: n("size_vram"),
            context_length: n("context_length"),
            quantization: m.pointer("/details/quantization_level").and_then(|x| x.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn gb(bytes: i64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}

fn describe(m: &Model) -> String {
    let mut parts = Vec::new();
    if let Some(s) = m.size {
        parts.push(gb(s));
    }
    match (m.size, m.size_vram) {
        (Some(s), Some(v)) if s > 0 && v >= s => parts.push("on GPU".into()),
        (Some(s), Some(v)) if s > 0 && v > 0 => parts.push(format!("{}% GPU", (v * 100 / s))),
        (_, Some(0)) => parts.push("CPU".into()),
        _ => {}
    }
    if let Some(q) = &m.quantization {
        parts.push(q.clone());
    }
    if let Some(c) = m.context_length {
        parts.push(format!("{}k ctx", c / 1024));
    }
    if parts.is_empty() { m.name.clone() } else { format!("{} ({})", m.name, parts.join(", ")) }
}

pub fn snapshot_from(models: &[Model], now: u64) -> UsageSnapshot {
    let first = if models.is_empty() {
        notes::c("nOllamaNoModel")
    } else {
        notes::p("nOllamaLoaded", &[&models.iter().map(describe).collect::<Vec<_>>().join("; ")])
    };
    let mut s = UsageSnapshot { status: "ok".into(), fetched_at: now, source: "local".into(), ..Default::default() };
    s.set_note(vec![first, notes::c("nLocalNoQuota")]);
    s
}

enum ReadErr {
    Unavailable,
    Other(String),
}

fn fetch(addr: &str) -> Result<Vec<Model>, ReadErr> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(3)).redirects(0).build();
    match agent.get(&format!("http://{addr}/api/ps")).set("Accept", "application/json").call() {
        Ok(r) => {
            let v: serde_json::Value = r.into_json().map_err(|_| ReadErr::Other("not an Ollama model listing".into()))?;
            parse_ps(&v).map_err(ReadErr::Other)
        }
        Err(ureq::Error::Status(code, _)) => Err(ReadErr::Other(format!("Ollama returned HTTP {code}"))),
        Err(_) => Err(ReadErr::Unavailable),
    }
}

fn read_once() -> UsageSnapshot {
    match fetch(&address()) {
        Ok(models) => snapshot_from(&models, now_ms()),
        // Installed but the server is not up: say so rather than hiding the provider
        Err(ReadErr::Unavailable) if installed() => {
            let mut s = UsageSnapshot { status: "none".into(), ..Default::default() };
            s.set_note(vec![notes::c("nOllamaDown")]);
            s
        }
        Err(ReadErr::Unavailable) => UsageSnapshot { status: "absent".into(), ..Default::default() },
        Err(ReadErr::Other(e)) => {
            let mut s = UsageSnapshot { status: "error".into(), ..Default::default() };
            s.set_note(vec![notes::text(e)]);
            s
        }
    }
}

/// Local-only: a fresh listing is one loopback request away
pub fn load_persisted() -> UsageSnapshot {
    UsageSnapshot { status: "absent".into(), ..Default::default() }
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        crate::activity::lower_thread_priority();
        loop {
            let snap = read_once();
            let wait = if snap.status == "absent" { ABSENT_POLL_SECS } else { POLL_SECS };
            {
                let st = app.state::<AppState>();
                *st.usage.get(ID).lock().unwrap_or_else(|e| e.into_inner()) = snap;
            }
            crate::providers::publish(&app);
            for _ in 0..wait {
                if REFRESH.swap(false, std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    });
}

/// For doctor
pub fn probe() -> String {
    let addr = address();
    let state = match fetch(&addr) {
        Ok(m) => format!("server up, {} model(s) loaded", m.len()),
        Err(ReadErr::Unavailable) => "server not reachable".into(),
        Err(ReadErr::Other(e)) => e,
    };
    format!("Ollama (local): {addr} {state} | app {}", if installed() { "installed" } else { "not found" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_loopback_hosts_are_accepted() {
        assert_eq!(loopback_address("127.0.0.1:11434").as_deref(), Some("127.0.0.1:11434"));
        assert_eq!(loopback_address("http://localhost:8080/").as_deref(), Some("127.0.0.1:8080"));
        assert_eq!(loopback_address("localhost").as_deref(), Some("127.0.0.1:11434"));
        assert_eq!(loopback_address("[::1]:9000").as_deref(), Some("[::1]:9000"));
        assert_eq!(loopback_address("0.0.0.0:11434"), None, "a listen-all bind is not an address to call");
        assert_eq!(loopback_address("192.168.1.5:11434"), None);
        assert_eq!(loopback_address("https://example.com"), None);
        assert_eq!(loopback_address("http://user:pw@localhost:1"), None);
        assert_eq!(loopback_address("localhost:0"), None);
        assert_eq!(loopback_address("localhost:11434/api"), None);
    }

    #[test]
    fn a_ps_listing_is_parsed_and_sorted() {
        let v: serde_json::Value = serde_json::from_str(include_str!("../fixtures/ollama_ps.json")).unwrap();
        let m = parse_ps(&v).unwrap();
        assert_eq!(m.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), vec!["llama3.2:3b", "qwen2.5-coder:7b"]);
        assert_eq!(m[0].quantization.as_deref(), Some("Q4_K_M"));
        assert_eq!(describe(&m[0]), "llama3.2:3b (3.4 GB, on GPU, Q4_K_M, 4k ctx)");
        assert_eq!(describe(&m[1]), "qwen2.5-coder:7b (6.0 GB, 50% GPU)");
    }

    #[test]
    fn an_empty_listing_means_nothing_loaded_not_an_error() {
        let m = parse_ps(&serde_json::json!({"models": []})).unwrap();
        let s = snapshot_from(&m, 1);
        assert_eq!(s.status, "ok");
        assert!(s.windows.is_empty(), "a local runtime has no quota to draw");
        assert_eq!(s.note, "Server running · no model loaded · local server, no quota");
        assert_eq!(s.note_parts, vec![notes::c("nOllamaNoModel"), notes::c("nLocalNoQuota")]);

    }

    #[test]
    fn a_foreign_reply_is_refused() {
        assert!(parse_ps(&serde_json::json!({"status": "ok"})).is_err());
        assert!(parse_ps(&serde_json::json!({"models": [{"name": ""}]})).is_err());
        assert!(parse_ps(&serde_json::json!({"models": [{"name": "a"}, {"name": "a"}]})).is_err());
        assert!(parse_ps(&serde_json::json!({"models": [{"name": "a", "size": -1}]})).is_err());
    }
}
