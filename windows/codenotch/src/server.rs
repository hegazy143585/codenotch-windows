//! Local event server: receives codenotch-hook's POST /event?e=<event>&ppid=<pid>
//! with the Claude Code hook's stdin JSON as the body. Lenient parsing: no missing field is an error.

use crate::state::HookEvent;
use crate::AppState;
use std::io::Read;
use tauri::{AppHandle, Manager};

pub fn start(app: AppHandle, port: u16) {
    std::thread::spawn(move || {
        let server = match tiny_http::Server::http(("127.0.0.1", port)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[codenotch] failed to bind port {port}: {e} (is another instance running?)");
                return;
            }
        };
        for mut req in server.incoming_requests() {
            let url = req.url().to_string();
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
