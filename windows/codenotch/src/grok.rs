//! Grok (W-07, ported from macOS `GrokLocalProvider`): the weekly Grok Build allowance from
//! `GET https://cli-chat-proxy.grok.com/v1/billing?format=credits`, the private endpoint Grok CLI
//! itself uses (approved by the owner for W-07), with the session Grok CLI writes to
//! `~/.grok/auth.json`. Only sessions issued by `https://auth.x.ai` are sent: Grok also supports a
//! customer IdP whose token is meant for a private proxy, and it must never reach the public host.
//! Refreshing the token is Grok CLI's job; an expired one asks the user to run `grok login`.

use crate::notes;
use crate::remote::{self, iso_ms, s, Fetch, Spec};
use crate::usage::{now_ms, LimitWindow, UsageSnapshot};
use std::sync::atomic::AtomicBool;
use tauri::AppHandle;

pub const ID: &str = "grok";
const ENDPOINT: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";
const TRUSTED_ISSUER: &str = "https://auth.x.ai";
static REFRESH: AtomicBool = AtomicBool::new(false);
static SPEC: Spec = Spec { id: ID, file: "grok.json", poll_secs: 300, fetch, refresh: &REFRESH };

pub fn request_refresh() {
    REFRESH.store(true, std::sync::atomic::Ordering::Relaxed);
}
pub fn load_persisted() -> UsageSnapshot {
    remote::load_persisted(SPEC.file)
}
pub fn start(app: AppHandle) {
    remote::start(app, &SPEC)
}

#[derive(Debug, PartialEq)]
pub struct Session {
    pub token: String,
    pub expires_at: Option<u64>,
    pub email: Option<String>,
}

/// The xAI-issued entry; a live one wins over an expired one
pub fn pick(auth: &serde_json::Value, now: u64) -> Option<Session> {
    let obj = auth.as_object()?;
    let mut keys: Vec<&String> = obj.keys().collect();
    keys.sort();
    let trusted: Vec<Session> = keys
        .into_iter()
        .filter_map(|k| {
            let e = &obj[k];
            let issuer_ok = k.starts_with(TRUSTED_ISSUER) || e.get("oidc_issuer").and_then(|x| x.as_str()) == Some(TRUSTED_ISSUER);
            if !issuer_ok {
                return None;
            }
            Some(Session { token: s(e.get("key"))?, expires_at: iso_ms(e.get("expires_at")), email: s(e.get("email")) })
        })
        .collect();
    let live = trusted.iter().position(|x| x.expires_at.map_or(true, |t| t > now));
    trusted.into_iter().nth(live.unwrap_or(0))
}

