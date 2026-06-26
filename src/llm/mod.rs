//! Chat backend abstraction for the interactive orchestrator.
//!
//! Phase 1 defines the trait + transport types and a deterministic mock
//! backend. Real network backends (Ollama) land in Phase 3.

pub mod mock;
pub mod ollama;

use anyhow::{anyhow, Result};
use std::env;

/// Role of a single chat message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
    }
}

/// A single message in a chat request.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

impl ChatMessage {
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }
}

/// A chat completion request handed to a backend.
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
}

/// Token accounting reported by a backend (best-effort).
#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// A chat completion response.
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub usage: Usage,
}

/// A pluggable chat backend.
pub trait ChatBackend {
    fn name(&self) -> &str;
    /// The concrete model identifier sent on the wire. Defaults to [`name`].
    fn model(&self) -> &str {
        self.name()
    }
    fn complete_chat(&self, request: ChatRequest) -> Result<ChatResponse>;
}

/// Resolve the effective backend name following the precedence order:
/// chat `--backend` → global `--agent-backend` → `FUZZY_AGENT_BACKEND`
/// → project config default → `none`.
pub fn resolve_backend_name(
    chat_backend: Option<&str>,
    global_backend: Option<&str>,
    config_default: Option<&str>,
) -> String {
    if let Some(name) = chat_backend.filter(|s| !s.is_empty()) {
        return name.to_string();
    }
    if let Some(name) = global_backend.filter(|s| !s.is_empty()) {
        return name.to_string();
    }
    if let Some(name) = env::var("FUZZY_AGENT_BACKEND")
        .ok()
        .filter(|s| !s.is_empty())
    {
        return name;
    }
    if let Some(name) = config_default.filter(|s| !s.is_empty()) {
        return name.to_string();
    }
    "none".to_string()
}

/// Instantiate a backend by name. `none` (or unknown) yields a helpful error.
pub fn build_backend(
    name: &str,
    ollama_config: ollama::OllamaConfig,
) -> Result<Box<dyn ChatBackend>> {
    match name {
        "mock" => Ok(Box::new(mock::MockBackend::new())),
        "ollama" => Ok(Box::new(ollama::OllamaBackend::new(ollama_config))),
        "none" | "" => Err(anyhow!(
            "no agent backend configured; pass `--backend mock` or set a default \
             (available backends: mock, ollama)"
        )),
        other => Err(anyhow!(
            "unknown agent backend `{other}` (available backends: mock, ollama)"
        )),
    }
}
