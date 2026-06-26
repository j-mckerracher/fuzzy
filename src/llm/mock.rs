//! Deterministic mock chat backend for tests and offline development.
//!
//! It never performs network I/O and is fully deterministic. Behavior depends
//! on whether the request carries the orchestrator system prompt:
//!
//! * **Legacy / minimal prompt** — echoes the last user message as
//!   `[mock] received: <text>` (the Phase 1 contract).
//! * **Orchestrator prompt** (contains the `Fuzzy Action Envelope` marker) —
//!   returns a JSON [`crate::protocol::ActionEnvelope`]. Tool requests are
//!   driven by explicit trigger tokens in the user text so tests stay
//!   deterministic:
//!     - `create_run` → request a `create_run` tool call
//!     - `add_question` → request an `add_question` tool call
//!     - `ask_librarian` → request an `ask_librarian` tool call
//!     - `run_explorer_readonly` → request a `run_explorer_readonly` tool call
//!     - `propose_openviking_memory` → request a `propose_openviking_memory` call
//!     - `validate_gate` → request a `validate_gate` tool call
//!     - `resolve_question` → resolve `Q-001`
//!     - `add_evidence` → request an `add_evidence` tool call
//!     - `add_hypothesis` → request an `add_hypothesis` tool call
//!     - `write_report` → request a `write_report` tool call
//!     - `set_permission_level` → request a `set_permission_level` (branch-write)
//!     - `promote_workbench` → request a `promote_workbench` tool call
//!     - `promote_openviking` → request a `promote_openviking` tool call
//!     - `run_autocontext_review` → request a `run_autocontext_review` call
//!     - `FORCE_UNKNOWN_TOOL` → request a nonexistent tool (block path)
//!     - `FORCE_BAD_JSON` → return malformed JSON (repair/fallback path)
//!
//!   With no trigger token it returns an assistant-only envelope whose message
//!   is `[mock] received: <text>`.

use super::{ChatBackend, ChatRequest, ChatResponse, Role, Usage};
use anyhow::Result;
use serde_json::json;

/// Marker substring present in the orchestrator system prompt.
const ENVELOPE_MARKER: &str = "Fuzzy Action Envelope";

pub struct MockBackend;

impl MockBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

fn count_words(s: &str) -> u32 {
    s.split_whitespace().count() as u32
}

impl ChatBackend for MockBackend {
    fn name(&self) -> &str {
        "mock"
    }

    fn complete_chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let last_user = request
            .messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map(|m| m.content.as_str())
            .unwrap_or("");

        let orchestrator = request
            .messages
            .iter()
            .any(|m| m.role == Role::System && m.content.contains(ENVELOPE_MARKER));
        let has_tool_results = request.messages.iter().any(|m| m.role == Role::Tool);
        // Detect the malformed-output trigger across all user turns so it
        // persists through repair retries (the repair prompt becomes the last
        // user message).
        let force_bad = request
            .messages
            .iter()
            .any(|m| m.role == Role::User && m.content.contains("FORCE_BAD_JSON"));

        let content = if !orchestrator {
            format!("[mock] received: {last_user}")
        } else if force_bad {
            "this is not valid json {oops".to_string()
        } else if has_tool_results {
            // Follow-up pass: summarize and finish (assistant-only).
            envelope_followup()
        } else {
            envelope_first_pass(last_user)
        };

        let prompt_tokens: u32 = request
            .messages
            .iter()
            .map(|m| count_words(&m.content))
            .sum();
        let completion_tokens = count_words(&content);
        Ok(ChatResponse {
            content,
            usage: Usage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
        })
    }
}

