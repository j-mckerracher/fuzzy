//! Chat transcript events (JSONL).
//!
//! As of Phase 2 every content body is passed through [`crate::redaction`]
//! before storage, so transcripts may now carry redacted content alongside the
//! existing metadata (event type, role, character count). Raw secrets never
//! reach disk.

use crate::redaction;
use chrono::{DateTime, Utc};
use serde::Serialize;

/// A single transcript event. `content` holds the redacted body when present.
#[derive(Debug, Clone, Serialize)]
pub struct TranscriptEvent {
    pub timestamp: DateTime<Utc>,
    pub chat_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Character length of the original content body, for observability.
    pub chars: usize,
    /// Redacted content body, omitted for metadata-only events.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

impl TranscriptEvent {
    /// Build a metadata-only event. `content` is measured but NOT stored.
    pub fn meta(
        chat_id: &str,
        run_id: Option<&str>,
        event_type: &str,
        role: Option<&str>,
        content: &str,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            chat_id: chat_id.to_string(),
            run_id: run_id.map(|s| s.to_string()),
            event_type: event_type.to_string(),
            role: role.map(|s| s.to_string()),
            chars: content.chars().count(),
            content: None,
        }
    }

    /// Build an event that stores the content body after redaction.
    pub fn entry(
        chat_id: &str,
        run_id: Option<&str>,
        event_type: &str,
        role: Option<&str>,
        content: &str,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            chat_id: chat_id.to_string(),
            run_id: run_id.map(|s| s.to_string()),
            event_type: event_type.to_string(),
            role: role.map(|s| s.to_string()),
            chars: content.chars().count(),
            content: Some(redaction::redact(content)),
        }
    }
}
