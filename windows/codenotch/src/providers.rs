//! Provider registry (W-06). Every usage provider is one `Provider` implementation listed in
//! `REGISTRY`; the app state holds one snapshot slot per registered id, and the page receives a single
//! `Vec<ProviderSnapshot>` (event `providers`, command `get_providers`) and renders whatever is in it.
//!
//! Adding a provider with usage = one module with `load_persisted` / `start` / `request_refresh`, one
//! `Provider` impl here, and one line in `REGISTRY`. Providers that only push activity through
//! `codenotch-hook --provider <id>` need no code at all: `list` appends them from the activity rows.

use crate::activity::Activity;
use crate::usage::UsageSnapshot;
use crate::AppState;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

pub trait Provider: Sync {
    /// Stable id: the glyph key, the activity `provider` value, and the key the page hovers by
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    /// Fallback letters for the ring when no glyph is available
    fn glyph(&self) -> &'static str;
    /// Opened when the user clicks the provider's cell
    fn page_url(&self) -> &'static str;
    /// Shown even when the tool is not installed (Claude: the notch's primary provider)
    fn always_shown(&self) -> bool {
        false
    }
    /// Has a quota to read (false for a local runtime: the card lists what it runs instead)
    fn has_usage(&self) -> bool {
        true
    }
    /// How this provider's working/waiting state is known when nothing has been pushed for it
    fn activity(&self, hooks_installed: bool) -> ActivitySupport {
        let _ = hooks_installed;
        ActivitySupport::Inferred
    }
    /// How old a reading may be before it counts as stale: about two missed polls of its source
    fn fresh_ms(&self, u: &UsageSnapshot) -> u64 {
        let _ = u;
        DEFAULT_FRESH_MS
    }
    fn load_persisted(&self) -> UsageSnapshot;
    fn start(&self, app: AppHandle);
    fn request_refresh(&self);
}

struct Claude;
struct Codex;
struct Cursor;
struct Antigravity;

impl Provider for Claude {
    fn id(&self) -> &'static str { "claude" }
    fn name(&self) -> &'static str { "Claude" }
    fn glyph(&self) -> &'static str { "C" }
    fn page_url(&self) -> &'static str { "https://claude.ai/settings/usage" }
    fn always_shown(&self) -> bool { true }
    /// Hooks report every state change; without them the transcript watcher and IO sampling guess
    fn activity(&self, hooks_installed: bool) -> ActivitySupport {
        if hooks_installed { ActivitySupport::Event } else { ActivitySupport::Inferred }
    }
    /// Claude Desktop samples every 15 min, so its readings stay current for longer
    fn fresh_ms(&self, u: &UsageSnapshot) -> u64 {
        if u.source == "desktop" { crate::claude_desktop::FRESH_MS } else { DEFAULT_FRESH_MS }
    }
    fn load_persisted(&self) -> UsageSnapshot { crate::usage::load_persisted() }
    fn start(&self, app: AppHandle) {
        crate::usage::start(app.clone());
        crate::claude_desktop::start(app.clone());
        crate::claude_refresh::start(app);
    }
    fn request_refresh(&self) { crate::usage::request_refresh() }
}

impl Provider for Codex {
    fn id(&self) -> &'static str { "codex" }
    fn name(&self) -> &'static str { "Codex" }
    fn glyph(&self) -> &'static str { "Cx" }
    fn page_url(&self) -> &'static str { "https://chatgpt.com/#settings/Account" }
    fn load_persisted(&self) -> UsageSnapshot { crate::codex::load_persisted() }
    fn start(&self, app: AppHandle) { crate::codex::start(app) }
    fn request_refresh(&self) { crate::codex::request_refresh() }
}

impl Provider for Cursor {
    fn id(&self) -> &'static str { "cursor" }
    fn name(&self) -> &'static str { "Cursor" }
    fn glyph(&self) -> &'static str { "Cu" }
    fn page_url(&self) -> &'static str { "https://cursor.com/dashboard" }
    fn load_persisted(&self) -> UsageSnapshot { crate::cursor::load_persisted() }
    fn start(&self, app: AppHandle) { crate::cursor::start(app) }
    fn request_refresh(&self) { crate::cursor::request_refresh() }
}

