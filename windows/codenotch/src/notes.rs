//! Provider notes as stable codes + arguments (W-23).
//!
//! Every adapter describes its note as a list of parts. Each part is a code from `EN` with
//! positional arguments (`{0}`, `{1}`), or `text` for words that are not translated (plan and
//! product names, error details from the network stack). The English sentence is still rendered
//! here into `UsageSnapshot.note` (tray logs, the settings window, and the fallback for a page that
//! does not know a code); `notch.html` translates the parts with the same keys in its `STR`
//! dictionary and joins them with " · ".
//!
//! The English templates below must equal the page's `STR.en` entries: a test in `i18n.rs` checks it.
//! Templates are single-quote free (the page dictionary uses '…' strings): write ’ instead.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NotePart {
    pub code: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
}

/// Verbatim text: `args[0]` is shown as-is in every language
pub const TEXT: &str = "text";

pub const EN: &[(&str, &str)] = &[
    // Shared by several providers
    ("nRateLimited", "Rate limited — retrying in {0}s"),
    ("nOffline", "Offline — {0}"),
    ("nLiveFailed", "Live read failed ({0})"),
    ("nVia", "via {0}"),
    ("nNoWindows", "{0} reported no usage windows"),
    ("nNothingMetered", "{0} has nothing metered on this account yet"),
    ("nUnlimitedQuotas", "{0} reported no metered quotas (unlimited)"),
    // Claude
    ("nCredRefreshed", "Credential refreshed — fetching"),
    ("nClaudeNoCred", "No Claude Code credential found"),
    ("nClaudeExpired", "Credential expired — run any claude command (or chat with Claude) to refresh it"),
    ("nClaudeExpiredLimited", "Credential expired — run any claude command (or chat with Claude) to refresh it; the old token is rate-limited until then"),
    ("nClaudeRejected", "Credential rejected (switched accounts?)"),
    ("nViaDesktop", "Live via Claude Desktop (it samples every 15 min)"),
    // Codex
    ("nCodexExpired", "Codex sign-in expired — open Codex once to refresh it"),
    ("nCodexRejected", "Codex rejected its sign-in — sign in to Codex again"),
    ("nFromLastRun", "from last Codex run"),
    ("nCodexNoSnapshot", "Codex has not recorded a usage snapshot yet"),
    // Cursor
    ("nCursorSignIn", "Sign in to Cursor (the editor) to see usage."),
    ("nCursorRejected", "Cursor session was rejected — sign in again in the editor"),
    ("nCursorUnlimited", "Unlimited on the {0} plan — nothing to meter"),
    ("nCursorNothing", "The {0} plan has nothing for Cursor to meter yet"),
    ("nCursorUnlimitedAny", "Unlimited plan — nothing to meter"),
    ("nCursorNothingAny", "This plan has nothing for Cursor to meter yet"),
    // Antigravity
    ("nAgClosed", "Antigravity is closed — last reading kept"),
    ("nAgRejected", "Antigravity’s Google session was rejected — sign in again in Antigravity"),
    ("nAgNoQuota", "Google publishes no quota for this account"),
    ("nAgOpen", "Open Antigravity to read its quota"),
    // Gemini API (local logs)
    ("nGeminiCalls", "{0} calls this month"),
    ("nPerToken", "billed per token, no limit"),
    ("nLocalLogs", "counted from local logs; your API key is never read"),
    // Ollama
    ("nOllamaLoaded", "Loaded: {0}"),
    ("nOllamaNoModel", "Server running · no model loaded"),
    ("nLocalNoQuota", "local server, no quota"),
    ("nOllamaDown", "Ollama server not running — open Ollama to see loaded models"),
    ("nOllamaNoUsage", "No Ollama cloud usage recorded yet for this period"),
    ("nNoResetDate", "the API publishes no reset date"),
    ("nOllamaKeyRejected", "Ollama rejected the API key — create a new one at ollama.com/settings/keys"),
    // Sign-in hints of the borrowed-credential providers
    ("nCmdRejected", "Command Code rejected the key — sign in again in the Command Code app"),
    ("nGhRejected", "GitHub rejected the token — run `gh auth login` and make sure Copilot is enabled"),
    ("nZaiRejected", "Z.ai rejected the plan key — sign in again in the tool that holds it"),
    ("nGrokExpired", "Grok sign-in expired — run `grok login` to refresh it"),
    ("nGrokRejected", "Grok rejected the sign-in — run `grok login`"),
    ("nOpencodeNoSub", "No OpenCode Go subscription on this key"),
    ("nOpencodeRejected", "OpenCode rejected the Go key — run `opencode auth login` again"),
    // Perplexity
    ("nPplxCounts", "counts left; no totals or reset times are published"),
    ("nPplxSignIn", "Sign in, or pass Perplexity’s check: click the Perplexity cell"),
];

