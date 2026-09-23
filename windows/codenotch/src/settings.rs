//! Settings window and first run (W-11). One local page (`ui/settings.html`) that replaces editing
//! config.json by hand: which providers appear and in what order, keys Codenotch keeps for itself
//! (Ollama Cloud), Perplexity's in-app session, Claude Code hooks, language, hover-only, start with
//! Windows. On the very first launch (no config.json yet) it opens with a short welcome.

use crate::providers::{self, REGISTRY};
use crate::AppState;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "settings";

#[derive(Serialize, Debug, PartialEq)]
pub struct Row {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// Plain words for the status, never a raw code
    pub state: String,
    pub note: String,
    pub has_usage: bool,
}

pub fn state_words(status: &str) -> &'static str {
    match status {
        "absent" | "" => "Not found on this PC",
        "ok" => "Connected",
        "stale" => "Connected · last reading is old",
        "needsAuth" => "Sign-in needed",
        "error" => "Error — see the note",
        "none" => "Signed in · nothing metered",
        _ => "Unknown",
    }
}

/// Registered providers in the user's order, with their switch and a status in words
pub fn rows(order: &[String], disabled: &[String], status: impl Fn(&str) -> (String, String)) -> Vec<Row> {
    let mut ids: Vec<&'static str> = REGISTRY.iter().map(|p| p.id()).collect();
    providers::sort_by_order(&mut ids, order, |id| id);
    ids.into_iter()
        .map(|id| {
            let p = providers::find(id).expect("registered");
            let (st, note) = status(id);
            // Absent means something different for the providers that need the user to act here
            let state = match (id, st.as_str()) {
                (crate::perplexity::ID, "absent") => "Not connected — see Signed-in sessions",
                (crate::ollama_cloud::ID, "absent") => "No API key — see Keys",
                _ => state_words(&st),
            };
            Row { id: id.into(), name: p.name().into(), enabled: !disabled.iter().any(|d| d == id), state: state.into(), note, has_usage: p.has_usage() }
        })
        .collect()
}

/// Move `id` one place up (delta -1) or down (+1) in the full order; returns the new order
pub fn moved(order: &[String], id: &str, delta: i32) -> Vec<String> {
    let mut ids: Vec<String> = REGISTRY.iter().map(|p| p.id().to_string()).collect();
    providers::sort_by_order(&mut ids, order, |s| s.as_str());
    if let Some(i) = ids.iter().position(|x| x == id) {
        let j = i as i32 + delta;
        if j >= 0 && (j as usize) < ids.len() {
            ids.swap(i, j as usize);
        }
    }
    ids
}

#[derive(Serialize)]
pub struct View {
    rows: Vec<Row>,
    lang: String,
    hover_only: bool,
    autostart: bool,
    hooks_installed: bool,
    perplexity: bool,
    ollama_key: bool,
    first_run: bool,
    version: &'static str,
    update_check: bool,
    update_available: Option<String>,
    update_status: String,
}

static FIRST_RUN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn mark_first_run() {
    FIRST_RUN.store(true, std::sync::atomic::Ordering::Relaxed);
}

fn view(app: &AppHandle) -> View {
    let st = app.state::<AppState>();
    let (order, disabled, lang, hover_only, perplexity, update_check) = {
        let c = st.cfg.lock().unwrap_or_else(|e| e.into_inner());
        (c.provider_order.clone(), c.disabled.clone(), c.lang.clone(), c.hover_only, c.perplexity, c.update_check)
    };
    let rows = rows(&order, &disabled, |id| {
        let u = st.usage.get(id).lock().unwrap_or_else(|e| e.into_inner()).clone();
        (u.status, u.note)
    });
    View {
        rows,
        lang,
        hover_only,
        autostart: crate::autostart::is_enabled(),
        hooks_installed: crate::hooks_install::is_installed(),
        perplexity,
        ollama_key: crate::remote::cred_read(crate::ollama_cloud::CRED_TARGET).is_some(),
        first_run: FIRST_RUN.load(std::sync::atomic::Ordering::Relaxed),
        version: env!("CARGO_PKG_VERSION"),
        update_check,
        update_available: crate::updates::pending_version(),
        update_status: crate::updates::last_result(),
    }
}

/// Re-render the settings page and the notch after a change
fn changed(app: &AppHandle) {
    let _ = app.emit_to(LABEL, "settings", ());
    providers::publish(app);
}

pub fn open(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        return;
    }
    let a = app.clone();
    // Built off the calling thread: a tray handler or setup must not wait on WebView2
    std::thread::spawn(move || {
        let r = WebviewWindowBuilder::new(&a, LABEL, WebviewUrl::App("settings.html".into()))
            .title("Codenotch settings")
            .inner_size(560.0, 700.0)
            .min_inner_size(460.0, 480.0)
            .build();
        if let Err(e) = r {
            crate::applog(&format!("settings window failed: {e}"));
        }
    });
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> View {
    view(&app)
}

