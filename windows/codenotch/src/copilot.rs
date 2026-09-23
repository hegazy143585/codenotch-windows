//! GitHub Copilot (W-07, ported from macOS `GitHubCopilotProvider`): monthly quotas from
//! `GET https://api.github.com/copilot_internal/user`, the endpoint GitHub's own editors use. It is
//! internal, not a published API; approved by the owner for W-07.
//!
//! Token, in order: `GH_TOKEN` / `GITHUB_TOKEN`, the `oauth_token` in GitHub CLI's
//! `%APPDATA%\GitHub CLI\hosts.yml`, then `gh auth token` (GitHub CLI keeps the token in the
//! Windows Credential Manager by default, and asking gh is the supported way to read it).
//! Unlimited quotas are skipped rather than drawn as 0 %.

use crate::remote::{self, Fetch, Spec};
use crate::usage::{LimitWindow, UsageSnapshot};
use std::sync::atomic::AtomicBool;
use tauri::AppHandle;

pub const ID: &str = "copilot";
const ENDPOINT: &str = "https://api.github.com/copilot_internal/user";
static REFRESH: AtomicBool = AtomicBool::new(false);
static SPEC: Spec = Spec { id: ID, file: "copilot.json", poll_secs: 300, fetch, refresh: &REFRESH };

pub fn request_refresh() {
    REFRESH.store(true, std::sync::atomic::Ordering::Relaxed);
}
pub fn load_persisted() -> UsageSnapshot {
    remote::load_persisted(SPEC.file)
}
pub fn start(app: AppHandle) {
    remote::start(app, &SPEC)
}

/// (user, oauth_token) under the `github.com:` block of hosts.yml
pub fn parse_hosts(text: &str) -> (Option<String>, Option<String>) {
    let mut lines = text.lines().skip_while(|l| l.trim() != "github.com:");
    if lines.next().is_none() {
        return (None, None);
    }
    let (mut user, mut token) = (None, None);
    for l in lines {
        if !(l.starts_with(' ') || l.starts_with('\t')) {
            break;
        }
        let t = l.trim();
        let val = |k: &str| t.strip_prefix(k).map(|v| v.trim().trim_matches(['"', '\'']).to_string()).filter(|v| !v.is_empty());
        if let Some(v) = val("user:") {
            user = Some(v);
        }
        if let Some(v) = val("oauth_token:") {
            token = Some(v);
        }
    }
    (user, token)
}

fn hosts_text() -> Option<String> {
    let p = dirs::config_dir()?.join("GitHub CLI").join("hosts.yml");
    std::fs::read_to_string(p).ok()
}

fn gh_token() -> Option<String> {
    let mut cmd = std::process::Command::new("gh");
    cmd.args(["auth", "token", "--hostname", "github.com"]).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let t = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!t.is_empty()).then_some(t)
}

/// (token, source)
fn token() -> Option<(String, &'static str)> {
    for var in ["GH_TOKEN", "GITHUB_TOKEN"] {
        if let Some(t) = std::env::var(var).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()) {
            return Some((t, "environment"));
        }
    }
    if let Some(t) = hosts_text().and_then(|h| parse_hosts(&h).1) {
        return Some((t, "GitHub CLI hosts.yml"));
    }
    gh_token().map(|t| (t, "GitHub CLI"))
}

/// Seconds, milliseconds, RFC 3339, or a bare `YYYY-MM-DD` (midnight UTC)
fn date_ms(v: Option<&serde_json::Value>) -> Option<u64> {
    let v = v?;
    if let Some(n) = v.as_f64() {
        return Some(if n > 10_000_000_000.0 { n as u64 } else { (n * 1000.0) as u64 });
    }
    let s = v.as_str()?;
    remote::iso_ms(Some(v)).or_else(|| {
        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(0, 0, 0)).map(|d| d.and_utc().timestamp_millis().max(0) as u64)
    })
}

fn label(id: &str) -> String {
    match id {
        "premium_interactions" => "Premium requests".into(),
        "chat" => "Chat requests".into(),
        "completions" => "Completions".into(),
        other => other.split('_').map(remote::cap).collect::<Vec<_>>().join(" "),
    }
}

fn window(id: &str, q: &serde_json::Value, root: &serde_json::Value) -> Option<LimitWindow> {
    if q.get("unlimited").and_then(|x| x.as_bool()) == Some(true) {
        return None;
    }
    let n = |k: &str| q.get(k).and_then(|x| x.as_f64());
    let (ent, rem, used) = (n("entitlement"), n("remaining"), n("used"));
    let resets_at = date_ms(q.get("reset_date").or(q.get("reset_at")).or(q.get("resets_at"))).or_else(|| date_ms(root.get("quota_reset_date")));
    if ent == Some(0.0) {
        return None;
    }
    let base = LimitWindow { id: id.into(), label: label(id), resets_at, ..Default::default() };
    if let Some(e) = ent.filter(|e| *e > 0.0) {
        let consumed = used.unwrap_or_else(|| (e - rem.unwrap_or(e)).max(0.0));
        return Some(LimitWindow { used: (consumed / e).clamp(0.0, 1.0), ..base });
    }
    if let (Some(r), None) = (rem.filter(|r| *r >= 0.0), used) {
        return Some(LimitWindow { count: Some(r.round() as i64), unit: "left".into(), ..base });
    }
    used.filter(|u| *u >= 0.0).map(|u| LimitWindow { count: Some(u.round() as i64), unit: "requests".into(), ..base })
}