fn template(code: &str) -> Option<&'static str> {
    EN.iter().find(|(c, _)| *c == code).map(|(_, t)| *t)
}

/// A coded part. The code must be in `EN` (checked in debug builds and by the tests).
pub fn p(code: &str, args: &[&str]) -> NotePart {
    debug_assert!(template(code).is_some(), "unknown note code {code}");
    NotePart { code: code.into(), args: args.iter().map(|a| a.to_string()).collect() }
}

/// A coded part without arguments
pub fn c(code: &str) -> NotePart {
    p(code, &[])
}

/// Words shown the same in every language (plan and product names, raw error details)
pub fn text(s: impl Into<String>) -> NotePart {
    NotePart { code: TEXT.into(), args: vec![s.into()] }
}

/// One part in English. `{n}` is replaced in a single pass, so an argument containing `{1}` stays literal.
pub fn english(part: &NotePart) -> String {
    let arg = |i: usize| part.args.get(i).cloned().unwrap_or_default();
    if part.code == TEXT {
        return arg(0);
    }
    let Some(t) = template(&part.code) else { return part.args.join(" ") };
    let mut out = String::new();
    let mut rest = t;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}').and_then(|close| after[..close].parse::<usize>().ok().map(|i| (i, close))) {
            Some((i, close)) => {
                out.push_str(&arg(i));
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The whole note in English: non-empty parts joined with " · "
pub fn render(parts: &[NotePart]) -> String {
    parts.iter().map(english).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_render_to_the_english_sentence() {
        assert_eq!(render(&[text("Plus"), p("nVia", &["Codex"])]), "Plus · via Codex");
        assert_eq!(render(&[p("nRateLimited", &["60"])]), "Rate limited — retrying in 60s");
        assert_eq!(render(&[text(""), c("nFromLastRun")]), "from last Codex run");
        assert_eq!(render(&[]), "");
    }

    #[test]
    fn arguments_are_not_substituted_twice() {
        assert_eq!(english(&p("nOffline", &["{0} {1}"])), "Offline — {0} {1}");
    }

    #[test]
    fn codes_are_unique_camel_case_and_quote_free() {
        let mut seen = std::collections::BTreeSet::new();
        for (code, en) in EN {
            assert!(seen.insert(*code), "{code} twice");
            // The page dictionary parser reads keys as [A-Za-z0-9]+ and values as '…'
            assert!(code.starts_with('n') && code.chars().all(|c| c.is_ascii_alphanumeric()), "{code}");
            assert!(!en.contains('\'') && !en.contains('\\') && !en.contains('\n'), "{code}");
        }
    }

    #[test]
    fn serialized_parts_carry_code_and_args() {
        let v = serde_json::to_value(vec![text("Pro"), c("nAgOpen")]).unwrap();
        assert_eq!(v, serde_json::json!([{"code": "text", "args": ["Pro"]}, {"code": "nAgOpen"}]));
    }
}
