use crate::models::*;
use crate::util::{collect_files, excerpt_around_match, query_terms, relative_to};
use anyhow::{Context, Result};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use uuid::Uuid;

pub struct OpenVikingAdapter {
    bin: String,
}

impl OpenVikingAdapter {
    pub fn new(bin: impl Into<String>) -> Self {
        Self { bin: bin.into() }
    }

    pub fn detect_mode(&self) -> (KnowledgeMode, Vec<ToolAnomaly>) {
        let mut anomalies = Vec::new();
        let installed = Command::new(&self.bin)
            .arg("system")
            .arg("health")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        match installed {
            Ok(status) if status.success() => {
                let overview = Command::new(&self.bin)
                    .arg("overview")
                    .arg("viking://resources/knowledge/")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                match overview {
                    Ok(status) if status.success() => (KnowledgeMode::OpenViking, anomalies),
                    Ok(status) => {
                        anomalies.push(ToolAnomaly {
                            tool: self.bin.clone(),
                            anomaly: format!("overview health check failed with status {status}"),
                        });
                        (KnowledgeMode::FlatFile, anomalies)
                    }
                    Err(err) => {
                        anomalies.push(ToolAnomaly {
                            tool: self.bin.clone(),
                            anomaly: format!("failed to run overview: {err}"),
                        });
                        (KnowledgeMode::FlatFile, anomalies)
                    }
                }
            }
            Ok(status) => {
                anomalies.push(ToolAnomaly {
                    tool: self.bin.clone(),
                    anomaly: format!("health check failed with status {status}"),
                });
                (KnowledgeMode::FlatFile, anomalies)
            }
            Err(err) => {
                anomalies.push(ToolAnomaly {
                    tool: self.bin.clone(),
                    anomaly: format!("not available or not executable: {err}"),
                });
                (KnowledgeMode::FlatFile, anomalies)
            }
        }
    }

    pub fn find(&self, query: &str, limit: usize) -> Result<Vec<KnowledgeSource>> {
        let output = Command::new(&self.bin)
            .arg("-o")
            .arg("json")
            .arg("--compact")
            .arg("find")
            .arg(query)
            .arg("--limit")
            .arg(limit.to_string())
            .output()
            .with_context(|| format!("running {} find", self.bin))?;
        if !output.status.success() {
            return Ok(Vec::new());
        }
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if stdout.is_empty() {
            return Ok(Vec::new());
        }
        let mut sources = Vec::new();
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&stdout) {
            flatten_ov_json_sources(&value, &mut sources, limit);
        }
        if sources.is_empty() {
            sources.push(KnowledgeSource {
                source_type: "knowledge_file".into(),
                source: "openviking:find".into(),
                tier: "L1".into(),
                excerpt: truncate(&stdout, 900),
                relevance: "raw OpenViking find output".into(),
            });
        }
        Ok(sources)
    }

    pub fn add_resource(&self, path: &Path, wait: bool) -> Result<bool> {
        let mut cmd = Command::new(&self.bin);
        cmd.arg("add-resource").arg(path);
        if wait {
            cmd.arg("--wait");
        }
        let status = cmd
            .status()
            .with_context(|| format!("running {} add-resource", self.bin))?;
        Ok(status.success())
    }
}

fn flatten_ov_json_sources(
    value: &serde_json::Value,
    sources: &mut Vec<KnowledgeSource>,
    limit: usize,
) {
    if sources.len() >= limit {
        return;
    }
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                flatten_ov_json_sources(item, sources, limit);
                if sources.len() >= limit {
                    break;
                }
            }
        }
        serde_json::Value::Object(map) => {
            let source = map
                .get("uri")
                .or_else(|| map.get("path"))
                .or_else(|| map.get("source"))
                .and_then(|v| v.as_str())
                .unwrap_or("openviking:result")
                .to_string();
            let excerpt = map
                .get("excerpt")
                .or_else(|| map.get("content"))
                .or_else(|| map.get("text"))
                .or_else(|| map.get("summary"))
                .and_then(|v| v.as_str())
                .map(|s| truncate(s, 900))
                .unwrap_or_else(|| {
                    truncate(&serde_json::Value::Object(map.clone()).to_string(), 900)
                });
            sources.push(KnowledgeSource {
                source_type: "knowledge_file".into(),
                source,
                tier: "L1".into(),
                excerpt,
                relevance: "OpenViking semantic retrieval result".into(),
            });
        }
        _ => {}
    }
}

pub fn flat_file_knowledge_search(
    project_root: &Path,
    query: &str,
    limit: usize,
) -> Result<Vec<KnowledgeSource>> {
    let roots = [
        project_root.join(".fuzzy/knowledge"),
        project_root.join("agent-context/knowledge"),
        project_root.join("knowledge"),
        project_root.join("docs"),
    ];
    search_roots(project_root, &roots, query, limit, "knowledge_file")
}

