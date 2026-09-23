//! GLM Coding Plan (W-07, ported from macOS `GLMProvider`): Z.ai's own usage monitor,
//! `GET <console>/api/monitor/usage/quota/limit`, with the plan key a coding tool already holds.
//!
//! Key sources, in order: Claude Code `~/.claude/settings.json` (only when `ANTHROPIC_BASE_URL` is a
//! Z.ai / bigmodel.cn host — otherwise the token is someone's Anthropic key), ZCode
//! `~/.zcode/v2/config.json` / `credentials.json` (an `enc:v1:` value is encrypted and skipped),
//! OpenCode `~/.local/share/opencode/auth.json`. The key goes in `Authorization` raw, no "Bearer".
//! The endpoint is not a published API. Errors can arrive inside an HTTP 200 envelope
//! (`{"code":401,"success":false}`), so the envelope is read before the payload is trusted.

use crate::notes;
use crate::remote::{self, s, Fetch, Spec};
use crate::usage::{LimitWindow, UsageSnapshot};
use std::sync::atomic::AtomicBool;
use tauri::AppHandle;

pub const ID: &str = "glm";
static REFRESH: AtomicBool = AtomicBool::new(false);
static SPEC: Spec = Spec { id: ID, file: "glm.json", poll_secs: 300, fetch, refresh: &REFRESH };

pub fn request_refresh() {
    REFRESH.store(true, std::sync::atomic::Ordering::Relaxed);
}
pub fn load_persisted() -> UsageSnapshot {
    remote::load_persisted(SPEC.file)
}
pub fn start(app: AppHandle) {
    remote::start(app, &SPEC)
}

const GLOBAL: &str = "https://api.z.ai";
const CHINA: &str = "https://open.bigmodel.cn";

#[derive(Debug, PartialEq)]
pub struct Credential {
    pub token: String,
    pub console: &'static str,
    pub source: &'static str,
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next()?.rsplit('@').next()?;
    let host = host.split(':').next()?.to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

pub fn is_zai_host(h: &str) -> bool {
    h == "api.z.ai" || h.ends_with(".z.ai") || h == "open.bigmodel.cn" || h.ends_with(".bigmodel.cn")
}

fn console_for(host: &str) -> &'static str {
    if host.ends_with("bigmodel.cn") { CHINA } else { GLOBAL }
}

pub fn from_claude_settings(v: &serde_json::Value) -> Option<Credential> {
    let env = v.get("env")?;
    let token = s(env.get("ANTHROPIC_AUTH_TOKEN")).or_else(|| s(env.get("ANTHROPIC_API_KEY")))?;
    let host = host_of(&s(env.get("ANTHROPIC_BASE_URL"))?)?;
    is_zai_host(&host).then(|| Credential { token, console: console_for(&host), source: "Claude Code" })
}

pub fn from_zcode_config(v: &serde_json::Value) -> Option<Credential> {
    let providers = v.get("provider")?.as_object()?;
    let mut ids: Vec<&String> = providers.keys().collect();
    ids.sort();
    for id in ids {
        let p = &providers[id];
        if !id.contains("coding-plan") || p.get("enabled").and_then(|x| x.as_bool()) == Some(false) {
            continue;
        }
        let Some(key) = s(p.pointer("/options/apiKey")) else { continue };
        let console = s(p.pointer("/options/baseURL")).and_then(|u| host_of(&u)).map(|h| console_for(&h)).unwrap_or(GLOBAL);
        return Some(Credential { token: key, console, source: "ZCode" });
    }
    None
}

pub fn from_zcode_credentials(v: &serde_json::Value) -> Option<Credential> {
    let t = s(v.get("oauth:zai:access_token"))?;
    // Encrypted at rest: a string we cannot read is a string we must not send
    (!t.starts_with("enc:v1:")).then(|| Credential { token: t, console: GLOBAL, source: "ZCode" })
}

pub fn from_opencode(v: &serde_json::Value) -> Option<Credential> {
    for id in ["zai-coding-plan", "zai", "z-ai", "z.ai", "glm", "zhipu", "zhipuai"] {
        let Some(e) = v.get(id) else { continue };
        let console = if id.starts_with("zhipu") { CHINA } else { GLOBAL };
        let key = s(Some(e)).or_else(|| ["apiKey", "api_key", "token", "key", "accessToken", "auth_token"].iter().find_map(|k| s(e.get(*k))));
        if let Some(token) = key {
            return Some(Credential { token, console, source: "OpenCode" });
        }
    }
    None
}

