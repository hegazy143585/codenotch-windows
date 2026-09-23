//! OpenCode Go (W-07, ported from macOS `OpenCodeProvider`): the Go plan's account-wide windows from
//! `GET https://opencode.ai/zen/go/v1/usage`, with the `opencode-go` key OpenCode's own sign-in
//! stores in `~/.local/share/opencode/auth.json` (OpenCode uses XDG paths on Windows too). Any other
//! entry there is another vendor's key and is never used. `percent` is already "used".
//! A 403 means the key has no Go subscription: nothing to meter, not a sign-in problem.

use crate::notes;
use crate::remote::{self, iso_ms, s, Fetch, Spec};
use crate::usage::{LimitWindow, UsageSnapshot};
use std::sync::atomic::AtomicBool;
use tauri::AppHandle;

pub const ID: &str = "opencode";
const ENDPOINT: &str = "https://opencode.ai/zen/go/v1/usage";
static REFRESH: AtomicBool = AtomicBool::new(false);
static SPEC: Spec = Spec { id: ID, file: "opencode.json", poll_secs: 300, fetch, refresh: &REFRESH };

pub fn request_refresh() {
    REFRESH.store(true, std::sync::atomic::Ordering::Relaxed);
}
pub fn load_persisted() -> UsageSnapshot {
    remote::load_persisted(SPEC.file)
}
pub fn start(app: AppHandle) {
    remote::start(app, &SPEC)
}

pub fn key_from(auth: &serde_json::Value) -> Option<String> {
    let e = auth.get("opencode-go")?;
    s(Some(e)).or_else(|| ["key", "apiKey", "api_key", "token", "accessToken"].iter().find_map(|k| s(e.get(*k))))
}

fn key() -> Option<String> {
    remote::home_json(&[".local", "share", "opencode", "auth.json"]).and_then(|v| key_from(&v))
}

pub fn parse(v: &serde_json::Value) -> Fetch {
    let Some(u) = v.get("usage") else { return Fetch::Other("not an OpenCode usage reply".into()) };
    let windows: Vec<LimitWindow> = [("rolling", "5h limit"), ("weekly", "Weekly limit"), ("monthly", "Monthly limit")]
        .iter()
        .filter_map(|(id, label)| {
            let e = u.get(*id)?;
            let pct = e.get("percent").and_then(|x| x.as_f64())?;
            Some(LimitWindow { id: (*id).into(), label: (*label).into(), used: (pct / 100.0).clamp(0.0, 1.0), resets_at: iso_ms(e.get("resetsAt")), ..Default::default() })
        })
        .collect();
    if windows.is_empty() {
        return Fetch::Other("OpenCode answered without usage windows".into());
    }
    Fetch::Ok { windows, note: vec![notes::text("OpenCode Go")] }
}

fn fetch() -> Fetch {
    let Some(k) = key() else { return Fetch::Absent };
    let resp = remote::agent().get(ENDPOINT).set("Authorization", &format!("Bearer {k}")).set("Accept", "application/json").call();
    if let Err(ureq::Error::Status(403, _)) = resp {
        return Fetch::Nothing(vec![notes::c("nOpencodeNoSub")]);
    }
    remote::call(resp, notes::c("nOpencodeRejected"), |v| parse(&v))

}

pub fn probe() -> String {
    match key() {
        Some(k) => format!("OpenCode Go: key found in auth.json ({} chars)", k.len()),
        None => "OpenCode Go: no opencode-go entry in ~/.local/share/opencode/auth.json".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_the_go_entry_is_claimed() {
        assert_eq!(key_from(&json!({"opencode-go": {"type": "api", "key": "k"}})).as_deref(), Some("k"));
        assert_eq!(key_from(&json!({"opencode-go": "k2"})).as_deref(), Some("k2"));
        assert_eq!(key_from(&json!({"openai": {"key": "sk"}, "google": "g"})), None);
        assert_eq!(key_from(&json!({"opencode-go": {"key": ""}})), None);
    }

    #[test]
    fn the_recorded_reply_yields_three_windows_with_millisecond_resets() {
        let v: serde_json::Value = serde_json::from_str(include_str!("../fixtures/opencode_go_usage.json")).unwrap();
        let Fetch::Ok { windows, .. } = parse(&v) else { panic!() };
        assert_eq!(windows.iter().map(|w| w.label.as_str()).collect::<Vec<_>>(), vec!["5h limit", "Weekly limit", "Monthly limit"]);
        assert!((windows[0].used - 0.07).abs() < 1e-9);
        assert_eq!(windows[0].resets_at, Some(1_788_697_866_611));
    }

    #[test]
    fn a_reply_without_windows_is_an_error_not_zero() {
        assert!(matches!(parse(&json!({"usage": {}})), Fetch::Other(_)));
        assert!(matches!(parse(&json!({})), Fetch::Other(_)));
    }
}
