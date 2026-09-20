//! Keeps Claude Code's OAuth token from ageing out. A port of upstream's `ClaudeTokenRefresher`.
//!
//! Codenotch only ever *reads* `~/.claude/.credentials.json`; the standalone `claude` command is
//! what writes it. On a machine where Claude is used through the desktop app the file is written
//! once and then rots — Desktop renews its own copy elsewhere — and eight hours later the API path
//! answers every request with a 429 and an hour-long Retry-After. Desktop's samples
//! (`claude_desktop.rs`) keep the ring current at 15-minute granularity; this brings the 30-second
//! path back.
//!
//! How it renews, and why that is a compatibility mechanism rather than an interface: running
//! `claude -p` with an empty stdin makes the command go through its whole start-up — where it checks
//! the token's age and renews it — and then exit non-zero for want of a prompt. Verified here on a
//! real machine: `expiresAt` moved from a week in the past to eight hours ahead, no transcript was
//! written, no conversation was created. None of that is promised by anyone, so the outcome is
//! judged rather than the command trusted: unless the expiry actually moved, this reports failure
//! and never retries for that token. And because no prompt is ever supplied, a version that started
//! accepting empty input could not be talked into answering one — the failure mode is a wasted
//! launch, never an invented conversation.
//!
//! Which `claude`: the one on PATH if there is one; otherwise the copy Claude Desktop bundles under
//! `%APPDATA%\Claude\claude-code\<version>\claude.exe`, newest version first. Both are the same
//! program and both renew the same file.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::AppHandle;

/// How close to expiry is close enough. Must stay under Claude Code's own five minutes: its start-up
/// renews only when now + 300 s >= expiresAt, so launching earlier is a no-op that would then be
/// (correctly) reported as a failure and stop retrying. Four minutes leaves room for a slow start.
const MARGIN_MS: u64 = 4 * 60_000;
const COOLDOWN_MS: u64 = 10 * 60_000;
const TIMEOUT: Duration = Duration::from_secs(30);
const INTERVAL: Duration = Duration::from_secs(60);

/// The pid of the launch in flight, or 0. The CLI registers a session of its own for the second it
/// lives, and its SessionStart hook reports that pid; the event server drops those events so the
/// renewal never shows up in the notch as work.
pub static IGNORED_PID: AtomicU32 = AtomicU32::new(0);

/// Last outcome, for doctor and the run log
static LAST: Mutex<String> = Mutex::new(String::new());

fn now_ms() -> u64 {
    crate::usage::now_ms()
}

/// `claude` on PATH (exe, cmd or bat), else Claude Desktop's bundled copy, else None.
pub fn cli_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for name in ["claude.exe", "claude.cmd", "claude.bat"] {
                let p = dir.join(name);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    let root = dirs::config_dir()?.join("Claude").join("claude-code");
    let mut versions: Vec<PathBuf> = std::fs::read_dir(&root)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("claude.exe").is_file())
        .collect();
    // Version directories sort lexically well enough ("2.1.271" > "2.1.9" is wrong, but a Desktop
    // update removes the old directory anyway); newest last, so take the last
    versions.sort();
    versions.pop().map(|p| p.join("claude.exe"))
}

/// Whether a launch is worth making. Pure, so every branch is testable without a clock or a process.
pub fn should_renew(expiry: Option<u64>, now: u64, attempted_for: Option<u64>, last_attempt: u64) -> bool {
    // Nothing read yet: never launch on a guess
    let Some(expiry) = expiry else { return false };
    // Plenty of time left — also the case where the command's own gate would not open either
    if expiry > now + MARGIN_MS {
        return false;
    }
    // Already spent an attempt on this exact token: the whole no-retry-loop guarantee. A launch that
    // failed to move the expiry leaves the same value here next tick, and is refused.
    if attempted_for == Some(expiry) {
        return false;
    }
    if last_attempt != 0 && now.saturating_sub(last_attempt) < COOLDOWN_MS {
        return false;
    }
    true
}