pub fn credential() -> Option<Credential> {
    remote::home_json(&[".claude", "settings.json"])
        .and_then(|v| from_claude_settings(&v))
        .or_else(|| remote::home_json(&[".zcode", "v2", "config.json"]).and_then(|v| from_zcode_config(&v)))
        .or_else(|| remote::home_json(&[".zcode", "v2", "credentials.json"]).and_then(|v| from_zcode_credentials(&v)))
        .or_else(|| remote::home_json(&[".local", "share", "opencode", "auth.json"]).and_then(|v| from_opencode(&v)))
}

/// Envelope + limits → windows (session, weekly, MCP), or the failure the envelope describes
pub fn parse(v: &serde_json::Value) -> Fetch {
    let success = v.get("success").and_then(|x| x.as_bool()).unwrap_or(false);
    let code = v.get("code").and_then(|x| x.as_i64());
    if !(success || code.is_none() || code == Some(200)) {
        return match code {
            Some(401 | 403) => Fetch::NeedsAuth(vec![notes::c("nZaiRejected")]),
            Some(429) => Fetch::RateLimited(0),
            Some(c) => Fetch::Other(format!("Z.ai answered code {c}")),
            None => Fetch::Other("Z.ai answered an error".into()),
        };
    }
    let data = v.get("data").cloned().unwrap_or_default();
    let mut windows: Vec<LimitWindow> = data.get("limits").and_then(|l| l.as_array()).map(|a| a.iter().filter_map(window).collect()).unwrap_or_default();
    let rank = |id: &str| match id { "session" => 0, "weekly" => 1, "mcp" => 2, _ => 3 };
    windows.sort_by(|a, b| rank(&a.id).cmp(&rank(&b.id)).then(a.id.cmp(&b.id)));
    let mut note: Vec<_> = s(data.get("level")).map(|l| notes::text(remote::cap(&l))).into_iter().collect();
    if windows.is_empty() {
        note.push(notes::p("nNoWindows", &["Z.ai"]));
        return Fetch::Nothing(note);
    }
    note.push(notes::text("GLM Coding Plan"));
    Fetch::Ok { windows, note }
}

fn window(l: &serde_json::Value) -> Option<LimitWindow> {
    // Without a percentage there is nothing to draw
    let pct = l.get("percentage").and_then(|x| x.as_f64())?;
    let (unit, number) = (l.get("unit").and_then(|x| x.as_i64()), l.get("number").and_then(|x| x.as_i64()));
    let ty = l.get("type").and_then(|x| x.as_str()).unwrap_or("");
    let (id, label) = if ty == "TIME_LIMIT" {
        ("mcp".to_string(), "MCP (1 month)".to_string())
    } else {
        match (unit, number) {
            (Some(3), Some(5)) => ("session".into(), "Current session".into()),
            (Some(6), Some(1)) => ("weekly".into(), "Weekly".into()),
            (Some(3), Some(n)) => (format!("window-3x{n}"), format!("Usage ({n} h)")),
            (Some(6), Some(n)) => (format!("window-6x{n}"), format!("Usage ({n} wk)")),
            (Some(u), Some(n)) => (format!("window-{u}x{n}"), "Usage".into()),
            _ => (if ty.is_empty() { "unknown".into() } else { ty.to_lowercase() }, "Usage".into()),
        }
    };
    Some(LimitWindow {
        id,
        label,
        used: (pct / 100.0).clamp(0.0, 1.0),
        resets_at: l.get("nextResetTime").and_then(|x| x.as_f64()).filter(|t| *t > 0.0).map(|t| t as u64),
        ..Default::default()
    })
}

fn fetch() -> Fetch {
    let Some(c) = credential() else { return Fetch::Absent };
    let resp = remote::agent()
        .get(&format!("{}/api/monitor/usage/quota/limit", c.console))
        .set("Authorization", &c.token)
        .set("Content-Type", "application/json")
        .call();
    remote::call(resp, notes::c("nZaiRejected"), |v| parse(&v))
}

