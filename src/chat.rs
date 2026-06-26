//! Interactive chat session state.
//!
//! A `ChatSession` owns the chat identity (`CHAT-YYYYMMDD-HHMMSS`), the
//! resolved backend name, and the optional active run it is bound to.

use crate::util::now_id_timestamp;

#[derive(Debug, Clone)]
pub struct ChatSession {
    pub id: String,
    pub backend: String,
    pub run_id: Option<String>,
}

impl ChatSession {
    pub fn new(backend: impl Into<String>, run_id: Option<String>) -> Self {
        Self {
            id: format!("CHAT-{}", now_id_timestamp()),
            backend: backend.into(),
            run_id,
        }
    }
}