fn humanize(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_uppercase() && !out.is_empty() {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

pub fn parse(v: &serde_json::Value) -> Fetch {
    let Some(c) = v.get("config") else { return Fetch::Other("not a Grok billing reply".into()) };
    let period = c.get("currentPeriod");
    let reset = period.and_then(|p| iso_ms(p.get("end"))).or_else(|| iso_ms(c.get("billingPeriodEnd")));
    let pct = |x: Option<&serde_json::Value>| x.and_then(|v| v.as_f64()).map(|p| (p / 100.0).clamp(0.0, 1.0));
    let products = c.get("productUsage").and_then(|p| p.as_array()).cloned().unwrap_or_default();
    let mut windows = Vec::new();
    if let Some(u) = pct(c.get("creditUsagePercent")) {
        let label = products.first().and_then(|p| s(p.get("product"))).map(|n| humanize(&n)).unwrap_or_else(|| "Grok Build".into());
        windows.push(LimitWindow { id: "credits".into(), label, used: u, resets_at: reset, ..Default::default() });
    } else {
        for p in &products {
            let Some(u) = pct(p.get("usagePercent")) else { continue };
            let name = s(p.get("product")).unwrap_or_else(|| "Usage".into());
            windows.push(LimitWindow { id: if windows.is_empty() { "credits".into() } else { name.clone() }, label: humanize(&name), used: u, resets_at: reset, ..Default::default() });
        }
    }
    // A fresh weekly pool omits the percentages until usage lands: Grok's own /usage shows 0 %
    let weekly = period.and_then(|p| p.get("type")).and_then(|t| t.as_str()).is_some_and(|t| t.contains("WEEKLY"));
    if windows.is_empty() && weekly {
        windows.push(LimitWindow { id: "credits".into(), label: "Weekly limit".into(), used: 0.0, resets_at: reset, ..Default::default() });
    }
    if windows.is_empty() {
        return Fetch::Nothing(vec![notes::p("nNothingMetered", &["Grok"])]);
    }
    Fetch::Ok { windows, note: vec![notes::text("Grok CLI")] }
}

fn session() -> Option<Session> {
    remote::home_json(&[".grok", "auth.json"]).and_then(|v| pick(&v, now_ms()))
}

fn fetch() -> Fetch {
    let Some(sess) = session() else { return Fetch::Absent };
    if sess.expires_at.is_some_and(|t| t <= now_ms()) {
        return Fetch::NeedsAuth(vec![notes::c("nGrokExpired")]);
    }
    let resp = remote::agent()
        .get(ENDPOINT)
        .set("Authorization", &format!("Bearer {}", sess.token))
        .set("X-XAI-Token-Auth", "xai-grok-cli")
        .set("Accept", "application/json")
        .call();
    remote::call(resp, notes::c("nGrokRejected"), |v| parse(&v))

}

pub fn probe() -> String {
    match session() {
        Some(s) => format!(
            "Grok: xAI session in ~/.grok/auth.json ({} chars, {})",
            s.token.len(),
            match s.expires_at {
                Some(t) if t <= now_ms() => "expired",
                _ => "valid",
            }
        ),
        None => "Grok: no xAI-issued session in ~/.grok/auth.json".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_xai_issued_sessions_are_used_and_live_ones_win() {
        let auth = json!({
            "https://auth.x.ai::old": {"key": "expired", "expires_at": "2020-01-01T00:00:00Z"},
            "https://auth.x.ai::new": {"key": "live", "expires_at": "2099-01-01T00:00:00Z", "email": "a@b.c"},
            "https://idp.corp::x": {"key": "corp-token"}
        });
        let s = pick(&auth, now_ms()).unwrap();
        assert_eq!(s.token, "live");
        assert_eq!(s.email.as_deref(), Some("a@b.c"));
        assert_eq!(pick(&json!({"https://idp.corp::x": {"key": "corp"}}), now_ms()), None);
        assert_eq!(pick(&json!({"k": {"key": "t", "oidc_issuer": "https://auth.x.ai"}}), now_ms()).unwrap().token, "t");
    }

    #[test]
    fn the_recorded_credits_reply_yields_the_weekly_grok_build_window() {
        let v: serde_json::Value = serde_json::from_str(include_str!("../fixtures/grok_billing_credits.json")).unwrap();
        let Fetch::Ok { windows, .. } = parse(&v) else { panic!() };
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].label, "Grok Build");
        assert!((windows[0].used - 0.08).abs() < 1e-9);
        assert_eq!(windows[0].resets_at, iso_ms(Some(&json!("2026-09-12T08:21:18.802818+00:00"))));
        assert!(windows[0].resets_at.is_some());
    }

    #[test]
    fn a_fresh_weekly_pool_shows_zero_not_nothing() {
        let v = json!({"config": {"currentPeriod": {"type": "USAGE_PERIOD_TYPE_WEEKLY", "end": "2026-09-12T08:21:18Z"}}});
        let Fetch::Ok { windows, .. } = parse(&v) else { panic!() };
        assert_eq!((windows[0].label.as_str(), windows[0].used), ("Weekly limit", 0.0));
        assert!(matches!(parse(&json!({"config": {}})), Fetch::Nothing(_)));
    }

    #[test]
    fn product_names_are_humanized() {
        assert_eq!(humanize("GrokBuild"), "Grok Build");
    }
}
