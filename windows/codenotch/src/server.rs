//! Local event server: receives codenotch-hook's POST /event?e=<event>&ppid=<pid>
//! with the Claude Code hook's stdin JSON as the body. Lenient parsing: no missing field is an error.

use crate::state::HookEvent;
use crate::AppState;
use std::io::Read;
use tauri::{AppHandle, Emitter, Manager};

/// Why the event server is not listening, if it is not (W-20). The page shows it as a banner that
/// stays up until the port is ours; without it a second copy silently loses every Claude event.
static BIND_ALERT: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
const BIND_RETRY_SECS: u64 = 30;

pub fn bind_alert() -> Option<String> {
    BIND_ALERT.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn set_bind_alert(app: &AppHandle, msg: Option<String>) {
    let changed = {
        let mut a = BIND_ALERT.lock().unwrap_or_else(|e| e.into_inner());
        let changed = *a != msg;
        *a = msg.clone();
        changed
    };
    if changed {
        let _ = app.emit("alert", msg);
    }
}

pub(crate) fn bind_failure_message(port: u16, holder_is_codenotch: bool) -> String {
    if holder_is_codenotch {
        "Another Codenotch is running — close it (tray → Quit) so this one can show Claude activity".into()
    } else {
        format!("Port {port} is used by another program — Claude activity is off until it is free (or change \"port\" in config.json)")
    }
}

/// Does the program holding the port answer like a Codenotch? Its `GET /activity` returns a JSON array.
fn holder_is_codenotch(port: u16) -> bool {
    ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .get(&format!("http://127.0.0.1:{port}/activity"))
        .call()
        .ok()
        .and_then(|r| r.into_string().ok())
        .map(|b| looks_like_activity(&b))
        .unwrap_or(false)
}

pub(crate) fn looks_like_activity(body: &str) -> bool {
    matches!(serde_json::from_str::<serde_json::Value>(body), Ok(serde_json::Value::Array(_)))
}

pub fn start(app: AppHandle, port: u16) {
    std::thread::spawn(move || {
        // Keep trying: the other copy may be quit from its tray, and then this one takes over
        let server = loop {
            match tiny_http::Server::http(("127.0.0.1", port)) {
                Ok(s) => {
                    set_bind_alert(&app, None);
                    break s;
                }
                Err(e) => {
                    let msg = bind_failure_message(port, holder_is_codenotch(port));
                    if bind_alert().as_deref() != Some(msg.as_str()) {
                        crate::applog(&format!("event server: failed to bind 127.0.0.1:{port}: {e} — {msg}"));
                    }
                    set_bind_alert(&app, Some(msg));
                    std::thread::sleep(std::time::Duration::from_secs(BIND_RETRY_SECS));
                }
            }
        };
        for mut req in server.incoming_requests() {
            let url = req.url().to_string();
            // Only the local hook binary and local tools may talk to this server. Browsers always send
            // Origin on cross-site POSTs, and a DNS-rebinding page arrives with a foreign Host, so both
            // are refused before any state is touched or read.
            let host = header(&req, "Host");
            let origin = header(&req, "Origin");
            if !request_allowed(host.as_deref(), origin.as_deref(), port) {
                let _ = req.respond(tiny_http::Response::from_string("forbidden").with_status_code(403));
                continue;
            }
            let mut body = String::new();
            let _ = req
                .as_reader()
                .take(256 * 1024)
                .read_to_string(&mut body);
            // GET /activity: the merged working-state list as JSON (pushed rows + probes). Read-only and
            // loopback-only; lets `doctor` and anyone wiring up a new provider verify the notch's view
            // without opening the UI.
            if req.method() == &tiny_http::Method::Get && url.starts_with("/activity") {
                let body = {
                    let st = app.state::<AppState>();
                    let a = st.activity.lock().unwrap();
                    serde_json::to_string(&*a).unwrap_or_else(|_| "[]".into())
                };
                let hdr = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                let _ = req.respond(tiny_http::Response::from_string(body).with_header(hdr));
                continue;
            }
            if url.starts_with("/event") {
                // `p=<provider>` tags an event as belonging to a non-Claude module. Claude keeps the
                // rich per-session four-state store (titles, jump-to-terminal); every other provider
                // rides the generic activity surface, which the notch already renders per provider. So
                // one transport (codenotch-hook) and one endpoint serve every module — the reliable,
                // update-proof path is no longer Claude-only.
                let provider = query_param(&url, "p");
                if !provider.is_empty() && !valid_provider_id(&provider) {
                    let _ = req.respond(tiny_http::Response::from_string("bad provider id").with_status_code(400));
                    continue;
                }
                if provider.is_empty() || provider == "claude" {
                    let ev = parse(&url, &body);
                    // The token renewer's own `claude -p` launch registers a session for the second it
                    // lives; it exits before a prompt could ever arrive and must never read as work
                    if ev.ppid != 0 && ev.ppid == crate::claude_refresh::IGNORED_PID.load(std::sync::atomic::Ordering::Relaxed) {
                        let _ = req.respond(tiny_http::Response::from_string("ok"));
                        continue;
                    }
                    let finished = ev.e == "done";
                    let state = app.state::<AppState>();
                    let changed = {
                        let mut store = state.store.lock().unwrap();
                        store.apply(ev)
                    };
                    if changed {
                        crate::broadcast(&app);
                    }
                    if finished {
                        crate::usage::request_refresh(); // the turn ended: the usage number just moved
                    }
                } else {
                    let e = query_param(&url, "e");
                    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
                    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
                    let session = {
                        let sid = s("session_id");
                        if sid.is_empty() { query_param(&url, "sid") } else { sid }
                    };
                    // name/detail are optional labels the integration may supply (query wins over body)
                    let name = {
                        let q = query_param(&url, "name");
                        if q.is_empty() { let b = s("name"); if b.is_empty() { s("prompt") } else { b } } else { q }
                    };
                    let detail = {
                        let q = query_param(&url, "detail");
                        if q.is_empty() { s("detail") } else { q }
                    };
                    crate::activity::push(&provider, &session, &e, &name, &detail);
                    // The activity thread owns emission (it dedups and re-sorts); it reflects this push on
                    // its next 2 s tick, the same cadence the other providers already update at.
                }
            }
            let _ = req.respond(tiny_http::Response::from_string("ok"));
        }
    });
}

fn header(req: &tiny_http::Request, name: &'static str) -> Option<String> {
    req.headers()
        .iter()
        .find(|h| h.field.equiv(name))
        .map(|h| h.value.as_str().to_string())
}

/// Loopback Host (with or without our port) and no browser Origin. The hook sends `Host: 127.0.0.1`.
pub(crate) fn request_allowed(host: Option<&str>, origin: Option<&str>, port: u16) -> bool {
    if origin.is_some() {
        return false;
    }
    let Some(h) = host else { return true }; // HTTP/1.0 clients may omit Host; the socket is loopback-only
    let h = h.trim().to_ascii_lowercase();
    let with_port = |name: &str| h == name || h == format!("{name}:{port}");
    with_port("127.0.0.1") || with_port("localhost") || with_port("[::1]")
}

/// Provider ids end up as DOM attributes and map keys: lowercase ASCII letters, digits, `-` and `_`, 1–32 chars.
pub(crate) fn valid_provider_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn query_param(url: &str, key: &str) -> String {
    let q = url.splitn(2, '?').nth(1).unwrap_or("");
    for pair in q.split('&') {
        let mut it = pair.splitn(2, '=');
        if it.next() == Some(key) {
            return it.next().unwrap_or("").to_string();
        }
    }
    String::new()
}

fn parse(url: &str, body: &str) -> HookEvent {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    // tool_input.command (Bash etc.) feeds the "last action" summary
    let tool_cmd = v
        .get("tool_input")
        .and_then(|t| t.get("command"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    HookEvent {
        e: query_param(url, "e"),
        session_id: {
            let id = s("session_id");
            if id.is_empty() { "unknown".into() } else { id }
        },
        ppid: query_param(url, "ppid").parse().unwrap_or(0),
        cwd: s("cwd"),
        prompt: s("prompt"),
        message: s("message"),
        tool_name: s("tool_name"),
        tool_cmd,
        model: s("model"),
        src: "hook",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_request_is_allowed() {
        assert!(request_allowed(Some("127.0.0.1"), None, 48666));
        assert!(request_allowed(Some("127.0.0.1:48666"), None, 48666));
        assert!(request_allowed(Some("localhost:48666"), None, 48666));
        assert!(request_allowed(None, None, 48666));
    }

    #[test]
    fn browser_origin_is_refused() {
        assert!(!request_allowed(Some("127.0.0.1:48666"), Some("https://evil.example"), 48666));
        assert!(!request_allowed(Some("127.0.0.1:48666"), Some("null"), 48666));
    }

    #[test]
    fn dns_rebinding_host_is_refused() {
        assert!(!request_allowed(Some("evil.example:48666"), None, 48666));
        assert!(!request_allowed(Some("127.0.0.1:1234"), None, 48666));
    }

    #[test]
    fn a_port_held_by_another_codenotch_says_so() {
        assert!(bind_failure_message(48666, true).starts_with("Another Codenotch is running"));
        let m = bind_failure_message(48666, false);
        assert!(m.contains("48666") && m.contains("another program"));
    }

    #[test]
    fn only_a_json_array_counts_as_a_codenotch_reply() {
        assert!(looks_like_activity("[]"));
        assert!(looks_like_activity(r#"[{"provider":"codex"}]"#));
        assert!(!looks_like_activity("<html>"));
        assert!(!looks_like_activity(r#"{"ok":true}"#));
        assert!(!looks_like_activity(""));
    }

    /// W-18: the pages run under a CSP; scripts only from the app (Tauri hashes the inline ones)
    #[test]
    fn the_app_pages_have_a_strict_script_policy() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let csp = conf["app"]["security"]["csp"].as_str().expect("csp must be set, not null");
        let script = csp.split(';').map(str::trim).find(|d| d.starts_with("script-src")).expect("script-src");
        assert!(!script.contains("unsafe-inline") && !script.contains("unsafe-eval"), "{script}");
        for d in ["object-src 'none'", "base-uri 'none'", "frame-ancestors 'none'", "connect-src ipc: http://ipc.localhost"] {
            assert!(csp.contains(d), "missing {d}");
        }
    }

    #[test]
    fn provider_ids_are_validated() {
        assert!(valid_provider_id("copilot"));
        assert!(valid_provider_id("command-code"));
        assert!(valid_provider_id("open_code2"));
        assert!(!valid_provider_id(""));
        assert!(!valid_provider_id("Copilot"));
        assert!(!valid_provider_id("x\"><img src=x onerror=alert(1)>"));
        assert!(!valid_provider_id(&"a".repeat(33)));
    }
}
