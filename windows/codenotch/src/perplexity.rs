//! Perplexity (W-07, ported from macOS `WebSessionProvider` + `PerplexityUsage`): what is left of
//! each Perplexity quota, from `GET /rest/rate-limit/all` — the endpoint perplexity.ai's own web app
//! calls (unofficial; approved by the owner for W-07).
//!
//! The endpoint sits behind Cloudflare bot checks, and a copied cookie does not pass them (the
//! clearance is bound to the browser that earned it). So the request is made *inside a browser the
//! user signs into themselves*: a Codenotch window showing perplexity.ai, in the app's own WebView2
//! profile. Nothing is taken from the user's real browser, nothing is faked, and a challenge is only
//! ever answered by the person at the keyboard.
//!
//! Nothing runs until the user connects (tray → Connect Perplexity). Then every 5 min a hidden
//! window loads perplexity.ai, runs the fetch in the page, and hands the result back by navigating
//! to `https://codenotch.invalid/report?...` — a navigation this module intercepts and cancels. The
//! page therefore gets no Tauri IPC access at all; the worst a hostile page could do is report a
//! wrong number about itself.
//!
//! The reply states only what is *left* (no totals, no reset times), so the card shows counts.

use crate::remote::{self, Fetch};
use crate::usage::{now_ms, LimitWindow, UsageSnapshot};
use crate::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const ID: &str = "perplexity";
const ORIGIN: &str = "https://www.perplexity.ai/";
const FETCH_LABEL: &str = "perplexity-fetch";
const SIGNIN_LABEL: &str = "perplexity-signin";
const REPORT_HOST: &str = "codenotch.invalid";
const POLL_SECS: u64 = 300;
const FILE: &str = "perplexity.json";

static REFRESH: AtomicBool = AtomicBool::new(false);

/// Runs in the page after load. Only once per page, whatever else loads afterwards.
const SCRIPT: &str = r#"(async () => {
  if (window.__codenotchDone) return; window.__codenotchDone = true;
  const report = (s, d) => { location.href = 'https://codenotch.invalid/report?s=' + s + '&d=' + encodeURIComponent(String(d).slice(0, 20000)); };
  try {
    const r = await fetch('/rest/rate-limit/all', { credentials: 'include', headers: { accept: 'application/json' } });
    report(r.status, await r.text());
  } catch (e) { report(0, e); }
})();"#;

pub fn request_refresh() {
    REFRESH.store(true, Ordering::Relaxed);
}

pub fn load_persisted() -> UsageSnapshot {
    let mut s = remote::load_persisted(FILE);
    if !connected_cfg() {
        s = UsageSnapshot { status: "absent".into(), ..Default::default() };
    }
    s
}

fn connected_cfg() -> bool {
    crate::config::load().perplexity
}

fn connected(app: &AppHandle) -> bool {
    app.state::<AppState>().cfg.lock().unwrap_or_else(|e| e.into_inner()).perplexity
}

fn set_connected(app: &AppHandle, on: bool) {
    let st = app.state::<AppState>();
    let mut c = st.cfg.lock().unwrap_or_else(|e| e.into_inner());
    c.perplexity = on;
    crate::config::save(&c);
}

/// Parse `/rest/rate-limit/all`: remaining counts, the headline first
pub fn parse(v: &serde_json::Value) -> Fetch {
    if !v.is_object() {
        return Fetch::Other("not a Perplexity rate-limit reply".into());
    }
    let mut windows: Vec<LimitWindow> = [
        ("remaining_pro", "Pro searches"),
        ("remaining_research", "Research"),
        ("remaining_agentic_research", "Agentic research"),
        ("remaining_labs", "Labs"),
    ]
    .iter()
    .filter_map(|(k, label)| {
        let n = v.get(*k)?.as_i64()?;
        Some(LimitWindow { id: (*k).into(), label: (*label).into(), count: Some(n), unit: "left".into(), ..Default::default() })
    })
    .collect();
    // Only an exact count means anything; Perplexity also reports vaguer kinds
    if v.pointer("/free_queries/remaining_detail/kind").and_then(|x| x.as_str()) == Some("exact") {
        if let Some(n) = v.pointer("/free_queries/remaining_detail/remaining").and_then(|x| x.as_i64()) {
            windows.push(LimitWindow { id: "free_queries".into(), label: "Free queries".into(), count: Some(n), unit: "left".into(), ..Default::default() });
        }
    }
    if windows.is_empty() {
        return Fetch::Other("Perplexity answered without any quota".into());
    }
    Fetch::Ok { windows, note: "Perplexity · counts left; no totals or reset times are published".into() }
}

