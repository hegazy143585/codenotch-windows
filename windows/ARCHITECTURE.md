# Codenotch for Windows — architecture (as of v0.3.0)

## Components
| Part | File(s) | Role |
|---|---|---|
| App shell | `codenotch/src/main.rs` | Tauri 2 app, window placement, hit-testing, tray wiring, Tauri commands |
| Usage providers | `usage.rs` (Claude API), `claude_desktop.rs`, `claude_refresh.rs`, `codex.rs`, `cursor.rs`, `antigravity.rs` | Each runs its own thread and writes its own `UsageSnapshot` field in `AppState` |
| Activity engine | `activity.rs`, `watcher.rs`, `state.rs`, `focus.rs` | 2 s tick: pushed events + per-provider probes (Cursor SQLite, Codex rollout files, Claude IO sampling, Antigravity writes) |
| Event ingress | `server.rs` + `codenotch-hook/` | `127.0.0.1:48666` HTTP. `codenotch-hook.exe <event> [--provider id]` POSTs events; `GET /activity` returns the merged list |
| UI | `ui/notch.html` | Single-file pill + hover card, no framework |
| Diagnostics | `doctor.rs`, `diag.rs` | `codenotch.exe doctor` CLI self-check |
| Config | `config.rs` | `%APPDATA%\codenotch\config.json` |

## Data flow
Provider thread → `AppState.<provider>` → `broadcast` → UI `get_*` command per provider → `providers()` in
`notch.html` builds the cell list (4 hardcoded + any provider that pushed activity).

## Provider matrix
| Provider | Usage source | Source type | Activity source | Activity type |
|---|---|---|---|---|
| Claude | `api.anthropic.com/api/oauth/usage` with Claude Code's token; Claude Desktop `plan-usage-history.json` | Unofficial API + local file | Claude Code hooks → codenotch-hook; transcript watcher; process IO sampling | Event (hooks) + inferred |
| Codex | `chatgpt.com/backend-api/wham/usage` with `~/.codex/auth.json`; rollout log fallback | Unofficial API + local file | Rollout file parsing | Inferred |
| Cursor | `cursor.com/api/usage-summary` with session from `state.vscdb` | Unofficial API + local SQLite | `state.vscdb` composer state | Inferred (local) |
| Antigravity | Local language-server bridge → Cloud Code API → turn count | Local unofficial + unofficial API | Recent writes | Inferred |
| Any other tool | none | — | `codenotch-hook --provider <id>` | Event (manual wiring) |

The macOS app supports more providers (GLM, Ollama, Grok, OpenCode, Command Code, GitHub Copilot,
Gemini API, Perplexity). None of these have usage collection on Windows yet.

## Why live activity is inconsistent
Only Claude has a reliable event source (hooks). Codex, Cursor, and Antigravity are inferred from file
changes, and the rest only show activity if the user wires up the hook manually. There is no per-provider
"activity supported / inferred / not supported" flag, so the UI can't tell the user which case they're in.

## Main structural limit
Adding a provider with usage needs edits in at least four places: a new `AppState` field, a new Tauri
command, a new UI variable, and a line in `providers()` plus the hover-card text. The target is one
provider trait + one `Vec<ProviderSnapshot>` (see `TASKS.md`, W-06).
