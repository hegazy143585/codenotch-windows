//! Command Code (W-07, ported from macOS `CommandCodeProvider`): the `/alpha` billing documents the
//! Command Code desktop app itself reads (private endpoints; approved by the owner for W-07), with the
//! key from `COMMAND_CODE_API_KEY` or `~/.commandcode/auth.json` (the desktop app's login file).
//!
//! whoami → org id; then credits, subscription and usage summary for that org. The ring is monthly
//! spend over the monthly cap (`totalCost + monthlyCredits`); 5-hour and weekly windows are added when
//! their caps are known. A `resetAt` of 0 means "none", not 1970.

use crate::notes;
use crate::remote::{self, s, Fetch, Spec};
use crate::usage::{LimitWindow, UsageSnapshot};
use std::sync::atomic::AtomicBool;
use tauri::AppHandle;

pub const ID: &str = "commandcode";
const BASE: &str = "https://api.commandcode.ai/alpha";
static REFRESH: AtomicBool = AtomicBool::new(false);
static SPEC: Spec = Spec { id: ID, file: "commandcode.json", poll_secs: 300, fetch, refresh: &REFRESH };

pub fn request_refresh() {
    REFRESH.store(true, std::sync::atomic::Ordering::Relaxed);
}
pub fn load_persisted() -> UsageSnapshot {
    remote::load_persisted(SPEC.file)
}
pub fn start(app: AppHandle) {
    remote::start(app, &SPEC)
}

pub fn key_from(env: Option<String>, auth: Option<&serde_json::Value>) -> Option<String> {
    env.map(|k| k.trim().to_string()).filter(|k| !k.is_empty()).or_else(|| auth.and_then(|a| s(a.get("apiKey"))))
}

fn key() -> Option<String> {
    key_from(std::env::var("COMMAND_CODE_API_KEY").ok(), remote::home_json(&[".commandcode", "auth.json"]).as_ref())
}

/// ISO 8601, unix seconds or unix milliseconds; 0 is "no reset"
fn date_ms(v: Option<&serde_json::Value>) -> Option<u64> {
    let v = v?;
    if let Some(n) = v.as_f64() {
        return (n > 0.0).then(|| if n > 1e12 { n as u64 } else { (n * 1000.0) as u64 });
    }
    remote::iso_ms(Some(v))
}

fn plan_name(id: Option<String>) -> Option<String> {
    let id = id?;
    Some(if id.to_lowercase().contains("goat") { "GOAT".into() } else { id })
}

pub fn windows(summary: &serde_json::Value, credits: &serde_json::Value, subscription: &serde_json::Value) -> Fetch {
    let sub = subscription.get("data").unwrap_or(subscription);
    let n = |v: &serde_json::Value, k: &str| v.get(k).and_then(|x| x.as_f64());
    let used = n(summary, "totalCost").unwrap_or(0.0);
    let remaining = credits.get("credits").and_then(|c| n(c, "monthlyCredits")).unwrap_or(0.0);
    let cap = if used > 0.0 || remaining > 0.0 { used + remaining } else { 0.0 };
    if cap <= 0.0 {
        return Fetch::Nothing(vec![notes::p("nNothingMetered", &["Command Code"])]);
    }
    let mut out = vec![LimitWindow { id: "monthly".into(), label: "Monthly limit".into(), used: (used / cap).clamp(0.0, 1.0), resets_at: date_ms(sub.get("currentPeriodEnd")), ..Default::default() }];
    let limits = credits.get("windowLimits");
    for (key, label) in [("fiveHour", "5h limit"), ("weekly", "Weekly limit")] {
        let Some(e) = limits.and_then(|l| l.get(key)) else { continue };
        let Some(c) = n(e, "cap").filter(|c| *c > 0.0) else { continue };
        out.push(LimitWindow { id: key.into(), label: label.into(), used: (n(e, "used").unwrap_or(0.0) / c).clamp(0.0, 1.0), resets_at: date_ms(e.get("resetAt")), ..Default::default() });
    }
    let mut note: Vec<_> = plan_name(s(sub.get("planId"))).map(notes::text).into_iter().collect();
    note.push(notes::text("Command Code"));
    Fetch::Ok { windows: out, note }
}