/// For doctor: where the key came from, never the key
pub fn probe() -> String {
    match credential() {
        Some(c) => format!("GLM: plan key found in {} ({} chars) for {}", c.source, c.token.len(), c.console),
        None => "GLM: no Z.ai plan key in Claude Code settings, ZCode or OpenCode".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_code_settings_are_claimed_only_for_a_zai_base_url() {
        let zai = json!({"env": {"ANTHROPIC_AUTH_TOKEN": "k1", "ANTHROPIC_BASE_URL": "https://api.z.ai/api/anthropic"}});
        assert_eq!(from_claude_settings(&zai), Some(Credential { token: "k1".into(), console: GLOBAL, source: "Claude Code" }));
        let cn = json!({"env": {"ANTHROPIC_AUTH_TOKEN": "k2", "ANTHROPIC_BASE_URL": "https://open.bigmodel.cn/api/anthropic"}});
        assert_eq!(from_claude_settings(&cn).unwrap().console, CHINA);
        let anthropic = json!({"env": {"ANTHROPIC_AUTH_TOKEN": "sk-ant", "ANTHROPIC_BASE_URL": "https://api.anthropic.com"}});
        assert_eq!(from_claude_settings(&anthropic), None);
        assert_eq!(from_claude_settings(&json!({"env": {"ANTHROPIC_AUTH_TOKEN": "k"}})), None);
        let spoof = json!({"env": {"ANTHROPIC_AUTH_TOKEN": "k", "ANTHROPIC_BASE_URL": "https://api.z.ai.evil.com"}});
        assert_eq!(from_claude_settings(&spoof), None);
    }

    #[test]
    fn zcode_skips_disabled_and_encrypted_keys() {
        let cfg = json!({"provider": {
            "builtin:a-coding-plan": {"enabled": false, "options": {"apiKey": "off"}},
            "builtin:b-coding-plan": {"options": {"apiKey": "on", "baseURL": "https://open.bigmodel.cn/api/anthropic"}}}});
        assert_eq!(from_zcode_config(&cfg), Some(Credential { token: "on".into(), console: CHINA, source: "ZCode" }));
        assert_eq!(from_zcode_credentials(&json!({"oauth:zai:access_token": "enc:v1:abc"})), None);
        assert!(from_zcode_credentials(&json!({"oauth:zai:access_token": "plain"})).is_some());
    }

    #[test]
    fn opencode_entries_as_strings_or_objects() {
        assert_eq!(from_opencode(&json!({"zai": "k"})).unwrap().token, "k");
        assert_eq!(from_opencode(&json!({"zhipuai": {"type": "api", "key": "k2"}})), Some(Credential { token: "k2".into(), console: CHINA, source: "OpenCode" }));
        assert_eq!(from_opencode(&json!({"openai": "nope"})), None);
    }

    #[test]
    fn the_documented_reply_yields_session_weekly_mcp_in_order() {
        let v: serde_json::Value = serde_json::from_str(include_str!("../fixtures/glm_quota_limit.json")).unwrap();
        let Fetch::Ok { windows, note } = parse(&v) else { panic!("expected windows") };
        assert_eq!(windows.iter().map(|w| w.label.as_str()).collect::<Vec<_>>(), vec!["Current session", "Weekly", "MCP (1 month)"]);
        assert!((windows[0].used - 0.125).abs() < 1e-9);
        assert_eq!(windows[0].resets_at, Some(1_788_682_200_000));
        assert_eq!(windows[2].resets_at, None, "MCP has no reset and is kept anyway");
        assert_eq!(notes::render(&note), "Pro · GLM Coding Plan");

    }

    #[test]
    fn an_error_inside_http_200_is_not_a_reading() {
        assert!(matches!(parse(&json!({"code": 401, "success": false, "msg": "token expired"})), Fetch::NeedsAuth(_)));
        assert!(matches!(parse(&json!({"code": 429, "success": false})), Fetch::RateLimited(0)));
        assert!(matches!(parse(&json!({"code": 500, "success": false})), Fetch::Other(_)));
        assert!(matches!(parse(&json!({"code": 200, "success": true, "data": {"limits": []}})), Fetch::Nothing(_)));
    }

    #[test]
    fn credit_plans_are_identified_by_window_length_not_meter() {
        let v = json!({"success": true, "data": {"limits": [{"type": "CREDIT_LIMIT", "unit": 3, "number": 5, "percentage": 50}]}});
        let Fetch::Ok { windows, .. } = parse(&v) else { panic!() };
        assert_eq!(windows[0].id, "session");
    }
}