/// Id stays "gemini": glyph overrides and pushed events already use it
impl Provider for Antigravity {
    fn id(&self) -> &'static str { "gemini" }
    fn name(&self) -> &'static str { "Antigravity" }
    fn glyph(&self) -> &'static str { "Ag" }
    fn page_url(&self) -> &'static str { "https://antigravity.google" }
    fn load_persisted(&self) -> UsageSnapshot { crate::antigravity::load_persisted() }
    fn start(&self, app: AppHandle) { crate::antigravity::start(app) }
    fn request_refresh(&self) { crate::antigravity::request_refresh() }
}

/// A provider described by data: the adapters ported in W-07 need nothing more than this
pub struct Simple {
    pub id: &'static str,
    pub name: &'static str,
    pub glyph: &'static str,
    pub page_url: &'static str,
    pub has_usage: bool,
    pub activity: ActivitySupport,
    pub load: fn() -> UsageSnapshot,
    pub start: fn(AppHandle),
    pub refresh: fn(),
}

impl Provider for Simple {
    fn id(&self) -> &'static str { self.id }
    fn name(&self) -> &'static str { self.name }
    fn glyph(&self) -> &'static str { self.glyph }
    fn page_url(&self) -> &'static str { self.page_url }
    fn has_usage(&self) -> bool { self.has_usage }
    fn activity(&self, _hooks_installed: bool) -> ActivitySupport { self.activity }
    fn load_persisted(&self) -> UsageSnapshot { (self.load)() }
    fn start(&self, app: AppHandle) { (self.start)(app) }
    fn request_refresh(&self) { (self.refresh)() }
}

/// Tokens counted from Gemini CLI / OpenCode / Hermes logs; no network, no key (gemini_api.rs).
/// None of the three tools reports working state.
static GEMINI_API: Simple = Simple {
    id: crate::gemini_api::ID, name: "Gemini API", glyph: "G", page_url: "https://aistudio.google.com/usage",
    has_usage: true, activity: ActivitySupport::NotSupported,
    load: crate::gemini_api::load_persisted, start: crate::gemini_api::start, refresh: crate::gemini_api::request_refresh,
};

/// The local Ollama server's loaded models (`/api/ps`, loopback only); no quota (ollama_local.rs).
/// A loaded model is not a running request.
static OLLAMA_LOCAL: Simple = Simple {
    id: crate::ollama_local::ID, name: "Ollama", glyph: "Ol", page_url: "https://ollama.com/settings",
    has_usage: false, activity: ActivitySupport::NotSupported,
    load: crate::ollama_local::load_persisted, start: crate::ollama_local::start, refresh: crate::ollama_local::request_refresh,
};

/// Z.ai GLM Coding Plan monitor with a key another tool holds (glm.rs). Unofficial endpoint.
static GLM: Simple = Simple {
    id: crate::glm::ID, name: "GLM", glyph: "Z", page_url: "https://z.ai/manage-apikey/subscription",
    has_usage: true, activity: ActivitySupport::NotSupported,
    load: crate::glm::load_persisted, start: crate::glm::start, refresh: crate::glm::request_refresh,
};

/// ollama.com/api/usage with the user's own key (env or Credential Manager) (ollama_cloud.rs)
static OLLAMA_CLOUD: Simple = Simple {
    id: crate::ollama_cloud::ID, name: "Ollama Cloud", glyph: "Ol", page_url: "https://ollama.com/settings",
    has_usage: true, activity: ActivitySupport::NotSupported,
    load: crate::ollama_cloud::load_persisted, start: crate::ollama_cloud::start, refresh: crate::ollama_cloud::request_refresh,
};

/// opencode.ai Go plan windows with the key OpenCode's sign-in stores (opencode.rs)
static OPENCODE: Simple = Simple {
    id: crate::opencode::ID, name: "OpenCode Go", glyph: "Oc", page_url: "https://opencode.ai/auth",
    has_usage: true, activity: ActivitySupport::NotSupported,
    load: crate::opencode::load_persisted, start: crate::opencode::start, refresh: crate::opencode::request_refresh,
};

