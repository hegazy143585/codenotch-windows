# Codenotch for Windows

A Windows port of [Codenotch](https://github.com/vinzdg/codenotch) — the usage notch that
sits on the edge of your screen and answers two questions at a glance:
**how much of my AI allowance is left**, and **is Claude still working**.

Same design language as the macOS original (inverse-rounded pill, colour-graded rings,
hover card with per-window bars), rebuilt for Windows in Rust + Tauri 2 / WebView2.
No code is copied from the Swift app; the providers are reimplemented from their
documented behaviour and the wire formats.

## What it shows

| Cell | Source | How it reads it |
|---|---|---|
| **Claude** | `GET https://api.anthropic.com/api/oauth/usage` with the token Claude Code keeps in `~/.claude/.credentials.json`, **and** Claude Desktop's own samples in `%APPDATA%\Claude\plan-usage-history.json` (five-hour / seven-day percentages it appends every 15 min) — whichever is newer wins | Session / weekly windows, 429 back-off with a persisted deadline (left early the moment the credential on disk changes), stale readings dimmed by their source's own cadence. Someone who works in Claude Desktop rather than the terminal has a CLI token that quietly expires; the Desktop samples keep the ring current anyway, no token or network needed, and only when the organization matches Claude Code's credential. A thin arc spins inside the ring while a Claude session is working, and pulses amber when one is waiting on you (Claude Code hooks + transcript watcher, desktop app included). |
| **Codex** | `GET https://chatgpt.com/backend-api/wham/usage` with the session Codex keeps in `~/.codex/auth.json` (read only, never refreshed), falling back to the `rate_limits` snapshot in the newest rollout log | Live primary/secondary windows (5h + weekly on paid plans, a monthly window on free) while Codex is signed in; otherwise the last snapshot, marked stale by its own timestamp. |
| **Cursor** | The editor's own session from `state.vscdb` → `cursor.com/api/usage-summary` | Included usage / API usage / on-demand, reset at billing-cycle end. Nothing to sign into: it borrows the editor's session, so there is only ever one account. |
| **Antigravity** | The local `language_server` bridge (quota summary), then Google's Cloud Code API for licensed accounts, then a plain count of today's model turns | Honest degradation: a percentage only when one exists, a `~count` when it does not. |

Providers that are not installed simply do not get a cell.

## Install / build

Prerequisites: Rust (MSVC toolchain), WebView2 runtime (ships with Windows 11).

```powershell
# from this directory (the repo root here; `windows/` inside the upstream repo)
cargo build --release
.\target\release\codenotch.exe          # pill appears on the right edge of the primary monitor
.\target\release\codenotch.exe doctor   # self-diagnosis: credentials, data sources, icons, hooks
```

Tray menu: refresh now, reset position, open data folder (`%APPDATA%\codenotch` — logs,
persisted readings, icon overrides), start with Windows, install/uninstall Claude Code hooks.

### Icons

Provider marks are the SVGs from [`@lobehub/icons-static-svg`](https://github.com/lobehub/lobe-icons)
(MIT), embedded unmodified — see `codenotch/glyphs/NOTICE.md`. Drop your own
`claude|codex|cursor|gemini.svg` (or `.png`) into `%APPDATA%\codenotch\glyphs\` to override.
The marks remain the trademarks of their owners.

## Reporting from any tool

The working state ("spinning / waiting on you") for Claude has always come the reliable way: Claude Code's
hooks run `codenotch-hook.exe <event>`, which POSTs to the local event server, and the ring follows. The
other cells used to depend on reverse-engineering each vendor's private local files, which is one vendor
update away from going dark, and a tool with no probe could never show a working state at all.

That path is now open to every provider. Any tool that can run a command on an event (Codex `notify`,
Cursor hooks, Gemini CLI hooks, a one-line shell wrapper) reports through the same binary:

```sh
codenotch-hook.exe running   --provider copilot   # spinning
codenotch-hook.exe attention --provider copilot   # amber pulse: waiting on you
codenotch-hook.exe done      --provider copilot   # clears the row (also: stop, idle, session_end)
```

`--provider=<id>` and the `CODENOTCH_PROVIDER` env var work too. Stdin may carry JSON with
`session_id` (several concurrent runs of one tool become separate rows), `name`, `detail` and `prompt`,
the same shape Claude Code's hooks send. A row that is never cleared expires after 30 min, so a tool
that crashes mid-run cannot pin the ring green.

Rules of the road:

- A pushed event is **authoritative**: while one is present for a provider, that provider's file probe
  (if it has one) is skipped for the tick. An accurate event always beats a guess.
- A provider with no usage snapshot still earns a cell for as long as it reports activity; nothing in
  `notch.html` lists providers by hand, so adding a tool never means editing the notch.
- `GET http://127.0.0.1:<port>/activity` returns the merged list as JSON — use it to verify a new
  integration without opening the UI (`port` is in `%APPDATA%\codenotch\config.json`, default 48666).

## Layout

```
.
├── codenotch/          Tauri 2 app: window, tray, providers (usage.rs, codex.rs, cursor.rs, antigravity.rs),
│   ├── src/            session engine (watcher.rs, state.rs, focus.rs), glyphs.rs, doctor.rs
│   ├── ui/notch.html   the pill + hover card (single file, no framework)
│   └── glyphs/         provider marks (+ NOTICE.md)
└── codenotch-hook/     <5 ms hook messenger Claude Code calls; forwards events to the app
```

## Relationship to upstream

This port follows the upstream design spec (`docs/specs/2026-08-28-usage-notch-design.md`)
and provider semantics. It is developed at
[Im-Midi/codenotch-windows](https://github.com/Im-Midi/codenotch-windows) and offered to the
upstream project as its `windows/` tree; the two are kept in sync. The session-detection engine
originated in [Im-Midi/Pac-Man](https://github.com/Im-Midi/Pac-Man) (MIT).

## License

MIT — see `LICENSE`. The Codenotch design and name belong to the upstream author.