const AUTH_NOTE: &str = "nCmdRejected";

fn get(path: &str, key: &str, query: &[(&str, &str)]) -> Result<serde_json::Value, Fetch> {
    let mut req = remote::agent()
        .get(&format!("{BASE}{path}"))
        .set("Authorization", &format!("Bearer {key}"))
        .set("User-Agent", "command-code-desktop")
        .set("x-command-code-version", "desktop")
        .set("Accept", "application/json");
    for (k, v) in query {
        req = req.query(k, v);
    }
    let mut out = None;
    let f = remote::call(req.call(), notes::c(AUTH_NOTE), |v| {
        out = Some(v);
        Fetch::Absent
    });
    out.ok_or(f)
}

fn fetch() -> Fetch {
    let Some(k) = key() else { return Fetch::Absent };
    let run = || -> Result<Fetch, Fetch> {
        let who = get("/whoami", &k, &[])?;
        let org = who.pointer("/org/id").and_then(|x| x.as_str()).map(String::from);
        let q: Vec<(&str, &str)> = org.as_deref().map(|o| vec![("orgId", o)]).unwrap_or_default();
        let credits = get("/billing/credits", &k, &q)?;
        let sub = get("/billing/subscriptions", &k, &q)?;
        let sub_data = sub.get("data").unwrap_or(&sub);
        let since = date_ms(sub_data.get("currentPeriodStart"))
            .and_then(|ms| chrono::DateTime::from_timestamp_millis(ms as i64))
            .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        let mut sq = q.clone();
        if let Some(sv) = since.as_deref() {
            sq.push(("since", sv));
        }
        let summary = get("/usage/summary", &k, &sq)?;
        Ok(windows(&summary, &credits, &sub))
    };
    run().unwrap_or_else(|e| e)
}

pub fn probe() -> String {
    match key() {
        Some(k) => format!("Command Code: API key found ({} chars)", k.len()),
        None => "Command Code: no COMMAND_CODE_API_KEY and no ~/.commandcode/auth.json".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_environment_key_wins_over_the_login_file() {
        let auth = json!({"apiKey": "file", "userName": "u"});
        assert_eq!(key_from(Some("env ".into()), Some(&auth)).as_deref(), Some("env"));
        assert_eq!(key_from(Some("  ".into()), Some(&auth)).as_deref(), Some("file"));
        assert_eq!(key_from(None, Some(&json!({"apiKey": ""}))), None);
    }

    #[test]
    fn monthly_spend_over_cap_with_window_limits() {
        let summary = json!({"totalCost": 12.5, "totalCount": 40});
        let credits = json!({"credits": {"monthlyCredits": 37.5}, "windowLimits": {
            "fiveHour": {"cap": 10, "used": 2, "resetAt": 1790000000},
            "weekly": {"cap": 0, "used": 1}}});
        let sub = json!({"data": {"planId": "goat-monthly", "currentPeriodEnd": "2026-10-01T00:00:00.000Z"}});
        let Fetch::Ok { windows: w, note } = windows(&summary, &credits, &sub) else { panic!() };
        assert_eq!(w.iter().map(|x| x.label.as_str()).collect::<Vec<_>>(), vec!["Monthly limit", "5h limit"]);
        assert!((w[0].used - 0.25).abs() < 1e-9);
        assert_eq!(w[0].resets_at, Some(1_790_812_800_000));
        assert_eq!(w[1].resets_at, Some(1_790_000_000_000), "seconds are converted");
        assert_eq!(notes::render(&note), "GOAT · Command Code");

    }

    #[test]
    fn no_spend_and_no_credits_is_nothing_metered() {
        assert!(matches!(windows(&json!({}), &json!({}), &json!({})), Fetch::Nothing(_)));
    }

    #[test]
    fn a_zero_reset_is_absence() {
        assert_eq!(date_ms(Some(&json!(0))), None);
        assert_eq!(date_ms(Some(&json!(1_790_000_000_000u64))), Some(1_790_000_000_000));
    }
}