/// The report navigation → (status, body)
pub fn read_report(url: &tauri::Url) -> Option<(u16, String)> {
    if url.host_str() != Some(REPORT_HOST) {
        return None;
    }
    let mut status = 0;
    let mut body = String::new();
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "s" => status = v.parse().unwrap_or(0),
            "d" => body = v.into_owned(),
            _ => {}
        }
    }
    Some((status, body))
}

pub fn classify(status: u16, body: &str) -> Fetch {
    match status {
        200..=299 => match serde_json::from_str::<serde_json::Value>(body) {
            Ok(v) => parse(&v),
            Err(_) => Fetch::Other("Perplexity answered something that is not JSON".into()),
        },
        401 | 403 => Fetch::NeedsAuth("Sign in, or pass Perplexity's check: click the Perplexity cell".into()),
        429 => Fetch::RateLimited(0),
        0 => Fetch::Offline("the page could not reach Perplexity".into()),
        s => Fetch::Other(format!("HTTP {s}")),
    }
}

/// Load perplexity.ai hidden, run the fetch in the page, wait for the report
fn fetch_in_page(app: &AppHandle) -> Fetch {
    if let Some(w) = app.get_webview_window(FETCH_LABEL) {
        let _ = w.destroy();
    }
    let (tx, rx) = mpsc::channel::<(u16, String)>();
    let tx = Mutex::new(Some(tx));
    let Ok(url) = ORIGIN.parse() else { return Fetch::Other("bad origin".into()) };
    let built = WebviewWindowBuilder::new(app, FETCH_LABEL, WebviewUrl::External(url))
        .visible(false)
        .focused(false)
        .skip_taskbar(true)
        .inner_size(1100.0, 800.0)
        .on_navigation(move |u| match read_report(u) {
            Some(r) => {
                if let Some(t) = tx.lock().ok().and_then(|mut g| g.take()) {
                    let _ = t.send(r);
                }
                false // cancelled: the report never leaves the machine
            }
            None => true,
        })
        .on_page_load(|w, p| {
            if matches!(p.event(), tauri::webview::PageLoadEvent::Finished) && p.url().host_str().is_some_and(|h| h.ends_with("perplexity.ai")) {
                let _ = w.eval(SCRIPT);
            }
        })
        .build();
    let win = match built {
        Ok(w) => w,
        Err(e) => return Fetch::Other(format!("could not open the Perplexity page ({e})")),
    };
    let r = rx.recv_timeout(Duration::from_secs(45));
    let _ = win.destroy();
    match r {
        Ok((status, body)) => classify(status, &body),
        Err(_) => Fetch::Offline("Perplexity did not load within 45 s".into()),
    }
}