/// api.github.com/copilot_internal/user with the GitHub CLI token (copilot.rs). Internal endpoint.
static COPILOT: Simple = Simple {
    id: crate::copilot::ID, name: "GitHub Copilot", glyph: "Gh", page_url: "https://github.com/settings/copilot",
    has_usage: true, activity: ActivitySupport::NotSupported,
    load: crate::copilot::load_persisted, start: crate::copilot::start, refresh: crate::copilot::request_refresh,
};

/// Grok CLI's billing proxy with the xAI session it stores (grok.rs). Private CLI endpoint.
static GROK: Simple = Simple {
    id: crate::grok::ID, name: "Grok", glyph: "X", page_url: "https://grok.com/?_s=usage",
    has_usage: true, activity: ActivitySupport::NotSupported,
    load: crate::grok::load_persisted, start: crate::grok::start, refresh: crate::grok::request_refresh,
};

/// Command Code's /alpha billing documents with the desktop app's key (commandcode.rs). Private endpoints.
static COMMANDCODE: Simple = Simple {
    id: crate::commandcode::ID, name: "Command Code", glyph: "Cc", page_url: "https://commandcode.ai",
    has_usage: true, activity: ActivitySupport::NotSupported,
    load: crate::commandcode::load_persisted, start: crate::commandcode::start, refresh: crate::commandcode::request_refresh,
};

/// perplexity.ai's rate-limit endpoint, read inside a Codenotch WebView the user signs into (perplexity.rs)
static PERPLEXITY: Simple = Simple {
    id: crate::perplexity::ID, name: "Perplexity", glyph: "P", page_url: "https://www.perplexity.ai/settings/account",
    has_usage: true, activity: ActivitySupport::NotSupported,
    load: crate::perplexity::load_persisted, start: crate::perplexity::start, refresh: crate::perplexity::request_refresh,
};

/// The API pollers read every 5 min while idle; two missed polls plus slack
const DEFAULT_FRESH_MS: u64 = 11 * 60_000;

/// Order = top to bottom in the pill
pub static REGISTRY: &[&dyn Provider] = &[&Claude, &Codex, &Cursor, &Antigravity, &GEMINI_API, &OLLAMA_LOCAL, &OLLAMA_CLOUD, &GLM, &OPENCODE, &COPILOT, &GROK, &COMMANDCODE, &PERPLEXITY];

pub fn find(id: &str) -> Option<&'static dyn Provider> {
    REGISTRY.iter().copied().find(|p| p.id() == id)
}

