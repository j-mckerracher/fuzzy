//! Secret redaction (Phase 2).
//!
//! Every prompt, model response, and tool/command output flows through
//! [`redact`] before it is written to a transcript or event log. The goal is to
//! keep obvious credentials — API keys, bearer tokens, `Authorization` headers,
//! and `.env`-style secret assignments — off disk. Redaction is intentionally
//! conservative: it favors a few high-signal patterns over aggressive matching
//! so ordinary text (commit SHAs, run IDs, prose) is left intact.
//!
//! No regex dependency is used; matching is done with plain string scanning.

/// Replacement token substituted for any detected secret.
pub const REDACTED: &str = "[REDACTED]";

/// Redact secrets from `input`, preserving line structure.
pub fn redact(input: &str) -> String {
    input.split_inclusive('\n').map(redact_segment).collect()
}

/// Redact a single line (which may carry a trailing `\n`).
fn redact_segment(segment: &str) -> String {
    let (line, newline) = match segment.strip_suffix('\n') {
        Some(rest) => (rest, "\n"),
        None => (segment, ""),
    };
    let mut s = redact_sensitive_assignment(line);
    s = redact_inline_assignments(&s);
    s = redact_bearer(&s);
    s = redact_token_words(&s);
    format!("{s}{newline}")
}

/// Redact `key = value` / `key: value` pairs whose key looks sensitive. This
/// also covers `Authorization:` headers (key contains `authorization`).
fn redact_sensitive_assignment(line: &str) -> String {
    let Some(idx) = line.find(['=', ':']) else {
        return line.to_string();
    };
    let (key_raw, rest) = line.split_at(idx);
    let sep = &rest[..1];
    let value = &rest[1..];
    if value.trim().is_empty() {
        return line.to_string();
    }
    let key_norm = key_raw
        .trim()
        .trim_matches(|c: char| c == '"' || c == '\'' || c == '-' || c.is_whitespace())
        .to_ascii_lowercase();
    if !is_sensitive_key(&key_norm) {
        return line.to_string();
    }
    let leading_ws: String = value.chars().take_while(|c| c.is_whitespace()).collect();
    format!("{key_raw}{sep}{leading_ws}{REDACTED}")
}

/// Redact `key=value` pairs that appear as whitespace-delimited tokens later in
/// a line (e.g. a `.env` assignment mid-prose or inside single-line JSON model
/// output). [`redact_sensitive_assignment`] only inspects the first delimiter;
/// this pass catches the rest.
fn redact_inline_assignments(line: &str) -> String {
    let mut result = line.to_string();
    for word in line.split_whitespace() {
        let Some(idx) = word.find(['=', ':']) else {
            continue;
        };
        let value = &word[idx + 1..];
        if value.is_empty() {
            continue;
        }
        let key_norm = word[..idx]
            .trim_matches(|c: char| {
                c == '"' || c == '\'' || c == '-' || c == '{' || c == ',' || c.is_whitespace()
            })
            .to_ascii_lowercase();
        if is_sensitive_key(&key_norm) {
            let redacted = format!("{}{}{REDACTED}", &word[..idx], &word[idx..idx + 1]);
            result = result.replace(word, &redacted);
        }
    }
    result
}

/// True when an assignment key denotes a credential.
fn is_sensitive_key(key: &str) -> bool {
    const NEEDLES: &[&str] = &[
        "authorization",
        "secret",
        "password",
        "passwd",
        "token",
        "api_key",
        "apikey",
        "api-key",
        "access_key",
        "private_key",
        "client_secret",
        "auth_token",
    ];
    NEEDLES.iter().any(|n| key.contains(n)) || key.ends_with("_key") || key == "key"
}

/// Redact the token following a standalone `Bearer` word.
fn redact_bearer(line: &str) -> String {
    let mut result = line.to_string();
    let words: Vec<&str> = line.split_whitespace().collect();
    for pair in words.windows(2) {
        if pair[0].eq_ignore_ascii_case("bearer") {
            let token = pair[1].trim_matches(|c: char| {
                !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
            });
            if token.len() >= 8 {
                result = result.replace(token, REDACTED);
            }
        }
    }
    result
}

/// Redact whitespace-delimited words that look like API keys/tokens.
fn redact_token_words(line: &str) -> String {
    let mut result = line.to_string();
    for word in line.split_whitespace() {
        let token =
            word.trim_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        if looks_like_secret(token) {
            result = result.replace(token, REDACTED);
        }
    }
    result
}

/// Heuristic: does this bare token look like a credential?
fn looks_like_secret(token: &str) -> bool {
    if token.len() < 12 {
        return false;
    }
    const PREFIXES: &[&str] = &[
        "sk-",
        "ghp_",
        "gho_",
        "ghu_",
        "ghs_",
        "ghr_",
        "github_pat_",
        "xoxb-",
        "xoxp-",
        "xoxa-",
        "AIza",
        "glpat-",
        "AKIA",
    ];
    if token.len() >= 16 && PREFIXES.iter().any(|p| token.starts_with(p)) {
        return true;
    }
    if token.len() >= 32 {
        let valid = token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        let has_upper = token.chars().any(|c| c.is_ascii_uppercase());
        let has_lower = token.chars().any(|c| c.is_ascii_lowercase());
        let has_digit = token.chars().any(|c| c.is_ascii_digit());
        // Mixed-case + digit avoids false positives on lowercase hex (e.g. git SHAs).
        return valid && has_upper && has_lower && has_digit;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_ordinary_text_unchanged() {
        let input = "the import failed at line 42 in module foo";
        assert_eq!(redact(input), input);
    }

    #[test]
    fn does_not_redact_git_sha() {
        // 40-char lowercase hex must survive (no uppercase letters).
        let sha = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
        let line = format!("commit {sha}");
        assert_eq!(redact(&line), line);
    }

    #[test]
    fn redacts_authorization_header() {
        let out = redact("Authorization: Bearer abcdef123456ABCDEF");
        assert!(out.contains(REDACTED));
        assert!(!out.contains("abcdef123456ABCDEF"));
        assert!(out.starts_with("Authorization:"));
    }

    #[test]
    fn redacts_standalone_bearer_token() {
        let out = redact("sent header Bearer s3cr3t-token-value-1234");
        assert!(out.contains(REDACTED));
        assert!(!out.contains("s3cr3t-token-value-1234"));
    }

    #[test]
    fn redacts_env_style_assignment() {
        let out = redact("API_KEY=sk-livedummyvalue1234567890");
        assert!(out.contains(REDACTED));
        assert!(!out.contains("sk-livedummyvalue1234567890"));
        assert!(out.starts_with("API_KEY="));
    }

    #[test]
    fn redacts_openai_style_key_in_prose() {
        let out = redact("use key sk-ABCDEFGHIJKLMNOP1234567890 today");
        assert!(out.contains(REDACTED));
        assert!(!out.contains("sk-ABCDEFGHIJKLMNOP1234567890"));
    }

    #[test]
    fn redacts_password_assignment() {
        let out = redact("password: hunter2hunter2hunter2");
        assert!(out.contains(REDACTED));
        assert!(!out.contains("hunter2hunter2hunter2"));
    }

    #[test]
    fn preserves_multiline_structure() {
        let input = "line one\nAPI_KEY=sk-ABCDEFGHIJKLMNOP1234567890\nline three\n";
        let out = redact(input);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "line one");
        assert!(lines[1].starts_with("API_KEY="));
        assert!(lines[1].contains(REDACTED));
        assert_eq!(lines[2], "line three");
        assert!(out.ends_with('\n'));
    }
}