/// Tray / cell click: show perplexity.ai in a Codenotch window so the user can sign in there
pub fn open_signin(app: &AppHandle) {
    set_connected(app, true);
    if let Some(w) = app.get_webview_window(SIGNIN_LABEL) {
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let Ok(url) = ORIGIN.parse() else { return };
    let a = app.clone();
    std::thread::spawn(move || {
        let built = WebviewWindowBuilder::new(&a, SIGNIN_LABEL, WebviewUrl::External(url))
            .title("Perplexity — sign in for Codenotch")
            .inner_size(1100.0, 800.0)
            // Leave the page only for sign-in providers and Cloudflare; anything else opens outside
            .on_navigation(|u| u.scheme() == "https" && u.host_str() != Some(REPORT_HOST))
            .build();
        match built {
            Ok(w) => {
                w.on_window_event(|e| {
                    if matches!(e, tauri::WindowEvent::Destroyed) {
                        request_refresh(); // read the new session as soon as the user is done
                    }
                });
            }
            Err(e) => crate::applog(&format!("perplexity: sign-in window failed: {e}")),
        }
    });
    request_refresh();
}

/// Tray: stop reading and forget the session this app's WebView holds
pub fn disconnect(app: &AppHandle) {
    set_connected(app, false);
    if let Some(w) = app.get_webview_window(SIGNIN_LABEL) {
        let _ = w.clear_all_browsing_data();
        let _ = w.destroy();
    } else if let Some(w) = app.get_webview_window("notch") {
        // One profile for every Codenotch webview; the notch page itself keeps nothing it needs there
        let _ = w.clear_all_browsing_data();
    }
    let _ = std::fs::remove_file(crate::config::config_path().with_file_name(FILE));
    *app.state::<AppState>().usage.get(ID).lock().unwrap_or_else(|e| e.into_inner()) = UsageSnapshot { status: "absent".into(), ..Default::default() };
    crate::providers::publish(app);
}

pub fn is_connected(app: &AppHandle) -> bool {
    connected(app)
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let mut consecutive_429 = 0u32;
        loop {
            if !connected(&app) {
                sleep(30);
                continue;
            }
            let prev = app.state::<AppState>().usage.get(ID).lock().unwrap_or_else(|e| e.into_inner()).clone();
            let now = now_ms();
            if prev.backoff_until > now {
                sleep(((prev.backoff_until - now) / 1000).max(1));
                continue;
            }
            // Never load a second copy while the user is signing in
            if app.get_webview_window(SIGNIN_LABEL).is_some() {
                sleep(5);
                continue;
            }
            let r = fetch_in_page(&app);
            let limited = matches!(r, Fetch::RateLimited(_));
            let prev = if prev.status == "absent" { UsageSnapshot::default() } else { prev };
            let next = remote::apply(&prev, r, now_ms(), consecutive_429);
            consecutive_429 = if limited { consecutive_429 + 1 } else { 0 };
            if connected(&app) {
                *app.state::<AppState>().usage.get(ID).lock().unwrap_or_else(|e| e.into_inner()) = next.clone();
                if let Ok(t) = serde_json::to_string_pretty(&next) {
                    let _ = std::fs::write(crate::config::config_path().with_file_name(FILE), t);
                }
                crate::providers::publish(&app);
            }
            sleep(POLL_SECS);
        }
    });
}

fn sleep(secs: u64) {
    for _ in 0..secs {
        if REFRESH.swap(false, Ordering::Relaxed) {
            return;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

pub fn probe() -> String {
    format!("Perplexity: {}", if connected_cfg() { "connected (read through the app's own WebView)" } else { "not connected (tray → Connect Perplexity)" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_recorded_reply_yields_counts_left_headline_first() {
        let v: serde_json::Value = serde_json::from_str(include_str!("../fixtures/perplexity_rate_limit.json")).unwrap();
        let Fetch::Ok { windows, .. } = parse(&v) else { panic!() };
        let got: Vec<(&str, Option<i64>)> = windows.iter().map(|w| (w.label.as_str(), w.count)).collect();
        assert_eq!(got, vec![("Pro searches", Some(2)), ("Research", Some(0)), ("Agentic research", Some(0)), ("Labs", Some(0)), ("Free queries", Some(10))]);
        assert!(windows.iter().all(|w| w.unit == "left" && w.resets_at.is_none()));
    }

    #[test]
    fn vague_free_query_counts_are_not_shown() {
        let Fetch::Ok { windows, .. } = parse(&json!({"remaining_pro": 1, "free_queries": {"remaining_detail": {"kind": "approximate", "remaining": 5}}})) else { panic!() };
        assert_eq!(windows.len(), 1);
    }

    #[test]
    fn only_the_report_host_is_intercepted_and_decoded() {
        let u: tauri::Url = "https://codenotch.invalid/report?s=200&d=%7B%22remaining_pro%22%3A3%7D".parse().unwrap();
        assert_eq!(read_report(&u), Some((200, r#"{"remaining_pro":3}"#.into())));
        let other: tauri::Url = "https://www.perplexity.ai/report?s=200".parse().unwrap();
        assert_eq!(read_report(&other), None);
    }

    #[test]
    fn statuses_map_to_honest_outcomes() {
        assert!(matches!(classify(403, "<html>challenge</html>"), Fetch::NeedsAuth(_)));
        assert!(matches!(classify(200, "<html>"), Fetch::Other(_)));
        assert!(matches!(classify(0, "TypeError: Failed to fetch"), Fetch::Offline(_)));
        assert!(matches!(classify(429, ""), Fetch::RateLimited(0)));
        assert!(matches!(classify(200, r#"{"remaining_pro":3}"#), Fetch::Ok { .. }));
    }
}