#[tauri::command]
pub fn set_provider_enabled(app: AppHandle, id: String, enabled: bool) {
    if providers::find(&id).is_none() {
        return;
    }
    {
        let st = app.state::<AppState>();
        let mut c = st.cfg.lock().unwrap_or_else(|e| e.into_inner());
        c.disabled.retain(|d| d != &id);
        if !enabled {
            c.disabled.push(id);
        }
        crate::config::save(&c);
    }
    changed(&app);
}

#[tauri::command]
pub fn move_provider(app: AppHandle, id: String, delta: i32) {
    {
        let st = app.state::<AppState>();
        let mut c = st.cfg.lock().unwrap_or_else(|e| e.into_inner());
        c.provider_order = moved(&c.provider_order, &id, delta.signum());
        crate::config::save(&c);
    }
    changed(&app);
}

#[tauri::command]
pub fn set_pref(app: AppHandle, key: String, value: serde_json::Value) -> Result<(), String> {
    match key.as_str() {
        "hover_only" => {
            let v = value.as_bool().ok_or("expected true/false")?;
            {
                let st = app.state::<AppState>();
                let mut c = st.cfg.lock().unwrap_or_else(|e| e.into_inner());
                c.hover_only = v;
                crate::config::save(&c);
            }
            let _ = app.emit("prefs", serde_json::json!({ "hover_only": v }));
        }
        "autostart" => {
            let on = value.as_bool().ok_or("expected true/false")?;
            if on { crate::autostart::enable() } else { crate::autostart::disable() }?;
        }
        "update_check" => {
            let v = value.as_bool().ok_or("expected true/false")?;
            let st = app.state::<AppState>();
            let mut c = st.cfg.lock().unwrap_or_else(|e| e.into_inner());
            c.update_check = v;
            crate::config::save(&c);
        }
        "lang" => {
            let l = value.as_str().ok_or("expected a language")?;
            if !["auto", "en", "zh", "ja", "ko"].contains(&l) {
                return Err("unknown language".into());
            }
            crate::apply_lang(&app, l);
        }
        _ => return Err("unknown setting".into()),
    }
    crate::tray::refresh_menu(&app);
    changed(&app);
    Ok(())
}

#[tauri::command]
pub fn set_hooks(app: AppHandle, install: bool) -> Result<String, String> {
    let r = if install { crate::hooks_install::install() } else { crate::hooks_install::uninstall() };
    changed(&app);
    r
}

/// Stored in Windows Credential Manager, never in config.json; the key is not sent back to the page
#[tauri::command]
pub fn set_ollama_key(app: AppHandle, key: Option<String>) -> Result<(), String> {
    match key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty()) {
        Some(k) => crate::remote::cred_write(crate::ollama_cloud::CRED_TARGET, &k)?,
        None => crate::remote::cred_delete(crate::ollama_cloud::CRED_TARGET)?,
    }
    crate::ollama_cloud::request_refresh();
    changed(&app);
    Ok(())
}

#[tauri::command]
pub fn set_perplexity(app: AppHandle, connect: bool) {
    if connect {
        crate::perplexity::open_signin(&app);
    } else {
        crate::perplexity::disconnect(&app);
    }
    crate::tray::refresh_menu(&app);
    changed(&app);
}

#[tauri::command]
pub fn check_updates() {
    crate::updates::request_check();
}

#[tauri::command]
pub fn install_update(app: AppHandle) {
    crate::updates::install(&app);
}

#[tauri::command]
pub fn finish_welcome(app: AppHandle) {
    FIRST_RUN.store(false, std::sync::atomic::Ordering::Relaxed);
    changed(&app);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(r: &[Row]) -> Vec<&str> {
        r.iter().map(|x| x.id.as_str()).collect()
    }

    #[test]
    fn rows_follow_the_saved_order_then_the_registry() {
        let order = vec!["codex".to_string(), "gone-provider".to_string(), "claude".to_string()];
        let r = rows(&order, &[], |_| ("ok".into(), String::new()));
        assert_eq!(&ids(&r)[..3], &["codex", "claude", "cursor"]);
        assert_eq!(r.len(), REGISTRY.len());
    }

    #[test]
    fn disabled_rows_are_switched_off_and_statuses_are_words() {
        let r = rows(&[], &["cursor".into()], |id| (if id == "codex" { "needsAuth".into() } else { "absent".into() }, String::new()));
        let cursor = r.iter().find(|x| x.id == "cursor").unwrap();
        assert!(!cursor.enabled);
        assert_eq!(r.iter().find(|x| x.id == "codex").unwrap().state, "Sign-in needed");
        assert_eq!(r[0].state, "Not found on this PC");
        assert_eq!(r.iter().find(|x| x.id == "perplexity").unwrap().state, "Not connected — see Signed-in sessions");
    }

    #[test]
    fn moving_swaps_neighbours_and_stops_at_the_ends() {
        let o = moved(&[], "codex", -1);
        assert_eq!(&o[..2], &["codex", "claude"]);
        let o2 = moved(&o, "codex", -1);
        assert_eq!(o2, o, "already first");
        let last = REGISTRY.last().unwrap().id();
        assert_eq!(moved(&[], last, 1).last().map(String::as_str), Some(last));
    }
}
