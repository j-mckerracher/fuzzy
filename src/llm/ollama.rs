//! Ollama chat backend.
//!
//! Two transport modes:
//! - Local daemon / local cloud offload: `POST {base_url}/api/chat`.
//! - Direct cloud API: `POST https://ollama.com/api/chat` with a
//!   `Authorization: Bearer $OLLAMA_API_KEY` header.
//!
//! The API key is read from the environment by name and is never logged.

use super::{ChatBackend, ChatRequest, ChatResponse, Usage};
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::json;
use std::env;
use std::time::Duration;

/// Connection + model settings for the Ollama backend.
#[derive(Debug, Clone)]
pub struct OllamaConfig {
    pub base_url: String,
    pub model: String,
    pub direct_cloud_api: bool,
    pub api_key_env: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".into(),
            model: "glm-5.2:cloud".into(),
            direct_cloud_api: false,
            api_key_env: "OLLAMA_API_KEY".into(),
        }
    }
}

/// Ollama-backed [`ChatBackend`].
pub struct OllamaBackend {
    cfg: OllamaConfig,
}

impl OllamaBackend {
    pub fn new(cfg: OllamaConfig) -> Self {
        Self { cfg }
    }

    /// The chat endpoint for the configured mode.
    fn endpoint(&self) -> String {
        if self.cfg.direct_cloud_api {
            "https://ollama.com/api/chat".to_string()
        } else {
            format!("{}/api/chat", self.cfg.base_url.trim_end_matches('/'))
        }
    }
}

#[derive(Deserialize, Default)]
struct OllamaMessage {
    #[serde(default)]
    content: String,
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    #[serde(default)]
    message: Option<OllamaMessage>,
    #[serde(default)]
    prompt_eval_count: u32,
    #[serde(default)]
    eval_count: u32,
}

impl ChatBackend for OllamaBackend {
    fn name(&self) -> &str {
        "ollama"
    }

    fn model(&self) -> &str {
        &self.cfg.model
    }

    fn complete_chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let messages: Vec<_> = request
            .messages
            .iter()
            .map(|m| json!({ "role": m.role.as_str(), "content": m.content }))
            .collect();
        let body = json!({
            "model": self.cfg.model,
            "messages": messages,
            "stream": false,
        });

        let url = self.endpoint();
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(120))
            .build();
        let mut req = agent.post(&url).set("Content-Type", "application/json");

        if self.cfg.direct_cloud_api {
            let key = env::var(&self.cfg.api_key_env).map_err(|_| {
                anyhow!(
                    "direct cloud mode needs an API key in ${} (set it, or disable \
                     agent.ollama.direct_cloud_api for local mode)",
                    self.cfg.api_key_env
                )
            })?;
            if key.trim().is_empty() {
                bail!(
                    "direct cloud mode needs a non-empty ${}",
                    self.cfg.api_key_env
                );
            }
            req = req.set("Authorization", &format!("Bearer {key}"));
        }

        let resp = req
            .send_json(body)
            .map_err(|e| map_transport_error(e, &url, self.cfg.direct_cloud_api))?;
        let parsed: OllamaChatResponse =
            resp.into_json().context("decoding Ollama chat response")?;

        let content = parsed.message.unwrap_or_default().content;
        let usage = Usage {
            prompt_tokens: parsed.prompt_eval_count,
            completion_tokens: parsed.eval_count,
            total_tokens: parsed.prompt_eval_count + parsed.eval_count,
        };
        Ok(ChatResponse { content, usage })
    }
}

/// Translate a `ureq` error into actionable guidance (no secrets included).
fn map_transport_error(err: ureq::Error, url: &str, direct: bool) -> anyhow::Error {
    match err {
        ureq::Error::Status(code, _) => {
            anyhow!("Ollama API returned HTTP {code} from {url}")
        }
        ureq::Error::Transport(t) => {
            if direct {
                anyhow!("could not reach the Ollama cloud API at {url}: {t}")
            } else {
                anyhow!(
                    "Ollama API is not reachable at {url}.\nStart and prepare it with:\n  \
                     ollama serve\n  ollama pull <model>\n  ollama run <model>\nunderlying \
                     error: {t}"
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_endpoint_uses_base_url() {
        let b = OllamaBackend::new(OllamaConfig::default());
        assert_eq!(b.endpoint(), "http://localhost:11434/api/chat");
    }

    #[test]
    fn cloud_endpoint_is_fixed() {
        let cfg = OllamaConfig {
            direct_cloud_api: true,
            ..OllamaConfig::default()
        };
        let b = OllamaBackend::new(cfg);
        assert_eq!(b.endpoint(), "https://ollama.com/api/chat");
    }

    #[test]
    fn trailing_slash_in_base_url_is_trimmed() {
        let cfg = OllamaConfig {
            base_url: "http://localhost:11434/".into(),
            ..OllamaConfig::default()
        };
        let b = OllamaBackend::new(cfg);
        assert_eq!(b.endpoint(), "http://localhost:11434/api/chat");
    }

    #[test]
    fn model_reports_configured_model() {
        let b = OllamaBackend::new(OllamaConfig::default());
        assert_eq!(b.model(), "glm-5.2:cloud");
    }
}
