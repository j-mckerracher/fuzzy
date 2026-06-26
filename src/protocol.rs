//! Backend-portable model response protocol (Phase 2).
//!
//! The LLM never mutates the world directly. It responds with exactly one JSON
//! [`ActionEnvelope`]. The Rust runtime parses it, decides what is allowed, and
//! executes any requested tool calls. Native tool-calling is intentionally not
//! used for the MVP so every backend behaves the same way.

use crate::models::{Confidence, PermissionLevel};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single requested tool invocation carrying a stable id (e.g. `call-001`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

/// The one JSON object a backend must return per turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionEnvelope {
    /// User-visible assistant text.
    #[serde(default)]
    pub assistant_message: String,
    /// Whether the assistant is waiting on the user (assistant-only turn).
    #[serde(default)]
    pub needs_user_input: bool,
    /// Whether tool results should be sent back to the model for a final
    /// response within the same turn.
    #[serde(default)]
    pub needs_followup: bool,
    /// Zero or more requested tool calls.
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    /// Optional requested permission level change.
    #[serde(default)]
    pub proposed_permission_change: Option<PermissionLevel>,
    /// Model self-reported confidence.
    #[serde(default)]
    pub confidence: Option<Confidence>,
}

/// User-facing message shown when the model's envelope cannot be parsed even
/// after repair attempts. No tools are ever run from unparsed text.
pub const FALLBACK_MESSAGE: &str =
    "I could not parse the model's action envelope. No tools were run.";

/// One-shot repair instruction appended when a response fails to parse.
pub const REPAIR_PROMPT: &str = "Your previous response was not valid JSON matching the Fuzzy Action Envelope.\nReturn only corrected JSON. Do not include commentary.";

/// Attempt to parse an action envelope from a raw model response.
///
/// 1. Try the whole response as JSON.
/// 2. Otherwise extract the first fenced ```json block and parse that.
///
/// Returns `None` when neither succeeds; callers then run a repair retry and,
/// failing that, fall back to [`FALLBACK_MESSAGE`].
pub fn try_parse(raw: &str) -> Option<ActionEnvelope> {
    let trimmed = raw.trim();
    if let Ok(env) = serde_json::from_str::<ActionEnvelope>(trimmed) {
        return Some(env);
    }
    if let Some(block) = extract_json_fence(raw) {
        if let Ok(env) = serde_json::from_str::<ActionEnvelope>(block.trim()) {
            return Some(env);
        }
    }
    None
}

/// Extract the contents of the first ```json ... ``` fenced block.
fn extract_json_fence(raw: &str) -> Option<&str> {
    let start_marker = "```json";
    let start = raw.find(start_marker)? + start_marker.len();
    let after = &raw[start..];
    // Skip an immediate newline after the fence marker.
    let body_start = after.strip_prefix('\n').map(|_| start + 1).unwrap_or(start);
    let body = &raw[body_start..];
    let end = body.find("```")?;
    Some(&body[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_raw_json() {
        let raw = r#"{"assistant_message":"hi","tool_calls":[]}"#;
        let env = try_parse(raw).expect("parsed");
        assert_eq!(env.assistant_message, "hi");
        assert!(env.tool_calls.is_empty());
    }

    #[test]
    fn parses_assistant_only_turn() {
        let raw = r#"{"assistant_message":"need more info","needs_user_input":true}"#;
        let env = try_parse(raw).expect("parsed");
        assert!(env.needs_user_input);
        assert!(env.tool_calls.is_empty());
    }

    #[test]
    fn parses_fenced_json_block() {
        let raw = "Sure, here you go:\n```json\n{\"assistant_message\":\"ok\",\"tool_calls\":[{\"id\":\"call-001\",\"name\":\"load_status\",\"arguments\":{}}]}\n```\nthanks";
        let env = try_parse(raw).expect("parsed from fence");
        assert_eq!(env.tool_calls.len(), 1);
        assert_eq!(env.tool_calls[0].name, "load_status");
        assert_eq!(env.tool_calls[0].id, "call-001");
    }

    #[test]
    fn returns_none_for_malformed() {
        assert!(try_parse("this is not json at all {oops").is_none());
    }
}