pub fn repository_explore(
    project_root: &Path,
    query: &str,
    scope: Option<PathBuf>,
    limit: usize,
) -> Result<ExplorationReport> {
    let search_root = scope.unwrap_or_else(|| project_root.to_path_buf());
    let sources = search_roots(
        project_root,
        std::slice::from_ref(&search_root),
        query,
        limit,
        "repo_file",
    )?;
    let confidence = match sources.len() {
        0 => Confidence::None,
        1 => Confidence::Partial,
        _ => Confidence::Full,
    };
    let key_file_paths = sources.iter().map(|s| s.source.clone()).collect::<Vec<_>>();
    let unresolved_gaps = if sources.is_empty() {
        vec![UnresolvedGap {
            gap: query.to_string(),
            reason: "No repository evidence matched the query terms.".into(),
        }]
    } else {
        Vec::new()
    };
    let next_suggestions = if sources.is_empty() {
        vec![NextSuggestion { action: "Rephrase the query, provide a narrower scope, or ask a human for likely source locations.".into(), priority: "high".into() }]
    } else {
        vec![NextSuggestion { action: "Return these evidence snippets to the Reference Librarian for synthesis and knowledge curation.".into(), priority: "medium".into() }]
    };
    Ok(ExplorationReport {
        exploration_id: format!("EP-{}", Uuid::new_v4().simple()),
        query: query.to_string(),
        knowledge_mode: KnowledgeMode::FlatFile,
        answer_summary: summarize_sources(&sources, query),
        confidence,
        evidence: sources,
        key_file_paths,
        canonical_sources_checked: vec![CanonicalSourceCheck { source: relative_to(&search_root, project_root), result: "searched".into() }],
        unresolved_gaps,
        next_suggestions,
        metacognitive_context: MetacognitiveContext {
            decision_rationale: "Performed focused read-only repository exploration and returned evidence, not final decisions.".into(),
            alternatives_discarded: vec![RejectedAlternative { approach: "Direct knowledge write".into(), reason_rejected: "Only the Reference Librarian/curator promotes durable knowledge.".into() }],
            knowledge_gaps: Vec::new(),
            tool_anomalies: Vec::new(),
        },
        created_at: Utc::now(),
    })
}

fn search_roots(
    project_root: &Path,
    roots: &[PathBuf],
    query: &str,
    limit: usize,
    source_type: &str,
) -> Result<Vec<KnowledgeSource>> {
    let terms = query_terms(query);
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let mut scored: Vec<(usize, KnowledgeSource)> = Vec::new();
    for root in roots {
        if !root.exists() {
            continue;
        }
        let files = collect_files(root, 5000)?;
        for file in files {
            let Ok(content) = fs::read_to_string(&file) else {
                continue;
            };
            let lower = content.to_ascii_lowercase();
            let score = terms
                .iter()
                .filter(|term| lower.contains(term.as_str()))
                .count();
            if score == 0 {
                continue;
            }
            let excerpt = excerpt_around_match(&content, &terms, 700)
                .unwrap_or_else(|| truncate(&content, 700));
            scored.push((
                score,
                KnowledgeSource {
                    source_type: source_type.into(),
                    source: relative_to(&file, project_root),
                    tier: if source_type == "knowledge_file" {
                        "flat".into()
                    } else {
                        "N/A".into()
                    },
                    excerpt,
                    relevance: format!("matched {score} query term(s)"),
                },
            ));
        }
    }
    scored.sort_by_key(|b| std::cmp::Reverse(b.0));
    scored.truncate(limit);
    Ok(scored.into_iter().map(|(_, s)| s).collect())
}

pub fn knowledge_packet_from_sources(
    id: String,
    query: &str,
    mode: KnowledgeMode,
    sources: Vec<KnowledgeSource>,
) -> KnowledgePacket {
    let confidence = match sources.len() {
        0 => Confidence::None,
        1 => Confidence::Partial,
        _ => Confidence::Full,
    };
    let unresolved_gaps = if sources.is_empty() {
        vec!["No knowledge source answered this query.".into()]
    } else {
        Vec::new()
    };
    let next_actions = match confidence {
        Confidence::Full => {
            vec!["Use the answer directly and cite this packet in output artifacts.".into()]
        }
        Confidence::Partial => {
            vec!["Invoke the Information Explorer for focused evidence before finalizing.".into()]
        }
        Confidence::None => {
            vec!["Record as an open question or escalate if it blocks progress.".into()]
        }
        _ => vec!["Review confidence before acting.".into()],
    };
    KnowledgePacket {
        id,
        query: query.to_string(),
        knowledge_mode: mode,
        confidence,
        answer_summary: summarize_sources(&sources, query),
        sources,
        unresolved_gaps,
        next_actions,
        created_at: Utc::now(),
    }
}

pub fn summarize_sources(sources: &[KnowledgeSource], query: &str) -> String {
    match sources.len() {
        0 => format!("No evidence found for query: {query}"),
        1 => format!("Found 1 relevant source: {}", sources[0].source),
        n => format!(
            "Found {n} relevant sources; top source: {}",
            sources[0].source
        ),
    }
}

fn truncate(s: &str, max: usize) -> String {
    let mut out = s.trim().replace('\n', " ");
    out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if out.len() > max {
        out.truncate(max);
        out.push_str("...");
    }
    out
}