/// Build the first-pass action envelope JSON from trigger tokens.
fn envelope_first_pass(user: &str) -> String {
    let mut tool_calls = Vec::new();
    let mut n = 0usize;
    let mut next_id = || {
        n += 1;
        format!("call-{n:03}")
    };

    if user.contains("FORCE_UNKNOWN_TOOL") {
        tool_calls.push(json!({
            "id": next_id(),
            "name": "nonexistent_tool",
            "arguments": {}
        }));
    }
    if user.contains("create_run") {
        tool_calls.push(json!({
            "id": next_id(),
            "name": "create_run",
            "arguments": { "request": user, "mode": "troubleshoot" }
        }));
    }
    if user.contains("add_question") {
        tool_calls.push(json!({
            "id": next_id(),
            "name": "add_question",
            "arguments": { "question": user, "blocking": true }
        }));
    }
    if user.contains("ask_librarian") {
        tool_calls.push(json!({
            "id": next_id(),
            "name": "ask_librarian",
            "arguments": { "question": user, "allow_explorer": true }
        }));
    }
    if user.contains("run_explorer_readonly") {
        tool_calls.push(json!({
            "id": next_id(),
            "name": "run_explorer_readonly",
            "arguments": { "question": user }
        }));
    }
    if user.contains("propose_openviking_memory") {
        tool_calls.push(json!({
            "id": next_id(),
            "name": "propose_openviking_memory",
            "arguments": {
                "title": "mock candidate",
                "body": "mock curation body",
                "sources": ["mock-source"]
            }
        }));
    }

    if user.contains("validate_gate") {
        tool_calls.push(json!({
            "id": next_id(),
            "name": "validate_gate",
            "arguments": {}
        }));
    }
    if user.contains("resolve_question") {
        tool_calls.push(json!({
            "id": next_id(),
            "name": "resolve_question",
            "arguments": { "id": "Q-001", "resolution": "resolved via mock" }
        }));
    }
    if user.contains("add_evidence") {
        tool_calls.push(json!({
            "id": next_id(),
            "name": "add_evidence",
            "arguments": { "claim": user, "source": "mock-log", "confidence": "high" }
        }));
    }
    if user.contains("add_hypothesis") {
        tool_calls.push(json!({
            "id": next_id(),
            "name": "add_hypothesis",
            "arguments": { "claim": user, "confidence": 0.6 }
        }));
    }
    if user.contains("write_report") {
        tool_calls.push(json!({
            "id": next_id(),
            "name": "write_report",
            "arguments": {}
        }));
    }
    if user.contains("set_permission_level") {
        tool_calls.push(json!({
            "id": next_id(),
            "name": "set_permission_level",
            "arguments": { "level": "branch-write" }
        }));
    }
    if user.contains("promote_workbench") {
        tool_calls.push(json!({
            "id": next_id(),
            "name": "promote_workbench",
            "arguments": {}
        }));
    }
    if user.contains("promote_openviking") {
        tool_calls.push(json!({
            "id": next_id(),
            "name": "promote_openviking",
            "arguments": {}
        }));
    }
    if user.contains("run_autocontext_review") {
        tool_calls.push(json!({
            "id": next_id(),
            "name": "run_autocontext_review",
            "arguments": {}
        }));
    }

    let has_calls = !tool_calls.is_empty();
    let assistant_message = if has_calls {
        format!("[mock] requesting {} tool(s)", tool_calls.len())
    } else {
        format!("[mock] received: {user}")
    };

    json!({
        "assistant_message": assistant_message,
        "needs_user_input": !has_calls,
        "needs_followup": has_calls,
        "tool_calls": tool_calls,
        "proposed_permission_change": null,
        "confidence": "partial"
    })
    .to_string()
}

/// Build the follow-up (post-tool) assistant-only envelope.
fn envelope_followup() -> String {
    json!({
        "assistant_message": "[mock] summary: tool results processed",
        "needs_user_input": false,
        "needs_followup": false,
        "tool_calls": [],
        "proposed_permission_change": null,
        "confidence": "partial"
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::ChatMessage;

    #[test]
    fn echoes_last_user_message() {
        let backend = MockBackend::new();
        let req = ChatRequest {
            model: "mock".into(),
            messages: vec![
                ChatMessage::new(Role::System, "system prompt"),
                ChatMessage::new(Role::User, "hello world"),
            ],
        };
        let resp = backend.complete_chat(req).unwrap();
        assert_eq!(resp.content, "[mock] received: hello world");
        assert!(resp.usage.total_tokens > 0);
    }

    #[test]
    fn deterministic_for_same_input() {
        let backend = MockBackend::new();
        let make = || ChatRequest {
            model: "mock".into(),
            messages: vec![ChatMessage::new(Role::User, "same input")],
        };
        let a = backend.complete_chat(make()).unwrap();
        let b = backend.complete_chat(make()).unwrap();
        assert_eq!(a.content, b.content);
        assert_eq!(a.usage.total_tokens, b.usage.total_tokens);
    }
}
