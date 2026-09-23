//! Ollama cloud (W-07, ported from macOS `OllamaProvider`): `GET https://ollama.com/api/usage`
//! with the user's own Ollama API key, from `OLLAMA_API_KEY` or the Windows Credential Manager
//! entry `codenotch:ollama-api-key` (the settings window stores it there).
//!
//! Modern plans answer one `monthly` fraction; legacy plans answer `session` and `weekly`. The API
//! exposes no billing-cycle reset, so the windows carry none rather than a guess. Per-model request
//! counts ride along as count rows.

use crate::notes;
use crate::remote::{self, Fetch, Spec};
use crate::usage::{LimitWindow, UsageSnapshot};
use std::sync::atomic::AtomicBool;
use tauri::AppHandle;

pub const ID: &str = "ollama-cloud";
pub const CRED_TARGET: &str = "codenotch:ollama-api-key";
const ENDPOINT: &str = "https://ollama.com/api/usage";
static REFRESH: AtomicBool = AtomicBool::new(false);
static SPEC: Spec = Spec { id: ID, file: "ollama-cloud.json", poll_secs: 300, fetch, refresh: &REFRESH };

pub fn request_refresh() {
    REFRESH.store(true, std::sync::atomic::Ordering::Relaxed);
}
pub fn load_persisted() -> UsageSnapshot {
    remote::load_persisted(SPEC.file)
}
pub fn start(app: AppHandle) {
    remote::start(app, &SPEC)
}

fn key() -> Option<(String, &'static str)> {
    if let Some(k) = std::env::var("OLLAMA_API_KEY").ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty()) {
        return Some((k, "OLLAMA_API_KEY"));
    }
    remote::cred_read(CRED_TARGET).map(|k| (k, "Credential Manager"))
}

fn models(limit: &serde_json::Value, prefix: &str) -> Vec<LimitWindow> {
    limit
        .get("models")
        .and_then(|m| m.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|m| {
                    let name = remote::s(m.get("name"))?;
                    let count = m.get("request_count").and_then(|x| x.as_i64()).filter(|c| *c > 0)?;
                    Some(LimitWindow { id: format!("{prefix}.{name}"), label: name, count: Some(count), unit: "requests".into(), ..Default::default() })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse(v: &serde_json::Value) -> Fetch {
    let Some(limits) = v.get("limits").and_then(|l| l.as_object()) else {
        return Fetch::Other("not an Ollama usage reply".into());
    };
    let mut windows = Vec::new();
    for (key, label) in [("session", "Session usage"), ("weekly", "Weekly usage"), ("monthly", "Monthly usage")] {
        let Some(l) = limits.get(key) else { continue };
        // A fraction of the plan's allowance; 0 is a reading too
        if let Some(u) = l.get("usage").and_then(|x| x.as_f64()).filter(|u| *u >= 0.0) {
            windows.push(LimitWindow { id: key.into(), label: label.into(), used: u.clamp(0.0, 1.0), ..Default::default() });
        }
    }
    for key in ["monthly", "weekly", "session"] {
        if let Some(l) = limits.get(key) {
            windows.extend(models(l, key));
        }
    }
    if windows.is_empty() {
        return Fetch::Nothing(vec![notes::c("nOllamaNoUsage")]);
    }
    Fetch::Ok { windows, note: vec![notes::text("Ollama cloud"), notes::c("nNoResetDate")] }
}

fn fetch() -> Fetch {
    let Some((k, _)) = key() else { return Fetch::Absent };
    let resp = remote::agent().get(ENDPOINT).set("Authorization", &format!("Bearer {k}")).set("Accept", "application/json").call();
    remote::call(resp, notes::c("nOllamaKeyRejected"), |v| parse(&v))

}

pub fn probe() -> String {
    match key() {
        Some((k, src)) => format!("Ollama cloud: API key from {src} ({} chars)", k.len()),
        None => "Ollama cloud: no OLLAMA_API_KEY and no Credential Manager entry".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_modern_plan_yields_the_monthly_fraction_and_model_counts() {
        let v: serde_json::Value = serde_json::from_str(include_str!("../fixtures/ollama_cloud_usage.json")).unwrap();
        let Fetch::Ok { windows, .. } = parse(&v) else { panic!() };
        assert_eq!(windows[0].id, "monthly");
        assert!((windows[0].used - 0.152).abs() < 1e-9);
        assert_eq!(windows[0].resets_at, None, "no reset is published, none is invented");
        let models: Vec<(&str, Option<i64>)> = windows[1..].iter().map(|w| (w.label.as_str(), w.count)).collect();
        assert_eq!(models, vec![("glm-5.3", Some(468)), ("gpt-oss:120b", Some(12))]);
    }

    #[test]
    fn a_legacy_plan_yields_session_and_weekly() {
        let v = json!({"limits": {"session": {"usage": 0.12}, "weekly": {"usage": 0.41, "models": [{"name": "m", "request_count": 0}]}}});
        let Fetch::Ok { windows, .. } = parse(&v) else { panic!() };
        assert_eq!(windows.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(), vec!["session", "weekly"]);
    }

    #[test]
    fn zero_is_a_reading_and_a_foreign_reply_is_not() {
        let Fetch::Ok { windows, .. } = parse(&json!({"limits": {"monthly": {"usage": 0.0}}})) else { panic!() };
        assert_eq!(windows[0].used, 0.0);
        assert!(matches!(parse(&json!({"limits": {}})), Fetch::Nothing(_)));
        assert!(matches!(parse(&json!({"error": "x"})), Fetch::Other(_)));
    }
}