pub fn parse(v: &serde_json::Value) -> Fetch {
    let Some(quotas) = v.get("quota_snapshots").and_then(|q| q.as_object()) else {
        return Fetch::Other("not a Copilot quota reply".into());
    };
    let order = ["premium_interactions", "chat", "completions"];
    let mut keys: Vec<&str> = order.iter().copied().filter(|k| quotas.contains_key(*k)).collect();
    let mut rest: Vec<&str> = quotas.keys().map(String::as_str).filter(|k| !order.contains(k)).collect();
    rest.sort();
    keys.extend(rest);
    let windows: Vec<LimitWindow> = keys.iter().filter_map(|k| window(k, &quotas[*k], v)).collect();
    let plan = remote::s(v.get("copilot_plan")).map(|p| format!("{} · ", remote::cap(&p))).unwrap_or_default();
    if windows.is_empty() {
        return Fetch::Nothing(format!("{plan}GitHub Copilot reported no metered quotas (unlimited)"));
    }
    Fetch::Ok { windows, note: format!("{plan}GitHub Copilot") }
}

fn fetch() -> Fetch {
    let Some((t, _)) = token() else { return Fetch::Absent };
    let resp = remote::agent()
        .get(ENDPOINT)
        .set("Authorization", &format!("Bearer {t}"))
        .set("Accept", "application/json")
        .set("X-GitHub-Api-Version", "2022-11-28")
        .set("User-Agent", concat!("Codenotch/", env!("CARGO_PKG_VERSION")))
        .call();
    remote::call(resp, "GitHub rejected the token — run `gh auth login` and make sure Copilot is enabled", |v| parse(&v))
}

pub fn probe() -> String {
    let user = hosts_text().and_then(|h| parse_hosts(&h).0);
    match token() {
        Some((t, src)) => format!("GitHub Copilot: token from {src} ({} chars){}", t.len(), user.map(|u| format!(", user {u}")).unwrap_or_default()),
        None => "GitHub Copilot: no GH_TOKEN/GITHUB_TOKEN, no hosts.yml token, gh not signed in".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hosts_yml_github_block_only() {
        let y = "gitlab.example.com:\n    oauth_token: other\ngithub.com:\n    user: octo\n    oauth_token: \"gho_abc\"\n    git_protocol: https\nnext.host:\n    oauth_token: nope\n";
        assert_eq!(parse_hosts(y), (Some("octo".into()), Some("gho_abc".into())));
        assert_eq!(parse_hosts("github.com:\n    user: octo\n"), (Some("octo".into()), None), "keyring storage leaves no token in the file");
        assert_eq!(parse_hosts(""), (None, None));
    }

    #[test]
    fn the_quota_reply_orders_premium_first_and_skips_unlimited() {
        let v: serde_json::Value = serde_json::from_str(include_str!("../fixtures/copilot_user.json")).unwrap();
        let Fetch::Ok { windows, note } = parse(&v) else { panic!() };
        assert_eq!(windows.iter().map(|w| w.label.as_str()).collect::<Vec<_>>(), vec!["Premium requests"]);
        assert!((windows[0].used - 0.4).abs() < 1e-9, "120 of 300");
        assert_eq!(windows[0].resets_at, Some(1_790_812_800_000), "bare date = midnight UTC");
        assert_eq!(note, "Individual_pro · GitHub Copilot");
    }

    #[test]
    fn remaining_or_used_without_entitlement_become_counts() {
        let v = json!({"quota_snapshots": {"chat": {"remaining": 7}, "completions": {"used": 3}, "x_y": {"entitlement": 0}}});
        let Fetch::Ok { windows, .. } = parse(&v) else { panic!() };
        assert_eq!(windows.iter().map(|w| (w.label.as_str(), w.count, w.unit.as_str())).collect::<Vec<_>>(), vec![("Chat requests", Some(7), "left"), ("Completions", Some(3), "requests")]);
    }

    #[test]
    fn all_unlimited_is_nothing_to_meter() {
        assert!(matches!(parse(&json!({"quota_snapshots": {"chat": {"unlimited": true}}})), Fetch::Nothing(_)));
        assert!(matches!(parse(&json!({"login": "x"})), Fetch::Other(_)));
    }
}
