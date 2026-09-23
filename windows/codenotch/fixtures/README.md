# Parser fixtures

Used by the unit tests in `src/usage.rs`, `src/codex.rs`, `src/cursor.rs`, `src/antigravity.rs`.
They must never hold tokens, cookies, account ids, emails, or prompt text.

| File | Origin |
|---|---|
| `codex_rollout_free_weekly.jsonl` | Recorded from a real Codex rollout on Windows; token counts zeroed |
| `claude_oauth_usage*.json`, `codex_wham_usage.json`, `cursor_usage_summary_*.json`, `antigravity_bridge.json`, `gemini_cli_chat.jsonl`, `ollama_ps.json`, `ollama_cloud_usage.json`, `opencode_go_usage.json`, `copilot_user.json`, `glm_quota_limit.json` (shapes from the macOS reader's recorded reply) | Hand-built from the reply shapes the parsers read. Replace with sanitized recordings when available |
