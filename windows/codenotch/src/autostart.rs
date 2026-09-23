//! Start at sign-in: an HKCU\...\Run registry value (per user, no administrator needed).
//! The command carries --silent: wait in the background, show no bar without sessions, appear when one starts.
//! Implemented with reg.exe, so no new dependency.

use std::process::Command;

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const NAME: &str = "Codenotch";

fn reg(args: &[&str]) -> Option<(bool, String)> {
    let mut c = Command::new("reg");
    c.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c.output().ok().map(|o| {
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
        (o.status.success(), text)
    })
}

pub fn is_enabled() -> bool {
    reg(&["query", RUN_KEY, "/v", NAME])
        .map(|(ok, out)| ok && out.contains(NAME))
        .unwrap_or(false)
}

/// The exe path inside a Run value like `"C:\\...\\codenotch.exe" --silent` (quoted or bare).
pub(crate) fn exe_in_value(value: &str) -> Option<String> {
    let v = value.trim();
    if let Some(rest) = v.strip_prefix('"') {
        return rest.split('"').next().filter(|p| !p.is_empty()).map(str::to_string);
    }
    v.split(" --").next().map(str::trim).filter(|p| !p.is_empty()).map(str::to_string)
}

/// True when start-at-sign-in is on but points at another copy of the app (e.g. the old unzipped
/// v0.3.0 after moving to the installer). That copy would start at sign-in, take the event port,
/// and leave this one without Claude activity.
pub(crate) fn points_elsewhere(value: &str, current_exe: &str) -> bool {
    match exe_in_value(value) {
        Some(p) => !p.eq_ignore_ascii_case(current_exe),
        None => false,
    }
}

fn current_value() -> Option<String> {
    let (ok, out) = reg(&["query", RUN_KEY, "/v", NAME])?;
    if !ok {
        return None;
    }
    // reg.exe prints: `    Codenotch    REG_SZ    "C:\...\codenotch.exe" --silent`
    out.lines()
        .find(|l| l.contains("REG_SZ"))
        .and_then(|l| l.split("REG_SZ").nth(1))
        .map(|v| v.trim().to_string())
}

/// Keep the user's choice but follow the app to wherever it is installed now. Called once at startup.
pub fn repoint_if_moved() {
    let (Some(value), Ok(exe)) = (current_value(), std::env::current_exe()) else { return };
    if points_elsewhere(&value, &exe.display().to_string()) {
        match enable() {
            Ok(_) => crate::applog("autostart: entry pointed at another copy; updated to this install"),
            Err(e) => crate::applog(&format!("autostart: could not update the entry: {e}")),
        }
    }
}

pub fn enable() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let val = format!("\"{}\" --silent", exe.display());
    match reg(&["add", RUN_KEY, "/v", NAME, "/t", "REG_SZ", "/d", &val, "/f"]) {
        Some((true, _)) => Ok("start at sign-in enabled (silent until a session appears)".into()),
        Some((false, out)) => Err(out),
        None => Err("reg.exe failed to run".into()),
    }
}

pub fn disable() -> Result<String, String> {
    match reg(&["delete", RUN_KEY, "/v", NAME, "/f"]) {
        Some((true, _)) => Ok("start at sign-in disabled".into()),
        Some((false, out)) => {
            if out.to_lowercase().contains("unable to find") || out.contains("找不到") { // reg.exe answers in the OS language; "找不到" is the Chinese "unable to find"
                Ok("start at sign-in was not enabled".into())
            } else {
                Err(out)
            }
        }
        None => Err("reg.exe failed to run".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_quoted_and_bare_paths() {
        assert_eq!(
            exe_in_value(r#""C:\Users\a\Downloads\Codenotch\codenotch.exe" --silent"#).as_deref(),
            Some(r"C:\Users\a\Downloads\Codenotch\codenotch.exe")
        );
        assert_eq!(exe_in_value(r"C:\Apps\codenotch.exe --silent").as_deref(), Some(r"C:\Apps\codenotch.exe"));
        assert_eq!(exe_in_value("   ").as_deref(), None);
    }

    #[test]
    fn an_old_unzipped_copy_is_detected() {
        let old = r#""C:\Users\a\Downloads\Codenotch\codenotch.exe" --silent"#;
        assert!(points_elsewhere(old, r"C:\Users\a\AppData\Local\Codenotch\codenotch.exe"));
    }

    #[test]
    fn the_same_install_is_left_alone_case_insensitively() {
        let v = r#""C:\Users\a\AppData\Local\Codenotch\codenotch.exe" --silent"#;
        assert!(!points_elsewhere(v, r"c:\users\a\appdata\local\codenotch\CODENOTCH.EXE"));
        assert!(!points_elsewhere("", r"C:\x\codenotch.exe"));
    }
}