/// One snapshot slot per registered provider. Each slot has its own lock, so a provider thread that
/// panics while holding it poisons only its own slot, never the others.
pub struct Slots(Vec<(&'static str, Mutex<UsageSnapshot>)>);

impl Slots {
    pub fn load() -> Self {
        Slots(REGISTRY.iter().map(|p| (p.id(), Mutex::new(p.load_persisted()))).collect())
    }

    #[cfg(test)]
    pub fn from(v: Vec<(&'static str, UsageSnapshot)>) -> Self {
        Slots(v.into_iter().map(|(id, s)| (id, Mutex::new(s))).collect())
    }

    /// Panics on an unregistered id: ids are compile-time constants, so that is a programming error
    pub fn get(&self, id: &str) -> &Mutex<UsageSnapshot> {
        &self.0.iter().find(|(k, _)| *k == id).unwrap_or_else(|| panic!("unregistered provider {id}")).1
    }

    fn read(&self, id: &str) -> UsageSnapshot {
        self.0
            .iter()
            .find(|(k, _)| *k == id)
            .map(|(_, m)| m.lock().unwrap_or_else(|e| e.into_inner()).clone())
            .unwrap_or_default()
    }
}

/// Where a provider's working / waiting state comes from (W-08). The card says which, so "idle"
/// is never confused with "cannot tell".
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ActivitySupport {
    /// The tool reports each change (Claude Code hooks, or `codenotch-hook --provider`)
    Event,
    /// Guessed from local files or process activity; can lag or miss a run
    Inferred,
    /// No way to see it on this platform
    NotSupported,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Capabilities {
    /// Codenotch can read this provider's usage windows
    pub usage: bool,
    pub activity: ActivitySupport,
}

/// Whether the numbers on screen are current (W-12). Decided here, per provider, instead of by
/// fixed windows in the page.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    /// Read successfully within the source's fresh window
    Live,
    /// Older than that, or the last refresh failed for a reason other than the network
    Stale,
    /// The last refresh could not reach the network; the reading shown is the last one that did
    Offline,
    /// Refresh failed and there is no reading at all
    Error,
    NeedsAuth,
    /// Nothing to show yet (not read, or nothing metered)
    NoData,
}

pub fn freshness(u: &UsageSnapshot, now: u64, fresh_ms: u64) -> Freshness {
    if u.status == "needsAuth" {
        return Freshness::NeedsAuth;
    }
    if u.windows.is_empty() {
        return if u.offline {
            Freshness::Offline
        } else if u.status == "error" {
            Freshness::Error
        } else {
            Freshness::NoData
        };
    }
    if u.status == "ok" && u.fetched_at > 0 && now.saturating_sub(u.fetched_at) <= fresh_ms {
        return Freshness::Live;
    }
    if u.offline { Freshness::Offline } else { Freshness::Stale }
}

/// Windows whose reset time passed after the reading was taken: the percentage is from a window
/// that is over, so the page must not show it as current usage.
fn mark_expired(u: &mut UsageSnapshot, now: u64) {
    let fetched = u.fetched_at;
    for w in &mut u.windows {
        w.expired = matches!(w.resets_at, Some(r) if r <= now && fetched < r);
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderSnapshot {
    pub id: String,
    pub name: String,
    pub glyph: String,
    pub capabilities: Capabilities,
    pub freshness: Freshness,
    /// ms since the reading was taken (0 = never read)
    pub age_ms: u64,
    pub usage: UsageSnapshot,
}

fn title_case(id: &str) -> String {
    let mut c = id.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// The cells the notch shows, in order: registered providers that are installed (or always shown),
/// then any provider that only reports activity through the event ingress.
pub fn list(slots: &Slots, activity: &[Activity], hooks_installed: bool, now: u64) -> Vec<ProviderSnapshot> {
    let mut out: Vec<ProviderSnapshot> = Vec::new();
    for p in REGISTRY {
        let mut usage = slots.read(p.id());
        mark_expired(&mut usage, now);
        // A pushed event proves the tool is there even when detection missed it
        let pushed = activity.iter().any(|a| a.provider == p.id());
        if !p.always_shown() && usage.status == "absent" && !pushed {
            continue;
        }
        out.push(ProviderSnapshot {
            id: p.id().into(),
            name: p.name().into(),
            glyph: p.glyph().into(),
            // A pushed row means the tool is wired to the hook right now: that beats any probe
            capabilities: Capabilities {
                usage: p.has_usage(),
                activity: if pushed_by_event(activity, p.id()) { ActivitySupport::Event } else { p.activity(hooks_installed) },
            },
            freshness: freshness(&usage, now, p.fresh_ms(&usage)),
            age_ms: if usage.fetched_at > 0 { now.saturating_sub(usage.fetched_at) } else { 0 },
            usage,
        });
    }
    let mut known: HashSet<String> = out.iter().map(|p| p.id.clone()).collect();
    for a in activity {
        if find(&a.provider).is_some() || !known.insert(a.provider.clone()) {
            continue;
        }
        out.push(ProviderSnapshot {
            id: a.provider.clone(),
            name: title_case(&a.provider),
            glyph: a.provider.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into()),
            capabilities: Capabilities { usage: false, activity: ActivitySupport::Event },
            freshness: Freshness::NoData,
            age_ms: 0,
            usage: UsageSnapshot { status: "none".into(), ..Default::default() },
        });
    }
    out
}

/// Stable sort by position in the user's saved order; ids it does not name keep their place after
pub fn sort_by_order<T>(items: &mut [T], order: &[String], key: impl Fn(&T) -> &str) {
    items.sort_by_key(|x| order.iter().position(|o| o == key(x)).unwrap_or(usize::MAX));
}

/// The user's layout (W-11): switched-off providers leave the notch, the rest follow the saved order
pub fn arrange(mut l: Vec<ProviderSnapshot>, order: &[String], disabled: &[String]) -> Vec<ProviderSnapshot> {
    l.retain(|p| !disabled.contains(&p.id));
    sort_by_order(&mut l, order, |p| p.id.as_str());
    l
}

pub fn current(app: &AppHandle) -> Vec<ProviderSnapshot> {
    let st = app.state::<AppState>();
    let activity = st.activity.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let (order, disabled) = {
        let c = st.cfg.lock().unwrap_or_else(|e| e.into_inner());
        (c.provider_order.clone(), c.disabled.clone())
    };
    arrange(list(&st.usage, &activity, crate::hooks_install::is_installed(), crate::usage::now_ms()), &order, &disabled)
}

/// Rows that came through the event ingress (probe rows are tagged by `activity::is_pushed`)
fn pushed_by_event(activity: &[Activity], id: &str) -> bool {
    activity.iter().any(|a| a.provider == id && a.pushed)
}

/// Push the whole list to the page. Called whenever any provider's snapshot or the activity changes.
pub fn publish(app: &AppHandle) {
    let _ = app.emit("providers", current(app));
}

pub fn start_all(app: &AppHandle) {
    for p in REGISTRY {
        p.start(app.clone());
    }
}

pub fn refresh_all() {
    for p in REGISTRY {
        p.request_refresh();
    }
}

const CLOCK_TICK_SECS: u64 = 15;
const REPUBLISH_EVERY_TICKS: u32 = 2;

/// The wall clock moved much further than the thread slept: the machine was asleep (or hibernated)
pub(crate) fn resumed(before_ms: u64, after_ms: u64, slept_ms: u64) -> bool {
    after_ms.saturating_sub(before_ms) > slept_ms + 60_000
}

/// Freshness moves with time, not only with new readings: republish every 30 s so "live" turns
/// "stale" on schedule. After sleep, every provider is refreshed at once instead of at its next poll.
pub fn start_clock(app: AppHandle) {
    std::thread::spawn(move || {
        let mut tick: u32 = 0;
        loop {
            let before = crate::usage::now_ms();
            std::thread::sleep(std::time::Duration::from_secs(CLOCK_TICK_SECS));
            let after = crate::usage::now_ms();
            if resumed(before, after, CLOCK_TICK_SECS * 1000) {
                crate::applog(&format!("resume detected ({} s gap): refreshing every provider", (after - before) / 1000));
                refresh_all();
                publish(&app);
            }
            tick = tick.wrapping_add(1);
            if tick % REPUBLISH_EVERY_TICKS == 0 {
                publish(&app);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(status: &str) -> UsageSnapshot {
        UsageSnapshot { status: status.into(), ..Default::default() }
    }

    fn act(provider: &str) -> Activity {
        Activity { provider: provider.into(), state: "busy".into(), name: String::new(), detail: String::new(), since: 0, pushed: true }
    }

    fn list3(slots: &Slots, activity: &[Activity]) -> Vec<ProviderSnapshot> {
        list(slots, activity, false, NOW)
    }

    const NOW: u64 = 1_800_000_000_000;
    const MIN: u64 = 60_000;

    fn reading(status: &str, age_min: u64) -> UsageSnapshot {
        UsageSnapshot {
            status: status.into(),
            fetched_at: NOW - age_min * MIN,
            windows: vec![crate::usage::LimitWindow { id: "w".into(), used: 0.5, resets_at: Some(NOW + 60 * MIN), ..Default::default() }],
            ..Default::default()
        }
    }

    #[test]
    fn freshness_follows_the_source_window() {
        assert_eq!(freshness(&reading("ok", 3), NOW, DEFAULT_FRESH_MS), Freshness::Live);
        assert_eq!(freshness(&reading("ok", 20), NOW, DEFAULT_FRESH_MS), Freshness::Stale);
        assert_eq!(freshness(&reading("stale", 1), NOW, DEFAULT_FRESH_MS), Freshness::Stale);
        // Claude Desktop's 15-minute cadence: a 20-minute-old desktop sample is still current
        let mut d = reading("ok", 20);
        d.source = "desktop".into();
        assert_eq!(freshness(&d, NOW, find("claude").unwrap().fresh_ms(&d)), Freshness::Live);
    }

    #[test]
    fn offline_error_and_auth_are_told_apart() {
        let mut off = reading("stale", 30);
        off.offline = true;
        assert_eq!(freshness(&off, NOW, DEFAULT_FRESH_MS), Freshness::Offline);
        let mut off_empty = snap("error");
        off_empty.offline = true;
        assert_eq!(freshness(&off_empty, NOW, DEFAULT_FRESH_MS), Freshness::Offline);
        assert_eq!(freshness(&snap("error"), NOW, DEFAULT_FRESH_MS), Freshness::Error);
        assert_eq!(freshness(&snap("needsAuth"), NOW, DEFAULT_FRESH_MS), Freshness::NeedsAuth);
        assert_eq!(freshness(&snap("none"), NOW, DEFAULT_FRESH_MS), Freshness::NoData);
        // A current local reading (Codex's rollout) is live even if the network call failed
        let mut local = reading("ok", 2);
        local.offline = true;
        assert_eq!(freshness(&local, NOW, DEFAULT_FRESH_MS), Freshness::Live);
    }

    #[test]
    fn a_window_that_reset_after_the_reading_is_expired() {
        let mut u = reading("stale", 120);
        u.windows[0].resets_at = Some(NOW - 30 * MIN);
        u.windows.push(crate::usage::LimitWindow { id: "future".into(), resets_at: Some(NOW + MIN), ..Default::default() });
        let slots = Slots::from(vec![("claude", u)]);
        let l = list3(&slots, &[]);
        assert!(l[0].usage.windows[0].expired);
        assert!(!l[0].usage.windows[1].expired);
        assert_eq!(l[0].age_ms, 120 * MIN);
    }

    #[test]
    fn a_reading_taken_after_the_reset_is_not_expired() {
        let mut u = reading("ok", 1);
        u.windows[0].resets_at = Some(NOW - 5 * MIN);
        let slots = Slots::from(vec![("claude", u)]);
        assert!(!list3(&slots, &[])[0].usage.windows[0].expired);
    }

    #[test]
    fn resume_is_a_wall_clock_jump_past_the_sleep() {
        assert!(!resumed(0, 15_000, 15_000));
        assert!(!resumed(0, 70_000, 15_000));
        assert!(resumed(0, 20 * MIN, 15_000));
    }

    fn all(status: &str) -> Slots {
        Slots::from(REGISTRY.iter().map(|p| (p.id(), snap(status))).collect())
    }

    #[test]
    fn registry_ids_are_unique_and_pass_the_server_validation() {
        let mut seen = HashSet::new();
        for p in REGISTRY {
            assert!(seen.insert(p.id()), "duplicate id {}", p.id());
            assert!(crate::server::valid_provider_id(p.id()), "{} would be refused by the event server", p.id());
            assert!(p.page_url().starts_with("https://"));
        }
    }

    #[test]
    fn claude_is_shown_even_when_absent_the_others_are_not() {
        let ids: Vec<String> = list3(&all("absent"), &[]).into_iter().map(|p| p.id).collect();
        assert_eq!(ids, vec!["claude"]);
    }

    #[test]
    fn installed_providers_keep_registry_order() {
        let ids: Vec<String> = list3(&all("ok"), &[]).into_iter().map(|p| p.id).collect();
        assert_eq!(ids, vec!["claude", "codex", "cursor", "gemini", "gemini-api", "ollama", "ollama-cloud", "glm", "opencode", "copilot", "grok", "commandcode", "perplexity"]);
    }

    #[test]
    fn a_failed_provider_is_still_listed_with_its_status() {
        let slots = Slots::from(vec![("claude", snap("ok")), ("codex", snap("error")), ("cursor", snap("needsAuth")), ("gemini", snap("absent")), ("gemini-api", snap("absent")), ("ollama", snap("absent")), ("ollama-cloud", snap("absent")), ("glm", snap("absent")), ("opencode", snap("absent")), ("copilot", snap("absent")), ("grok", snap("absent")), ("commandcode", snap("absent")), ("perplexity", snap("absent"))]);
        let l = list3(&slots, &[]);
        assert_eq!(l.iter().map(|p| p.usage.status.as_str()).collect::<Vec<_>>(), vec!["ok", "error", "needsAuth"]);
    }

    #[test]
    fn activity_only_providers_are_appended_once_without_usage() {
        let l = list3(&all("absent"), &[act("kilo"), act("kilo"), act("t_other")]);
        let ids: Vec<&str> = l.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["claude", "kilo", "t_other"]);
        let c = &l[1];
        assert_eq!(c.name, "Kilo");
        assert_eq!(c.glyph, "K");
        assert!(!c.capabilities.usage);
        assert_eq!(c.usage.status, "none");
    }

    #[test]
    fn a_registered_provider_that_pushed_activity_is_shown_under_its_own_name() {
        let l = list3(&all("absent"), &[act("codex")]);
        assert_eq!(l.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), vec!["Claude", "Codex"]);
        assert!(l[1].capabilities.usage);
    }

    fn caps(l: &[ProviderSnapshot]) -> Vec<(String, ActivitySupport)> {
        l.iter().map(|p| (p.id.clone(), p.capabilities.activity)).collect()
    }

    #[test]
    fn claude_activity_is_event_only_with_hooks() {
        let without = list(&all("ok"), &[], false, NOW);
        let with = list(&all("ok"), &[], true, NOW);
        assert_eq!(without[0].capabilities.activity, ActivitySupport::Inferred);
        assert_eq!(with[0].capabilities.activity, ActivitySupport::Event);
    }

    #[test]
    fn probed_providers_are_inferred_until_they_push() {
        let probed = Activity { pushed: false, ..act("codex") };
        let l = list(&all("ok"), &[probed], true, NOW);
        assert_eq!(caps(&l)[1], ("codex".into(), ActivitySupport::Inferred));
        let l = list(&all("ok"), &[act("codex")], true, NOW);
        assert_eq!(caps(&l)[1], ("codex".into(), ActivitySupport::Event));
    }

    #[test]
    fn activity_only_providers_are_event_driven() {
        let l = list(&all("absent"), &[act("kilo")], false, NOW);
        assert_eq!(caps(&l)[1], ("kilo".into(), ActivitySupport::Event));
    }

    #[test]
    fn activity_support_serializes_snake_case() {
        assert_eq!(serde_json::to_string(&ActivitySupport::NotSupported).unwrap(), "\"not_supported\"");
    }

    #[test]
    fn a_local_runtime_declares_no_usage() {
        let l = list3(&all("ok"), &[]);
        let o = l.iter().find(|p| p.id == "ollama").unwrap();
        assert!(!o.capabilities.usage);
        assert_eq!(o.capabilities.activity, ActivitySupport::NotSupported);
    }

    #[test]
    fn the_saved_layout_orders_and_hides_cells() {
        let l = list3(&all("ok"), &[act("kilo")]);
        let order = vec!["kilo".to_string(), "cursor".to_string()];
        let out = arrange(l, &order, &["codex".to_string()]);
        let ids: Vec<&str> = out.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(&ids[..3], &["kilo", "cursor", "claude"]);
        assert!(!ids.contains(&"codex"));
    }

    #[test]
    fn find_resolves_page_urls() {
        assert_eq!(find("cursor").unwrap().page_url(), "https://cursor.com/dashboard");
        assert!(find("nope").is_none());
    }
}
