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
    /// How this provider's working/waiting state is known when nothing has been pushed for it
    fn activity(&self, hooks_installed: bool) -> ActivitySupport {
        let _ = hooks_installed;
        ActivitySupport::Inferred
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

/// Order = top to bottom in the pill
pub static REGISTRY: &[&dyn Provider] = &[&Claude, &Codex, &Cursor, &Antigravity];

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

#[derive(Debug, Clone, Serialize)]
pub struct ProviderSnapshot {
    pub id: String,
    pub name: String,
    pub glyph: String,
    pub capabilities: Capabilities,
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
pub fn list(slots: &Slots, activity: &[Activity], hooks_installed: bool) -> Vec<ProviderSnapshot> {
    let mut out: Vec<ProviderSnapshot> = Vec::new();
    for p in REGISTRY {
        let usage = slots.read(p.id());
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
                usage: true,
                activity: if pushed_by_event(activity, p.id()) { ActivitySupport::Event } else { p.activity(hooks_installed) },
            },
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
            usage: UsageSnapshot { status: "none".into(), ..Default::default() },
        });
    }
    out
}

pub fn current(app: &AppHandle) -> Vec<ProviderSnapshot> {
    let st = app.state::<AppState>();
    let activity = st.activity.lock().unwrap_or_else(|e| e.into_inner()).clone();
    list(&st.usage, &activity, crate::hooks_install::is_installed())
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
        list(slots, activity, false)
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
        assert_eq!(ids, vec!["claude", "codex", "cursor", "gemini"]);
    }

    #[test]
    fn a_failed_provider_is_still_listed_with_its_status() {
        let slots = Slots::from(vec![("claude", snap("ok")), ("codex", snap("error")), ("cursor", snap("needsAuth")), ("gemini", snap("absent"))]);
        let l = list3(&slots, &[]);
        assert_eq!(l.iter().map(|p| p.usage.status.as_str()).collect::<Vec<_>>(), vec!["ok", "error", "needsAuth"]);
    }

    #[test]
    fn activity_only_providers_are_appended_once_without_usage() {
        let l = list3(&all("absent"), &[act("copilot"), act("copilot"), act("glm")]);
        let ids: Vec<&str> = l.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["claude", "copilot", "glm"]);
        let c = &l[1];
        assert_eq!(c.name, "Copilot");
        assert_eq!(c.glyph, "C");
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
        let without = list(&all("ok"), &[], false);
        let with = list(&all("ok"), &[], true);
        assert_eq!(without[0].capabilities.activity, ActivitySupport::Inferred);
        assert_eq!(with[0].capabilities.activity, ActivitySupport::Event);
    }

    #[test]
    fn probed_providers_are_inferred_until_they_push() {
        let probed = Activity { pushed: false, ..act("codex") };
        let l = list(&all("ok"), &[probed], true);
        assert_eq!(caps(&l)[1], ("codex".into(), ActivitySupport::Inferred));
        let l = list(&all("ok"), &[act("codex")], true);
        assert_eq!(caps(&l)[1], ("codex".into(), ActivitySupport::Event));
    }

    #[test]
    fn activity_only_providers_are_event_driven() {
        let l = list(&all("absent"), &[act("copilot")], false);
        assert_eq!(caps(&l)[1], ("copilot".into(), ActivitySupport::Event));
    }

    #[test]
    fn activity_support_serializes_snake_case() {
        assert_eq!(serde_json::to_string(&ActivitySupport::NotSupported).unwrap(), "\"not_supported\"");
    }

    #[test]
    fn find_resolves_page_urls() {
        assert_eq!(find("cursor").unwrap().page_url(), "https://cursor.com/dashboard");
        assert!(find("nope").is_none());
    }
}