/// Runs the command with nothing on stdin — the whole trick: an immediate end-of-input, so it starts
/// up, renews, and refuses for want of a prompt. Output goes nowhere (a token could in principle be
/// echoed into it). Killed at the timeout rather than left to linger.
fn run(cli: &std::path::Path) -> Option<i32> {
    let mut cmd = if cli.extension().map(|e| e.eq_ignore_ascii_case("exe")).unwrap_or(false) {
        let mut c = std::process::Command::new(cli);
        c.arg("-p");
        c
    } else {
        let mut c = std::process::Command::new("cmd.exe");
        c.args(["/C", &cli.to_string_lossy(), "-p"]);
        c
    };
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().ok()?;
    IGNORED_PID.store(child.id(), Ordering::Relaxed);
    let deadline = std::time::Instant::now() + TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s.code(),
            Ok(None) if std::time::Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    IGNORED_PID.store(0, Ordering::Relaxed);
    status
}

fn set_last(msg: String) {
    crate::applog(&format!("token renewal: {msg}"));
    *LAST.lock().unwrap() = msg;
}

/// For doctor
pub fn probe() -> String {
    let cli = cli_path().map(|p| p.display().to_string()).unwrap_or_else(|| "none found (PATH or Claude Desktop bundle)".into());
    let last = LAST.lock().unwrap().clone();
    let exp = crate::usage::credential_expiry()
        .map(|e| {
            let now = now_ms();
            if e > now { format!("expires in {} min", (e - now) / 60_000) } else { format!("expired {} min ago", (now - e) / 60_000) }
        })
        .unwrap_or_else(|| "no credential".into());
    format!("token renewer: cli={cli} | token {exp}{}", if last.is_empty() { String::new() } else { format!(" | last: {last}") })
}

/// One renewal attempt if the gate is open. Returns whether the expiry moved.
fn consider(attempted_for: &mut Option<u64>, last_attempt: &mut u64) -> bool {
    let now = now_ms();
    let current = crate::usage::credential_expiry();
    if !should_renew(current, now, *attempted_for, *last_attempt) {
        return false;
    }
    let current = current.unwrap();
    *last_attempt = now;
    *attempted_for = Some(current);
    let Some(cli) = cli_path() else {
        set_last("Claude's saved login has expired and no `claude` command is installed to renew it — run Claude Code once to sign in again".into());
        return false;
    };
    crate::applog(&format!("token renewal: token expires at {current} ({}s from now); renewing via {}", (current as i64 - now as i64) / 1000, cli.display()));
    let status = run(&cli);
    // Judged on the outcome, never the exit status: refusing an empty prompt is a non-zero exit and a
    // successful renewal at the same time
    match crate::usage::credential_expiry() {
        Some(after) if after > current => {
            set_last(format!("renewed (exit {:?}), now expires at {after}", status));
            true
        }
        after => {
            set_last(format!("ran (exit {:?}) but the expiry did not move (still {:?}) — run `claude` once in a terminal to sign in again", status, after));
            false
        }
    }
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        crate::activity::lower_thread_priority();
        let mut attempted_for: Option<u64> = None;
        let mut last_attempt: u64 = 0;
        loop {
            if consider(&mut attempted_for, &mut last_attempt) {
                // The usage loop notices the changed credential on its own (backoff escape); nudge it anyway
                crate::usage::request_refresh();
            }
            let _ = &app;
            std::thread::sleep(INTERVAL);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_000_000_000_000;

    #[test]
    fn never_launches_on_a_guess() {
        assert!(!should_renew(None, NOW, None, 0));
    }

    #[test]
    fn plenty_of_time_left_means_no_launch() {
        assert!(!should_renew(Some(NOW + MARGIN_MS + 1), NOW, None, 0));
    }

    #[test]
    fn near_or_past_expiry_launches_once_per_token() {
        let exp = NOW + MARGIN_MS - 1;
        assert!(should_renew(Some(exp), NOW, None, 0));
        assert!(should_renew(Some(NOW - 7 * 86_400_000), NOW, None, 0)); // a week expired, like today
        // the same token, already attempted: never again
        assert!(!should_renew(Some(exp), NOW + COOLDOWN_MS + 1, Some(exp), NOW));
        // a different (renewed) token opens the gate again
        assert!(should_renew(Some(exp + 1), NOW + COOLDOWN_MS + 1, Some(exp), NOW));
    }

    #[test]
    fn cooldown_holds_between_attempts() {
        assert!(!should_renew(Some(NOW), NOW + 1_000, Some(NOW - 1), NOW));
        assert!(should_renew(Some(NOW), NOW + COOLDOWN_MS, Some(NOW - 1), NOW));
    }
}
