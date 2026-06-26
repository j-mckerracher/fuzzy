use anyhow::{Context, Result};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};

// Chat-session IDs use this timestamp form (CHAT-YYYYMMDD-HHMMSS).
pub fn now_id_timestamp() -> String {
    Utc::now().format("%Y%m%d-%H%M%S").to_string()
}

pub fn run_id() -> String {
    format!("FUZ-{}", Utc::now().format("%Y%m%d-%H%M%S"))
}

pub fn slugify(input: &str, max_len: usize) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in input.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
        if out.len() >= max_len {
            break;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "work".into()
    } else {
        out
    }
}

pub fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("creating {}", path.display()))
}

pub fn write_string(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    fs::write(path, content).with_context(|| format!("writing {}", path.display()))
}

pub fn append_string(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    file.write_all(content.as_bytes())
        .with_context(|| format!("appending {}", path.display()))?;
    Ok(())
}

pub fn read_to_string_if_exists(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

pub fn relative_to(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub fn command_exists(bin: &str) -> bool {
    std::process::Command::new(bin)
        .arg("--version")
        .output()
        .map(|o| o.status.success() || !o.stdout.is_empty() || !o.stderr.is_empty())
        .unwrap_or(false)
}

pub fn collect_files(root: &Path, max_files: usize) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files_inner(root, &mut files, max_files)?;
    Ok(files)
}

fn collect_files_inner(path: &Path, files: &mut Vec<PathBuf>, max_files: usize) -> Result<()> {
    if files.len() >= max_files {
        return Ok(());
    }
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let skip_dirs = [
        ".git",
        "target",
        "node_modules",
        ".fuzzy",
        "fuzzy-runs",
        ".venv",
        "dist",
        "build",
        "__pycache__",
    ];
    if path.is_dir() && skip_dirs.contains(&name) {
        return Ok(());
    }
    if path.is_file() {
        if is_text_like(path) {
            files.push(path.to_path_buf());
        }
        return Ok(());
    }
    if path.is_dir() {
        for entry in fs::read_dir(path).with_context(|| format!("listing {}", path.display()))? {
            let entry = entry?;
            collect_files_inner(&entry.path(), files, max_files)?;
            if files.len() >= max_files {
                break;
            }
        }
    }
    Ok(())
}

fn is_text_like(path: &Path) -> bool {
    let Some(ext) = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
    else {
        return false;
    };
    matches!(
        ext.as_str(),
        "rs" | "py"
            | "js"
            | "ts"
            | "tsx"
            | "jsx"
            | "go"
            | "java"
            | "kt"
            | "cs"
            | "md"
            | "mdx"
            | "txt"
            | "json"
            | "yaml"
            | "yml"
            | "toml"
            | "ini"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
            | "html"
            | "css"
            | "scss"
            | "sql"
            | "xml"
            | "csv"
            | "dockerfile"
            | "tf"
            | "hcl"
            | "rb"
            | "php"
            | "swift"
            | "c"
            | "h"
            | "cpp"
            | "hpp"
    )
}

pub fn excerpt_around_match(
    content: &str,
    query_terms: &[String],
    max_chars: usize,
) -> Option<String> {
    let lower = content.to_ascii_lowercase();
    let mut first_pos = None;
    for term in query_terms {
        if term.is_empty() {
            continue;
        }
        if let Some(pos) = lower.find(&term.to_ascii_lowercase()) {
            first_pos = Some(first_pos.map_or(pos, |old: usize| old.min(pos)));
        }
    }
    let pos = first_pos?;
    let start = pos.saturating_sub(max_chars / 3);
    let end = (pos + (max_chars * 2 / 3)).min(content.len());
    let mut excerpt = content[start..end].replace('\n', " ");
    excerpt = excerpt.split_whitespace().collect::<Vec<_>>().join(" ");
    if excerpt.len() > max_chars {
        excerpt.truncate(max_chars);
    }
    Some(excerpt)
}

pub fn query_terms(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| s.len() >= 3)
        .filter(|s| {
            !matches!(
                s.as_str(),
                "the"
                    | "and"
                    | "for"
                    | "with"
                    | "that"
                    | "this"
                    | "what"
                    | "where"
                    | "when"
                    | "why"
                    | "how"
                    | "should"
                    | "would"
                    | "could"
            )
        })
        .collect()
}
