# Codenotch for Windows — architecture (as of v0.3.0)

## Components
| Part | File(s) | Role |
|---|---|---|
| App shell | `codenotch/src/main.rs` | Tauri 2 app, window placement, hit-testing, tray wiring, Tauri commands |
| Provider registry | `providers.rs` | `Provider` trait + `REGISTRY`; one `UsageSnapshot` slot per id (`AppState.usage`); builds the `Vec<ProviderSnapshot>` the page renders |
| Usage providers | `usage.rs` (Claude API), `claude_desktop.rs`, `claude_refresh.rs`, `codex.rs`, `cursor.rs`, `antigravity.rs` | Each runs its own thread and writes only its own slot |
| Activity engine | `activity.rs`, `watcher.rs`, `state.rs`, `focus.rs` | 2 s tick: pushed events + per-provider probes (Cursor SQLite, Codex rollout files, Claude IO sampling, Antigravity writes) |
| Event ingress | `server.rs` + `codenotch-hook/` | `127.0.0.1:48666` HTTP. `codenotch-hook.exe <event> [--provider id]` POSTs events; `GET /activity` returns the merged list |
| UI | `ui/notch.html` | Single-file pill + hover card, no framework |
| Diagnostics | `doctor.rs`, `diag.rs` | `codenotch.exe doctor` CLI self-check |
| Config | `config.rs` | `%APPDATA%\codenotch\config.json` |

## Data flow
Provider thread → its slot in `AppState.usage` → `providers::publish` → event `providers` (the full
`Vec<ProviderSnapshot>`: id, name, glyph, capabilities, usage) → `notch.html` renders the list as-is.
The list holds registered providers that are installed (Claude always), then any tool that only pushed
activity through `codenotch-hook --provider <id>`. The page fetches the same list with `get_providers`.
Each entry carries `freshness` (live / stale / offline / error / needs_auth / no_data) and `age_ms`,
computed in Rust (`providers::freshness`); windows whose reset passed after the reading are `expired`.
`providers::start_clock` republishes every 30 s and refreshes every provider after sleep/resume.

## Provider matrix
| Provider | Usage source | Source type | Activity source | Activity type |
|---|---|---|---|---|
| Claude | `api.anthropic.com/api/oauth/usage` with Claude Code's token; Claude Desktop `plan-usage-history.json` | Unofficial API + local file | Claude Code hooks → codenotch-hook; transcript watcher; process IO sampling | Event (hooks) + inferred |
| Codex | `chatgpt.com/backend-api/wham/usage` with `~/.codex/auth.json`; rollout log fallback | Unofficial API + local file | Rollout file parsing | Inferred |
| Cursor | `cursor.com/api/usage-summary` with session from `state.vscdb` | Unofficial API + local SQLite | `state.vscdb` composer state | Inferred (local) |
| Antigravity | Local language-server bridge → Cloud Code API → turn count | Local unofficial + unofficial API | Recent writes | Inferred |
| Gemini API | Token counts from Gemini CLI chats, OpenCode and Hermes SQLite logs (`gemini_api.rs`); no network, key never read | Local files | none (`not_supported`) unless pushed | — |
| Ollama (local) | `GET /api/ps` on the loopback Ollama server (`ollama_local.rs`); loaded models, no quota (`capabilities.usage = false`) | Official local API | none (`not_supported`) unless pushed | — |
| GLM (Z.ai) | `<console>/api/monitor/usage/quota/limit` with the plan key from Claude Code settings (Z.ai base URL only), ZCode or OpenCode (`glm.rs`) | Unofficial API | none unless pushed | — |
| Ollama Cloud | `ollama.com/api/usage` with `OLLAMA_API_KEY` or Credential Manager `codenotch:ollama-api-key` (`ollama_cloud.rs`) | Vendor API, user's own key | none unless pushed | — |
| Any other tool | none | — | `codenotch-hook --provider <id>` | Event (manual wiring) |

The macOS app supports more providers (Grok, OpenCode, Command Code, GitHub Copilot,
Perplexity). Gemini API, Ollama (local runtime and cloud) and GLM are ported (W-07); the others are not yet.

## Why live activity is inconsistent
Only Claude has a reliable event source (hooks). Codex, Cursor, and Antigravity are inferred from file
changes, and the rest only show activity if the user wires up the hook manually. Each provider now carries
`capabilities.activity` (`event` / `inferred` / `not_supported`) and the card shows it, so the user can tell
"idle" from "estimated" (W-08). Activity rows carry `pushed` so a real event is never labelled a guess.

## Adding a provider
HTTP adapters with a borrowed credential use `remote.rs` (poll loop, persisted 429 backoff, offline/stale
rules) and supply only `fetch`; they register as a `providers::Simple` entry.
A provider with usage needs one module (`load_persisted` / `start` / `request_refresh`, writing its slot
and calling `providers::publish`), one `Provider` impl in `providers.rs`, and one line in `REGISTRY`.
No UI change. A provider with activity only needs no code: it pushes events through the hook.
