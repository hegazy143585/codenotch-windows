# Codenotch — working rules for AI coding sessions

## Product goal
Codenotch is a lightweight AI usage and activity monitor. At a glance a user sees which AI tools are
connected, how much usage is left, when it resets, which tool is working / waiting for them / done,
and whether each number is current. Targets: Windows (`windows/`, Rust + Tauri 2), macOS (`Sources/`,
Swift — keep working), and a planned iOS companion.

## Rules
- Inspect before modifying. Don't rewrite working features without a stated reason.
- Never log, display, or send tokens, API keys, cookies, session ids, or auth headers.
- No hardcoded user paths or usernames. Resolve platform directories (`dirs`, `%APPDATA%`).
- No fake or sample data in production code paths.
- Don't claim a capability a provider or platform doesn't have. Show "Not supported" instead.
- One provider's failure must never affect the others. Never hide a provider failure.
- The local event server (`windows/codenotch/src/server.rs`) is loopback-only and must refuse browser
  Origins and foreign Host headers. Provider ids are validated. Keep it that way.
- Everything rendered with `innerHTML` in `ui/notch.html` goes through `esc()`.

## Session-end checklist
1. `cd windows && cargo test --workspace --locked` passes.
2. Build every target this machine allows (Windows: `scripts/build-installer.ps1`).
3. Every fixed bug has a regression test where practical.
4. Update `windows/TASKS.md` statuses and any docs the change affects.
5. Commit with a clear message.
6. Report: Done / Changed files / Tests & builds run (with results) / Not done & why / New issues added.
