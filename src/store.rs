use crate::models::*;
use crate::transcript::TranscriptEvent;
use crate::util::{append_string, ensure_dir, read_to_string_if_exists, write_string};
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub const CONFIG_DIR: &str = ".fuzzy";
pub const CONFIG_FILE: &str = "config.toml";
pub const CHATS_DIR: &str = "chats";
// Active-run pointer; consumed by Phase 2 first-turn run binding.
#[allow(dead_code)]
pub const ACTIVE_RUN_FILE: &str = "current_run";

#[derive(Debug, Clone)]
pub struct Store {
    pub root: PathBuf,
    pub config: ConfigDoc,
}

impl Store {
    pub fn init_at(root: PathBuf, force: bool) -> Result<Self> {
        ensure_dir(&root)?;
        let fuzzy_dir = root.join(CONFIG_DIR);
        ensure_dir(&fuzzy_dir)?;
        let config_path = fuzzy_dir.join(CONFIG_FILE);
        if config_path.exists() && !force {
            return Err(anyhow!(
                "{} already exists; use --force to overwrite the default config",
                config_path.display()
            ));
        }
        let config = ConfigDoc::default();
        write_toml(&config_path, &config)?;
        ensure_dir(&root.join(&config.runs_dir))?;
        ensure_dir(&fuzzy_dir.join("knowledge"))?;
        write_string(
            &fuzzy_dir.join("README.md"),
            "# Fuzzy Harness Project State\n\nThis directory stores local harness configuration and optional flat-file knowledge.\n",
        )?;
        Ok(Self { root, config })
    }

    pub fn open(root_override: Option<PathBuf>) -> Result<Self> {
        let root = match root_override {
            Some(path) => path,
            None => find_project_root(env::current_dir()?)?,
        };
        let config_path = root.join(CONFIG_DIR).join(CONFIG_FILE);
        let config: ConfigDoc = read_toml(&config_path)?;
        Ok(Self { root, config })
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join(CONFIG_DIR).join(CONFIG_FILE)
    }

    pub fn runs_root(&self) -> PathBuf {
        self.root.join(&self.config.runs_dir)
    }

    pub fn run_dir(&self, run_id: &str) -> PathBuf {
        self.runs_root().join(run_id)
    }

    pub fn create_run(&self, run: &RunDoc) -> Result<PathBuf> {
        let dir = self.run_dir(&run.id);
        let dirs = [
            "librarian/knowledge_packets",
            "librarian/openviking_queries",
            "explorer/evidence_packets",
            "explorer/pi_sessions",
            "evidence/files",
            "outputs",
            "handoff/workbench",
            "logs/reference_librarian",
            "logs/information_explorer",
            "scratch",
        ];
        for d in dirs {
            ensure_dir(&dir.join(d))?;
        }
        write_yaml(&dir.join("run.yaml"), run)?;
        write_yaml(
            &dir.join("open_questions.yaml"),
            &OpenQuestionsDoc::default(),
        )?;
        write_yaml(
            &dir.join("hypothesis_ledger.yaml"),
            &HypothesisLedgerDoc::default(),
        )?;
        write_yaml(
            &dir.join("evidence_ledger.yaml"),
            &EvidenceLedgerDoc::default(),
        )?;
        write_yaml(&dir.join("decision_log.yaml"), &DecisionLogDoc::default())?;
        write_yaml(&dir.join("action_log.yaml"), &ActionLogDoc::default())?;
        write_yaml(&dir.join("risk_log.yaml"), &RiskLogDoc::default())?;
        write_yaml(&dir.join("constraints.yaml"), &ConstraintsDoc::default())?;
        write_yaml(&dir.join("non_goals.yaml"), &NonGoalsDoc::default())?;
        write_yaml(&dir.join("option_matrix.yaml"), &OptionMatrixDoc::default())?;
        write_yaml(&dir.join("policy.yaml"), &run.policies)?;
        write_string(&dir.join("events.jsonl"), "")?;
        write_string(&dir.join("work_brief.md"), &default_work_brief(run))?;
        self.append_event(
            Some(&run.id),
            "run.created",
            json!({"mode": run.mode, "title": run.title}),
        )?;
        Ok(dir)
    }

    pub fn list_runs(&self) -> Result<Vec<RunDoc>> {
        let mut runs = Vec::new();
        let root = self.runs_root();
        if !root.exists() {
            return Ok(runs);
        }
        for entry in fs::read_dir(&root).with_context(|| format!("listing {}", root.display()))? {
            let entry = entry?;
            let path = entry.path().join("run.yaml");
            if path.exists() {
                match read_yaml::<RunDoc>(&path) {
                    Ok(run) => runs.push(run),
                    Err(err) => eprintln!("warning: failed to read {}: {err}", path.display()),
                }
            }
        }
        runs.sort_by_key(|r| r.created_at);
        runs.reverse();
        Ok(runs)
    }

    pub fn latest_run_id(&self) -> Result<String> {
        self.list_runs()?
            .first()
            .map(|r| r.id.clone())
            .ok_or_else(|| {
                anyhow!("no runs found; create one with `fuzzy start --mode investigate ...`")
            })
    }

    pub fn resolve_run_id(&self, run: Option<String>) -> Result<String> {
        match run {
            Some(r) => Ok(r),
            None => self.latest_run_id(),
        }
    }

    pub fn load_run(&self, run_id: &str) -> Result<RunDoc> {
        read_yaml(&self.run_dir(run_id).join("run.yaml"))
    }

    pub fn save_run(&self, run: &RunDoc) -> Result<()> {
        write_yaml(&self.run_dir(&run.id).join("run.yaml"), run)
    }

    pub fn append_event(
        &self,
        run_id: Option<&str>,
        event_type: &str,
        data: serde_json::Value,
    ) -> Result<()> {
        let record = EventRecord {
            timestamp: Utc::now(),
            run_id: run_id.map(|s| s.to_string()),
            event_type: event_type.to_string(),
            data,
        };
        let line = serde_json::to_string(&record)? + "\n";
        match run_id {
            Some(id) => append_string(&self.run_dir(id).join("events.jsonl"), &line),
            None => append_string(&self.root.join(CONFIG_DIR).join("events.jsonl"), &line),
        }
    }

    pub fn load_questions(&self, run_id: &str) -> Result<OpenQuestionsDoc> {
        read_yaml(&self.run_dir(run_id).join("open_questions.yaml"))
    }

    pub fn save_questions(&self, run_id: &str, doc: &OpenQuestionsDoc) -> Result<()> {
        write_yaml(&self.run_dir(run_id).join("open_questions.yaml"), doc)
    }

    pub fn load_hypotheses(&self, run_id: &str) -> Result<HypothesisLedgerDoc> {
        read_yaml(&self.run_dir(run_id).join("hypothesis_ledger.yaml"))
    }

    pub fn save_hypotheses(&self, run_id: &str, doc: &HypothesisLedgerDoc) -> Result<()> {
        write_yaml(&self.run_dir(run_id).join("hypothesis_ledger.yaml"), doc)
    }

    pub fn load_evidence(&self, run_id: &str) -> Result<EvidenceLedgerDoc> {
        read_yaml(&self.run_dir(run_id).join("evidence_ledger.yaml"))
    }

    pub fn save_evidence(&self, run_id: &str, doc: &EvidenceLedgerDoc) -> Result<()> {
        write_yaml(&self.run_dir(run_id).join("evidence_ledger.yaml"), doc)
    }

    pub fn load_decisions(&self, run_id: &str) -> Result<DecisionLogDoc> {
        read_yaml(&self.run_dir(run_id).join("decision_log.yaml"))
    }

    pub fn save_decisions(&self, run_id: &str, doc: &DecisionLogDoc) -> Result<()> {
        write_yaml(&self.run_dir(run_id).join("decision_log.yaml"), doc)
    }

    pub fn load_risks(&self, run_id: &str) -> Result<RiskLogDoc> {
        read_yaml(&self.run_dir(run_id).join("risk_log.yaml"))
    }

    pub fn save_risks(&self, run_id: &str, doc: &RiskLogDoc) -> Result<()> {
        write_yaml(&self.run_dir(run_id).join("risk_log.yaml"), doc)
    }

    pub fn load_constraints(&self, run_id: &str) -> Result<ConstraintsDoc> {
        read_yaml(&self.run_dir(run_id).join("constraints.yaml"))
    }

    pub fn save_constraints(&self, run_id: &str, doc: &ConstraintsDoc) -> Result<()> {
        write_yaml(&self.run_dir(run_id).join("constraints.yaml"), doc)
    }

    pub fn load_non_goals(&self, run_id: &str) -> Result<NonGoalsDoc> {
        read_yaml(&self.run_dir(run_id).join("non_goals.yaml"))
    }

    pub fn save_non_goals(&self, run_id: &str, doc: &NonGoalsDoc) -> Result<()> {
        write_yaml(&self.run_dir(run_id).join("non_goals.yaml"), doc)
    }

    pub fn save_option_matrix(&self, run_id: &str, doc: &OptionMatrixDoc) -> Result<()> {
        write_yaml(&self.run_dir(run_id).join("option_matrix.yaml"), doc)
    }

    pub fn next_id(&self, _run_id: &str, prefix: &str, count: usize) -> String {
        format!("{}-{:03}", prefix, count + 1)
    }

    pub fn write_knowledge_packet(
        &self,
        run_id: &str,
        packet: &KnowledgePacket,
    ) -> Result<PathBuf> {
        let path = self
            .run_dir(run_id)
            .join("librarian/knowledge_packets")
            .join(format!("{}.json", packet.id));
        write_json_pretty(&path, packet)?;
        Ok(path)
    }

    pub fn write_exploration_report(
        &self,
        run_id: &str,
        report: &ExplorationReport,
    ) -> Result<PathBuf> {
        let json_path = self
            .run_dir(run_id)
            .join("explorer/evidence_packets")
            .join(format!("{}.json", report.exploration_id));
        write_json_pretty(&json_path, report)?;
        let yaml_path = self
            .run_dir(run_id)
            .join("logs/information_explorer")
            .join(format!("{}_exploration.yaml", report.exploration_id));
        write_yaml(&yaml_path, report)?;
        Ok(json_path)
    }

    // --- Interactive chat (Phase 1) ---

    /// Directory holding a chat session's artifacts.
    pub fn chat_dir(&self, chat_id: &str) -> PathBuf {
        self.root.join(CONFIG_DIR).join(CHATS_DIR).join(chat_id)
    }

    /// Create a chat session directory and persist its metadata.
    pub fn create_chat_session(&self, session: &crate::chat::ChatSession) -> Result<PathBuf> {
        let dir = self.chat_dir(&session.id);
        ensure_dir(&dir.join("turns"))?;
        write_string(&dir.join("transcript.jsonl"), "")?;
        write_json_pretty(
            &dir.join("session.json"),
            &json!({
                "chat_id": session.id,
                "backend": session.backend,
                "run_id": session.run_id,
                "created_at": Utc::now(),
            }),
        )?;
        Ok(dir)
    }

    /// Append a metadata-only transcript event for a chat session.
    pub fn append_transcript_event(&self, chat_id: &str, event: &TranscriptEvent) -> Result<()> {
        let line = serde_json::to_string(event)? + "\n";
        append_string(&self.chat_dir(chat_id).join("transcript.jsonl"), &line)
    }

    /// Persist a turn artifact (raw JSON) under the chat session.
    pub fn write_turn_artifacts(
        &self,
        chat_id: &str,
        turn_index: usize,
        artifact: &serde_json::Value,
    ) -> Result<PathBuf> {
        let path = self
            .chat_dir(chat_id)
            .join("turns")
            .join(format!("turn-{turn_index:03}.json"));
        write_json_pretty(&path, artifact)?;
        Ok(path)
    }

    // The active-run pointer API is wired into Phase 2 first-turn behavior
    // (orchestrator binds/creates a run); defined here per the Phase 1 store
    // contract but not yet exercised.
    #[allow(dead_code)]
    fn active_run_path(&self) -> PathBuf {
        self.root.join(CONFIG_DIR).join(ACTIVE_RUN_FILE)
    }

    /// Read the active run id, if one is set.
    #[allow(dead_code)]
    pub fn active_run_id(&self) -> Result<Option<String>> {
        Ok(read_to_string_if_exists(&self.active_run_path())?
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()))
    }

    /// Set the active run id pointer (`.fuzzy/current_run`).
    #[allow(dead_code)]
    pub fn set_active_run_id(&self, run_id: &str) -> Result<()> {
        write_string(&self.active_run_path(), run_id)
    }

    /// Clear the active run id pointer if present.
    #[allow(dead_code)]
    pub fn clear_active_run_id(&self) -> Result<()> {
        let path = self.active_run_path();
        if path.exists() {
            fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
        }
        Ok(())
    }
}

pub fn find_project_root(start: PathBuf) -> Result<PathBuf> {
    let mut current = start.as_path();
    loop {
        if current.join(CONFIG_DIR).join(CONFIG_FILE).exists() {
            return Ok(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => {
                return Err(anyhow!(
                    "could not find .fuzzy/config.toml; run `fuzzy init` first"
                ))
            }
        }
    }
}

pub fn read_yaml<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_yaml::from_str(&text).with_context(|| format!("parsing YAML {}", path.display()))
}

pub fn write_yaml<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = serde_yaml::to_string(value)
        .with_context(|| format!("serializing YAML {}", path.display()))?;
    write_string(path, &text)
}

pub fn read_toml<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing TOML {}", path.display()))
}

pub fn write_toml<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = toml::to_string_pretty(value)
        .with_context(|| format!("serializing TOML {}", path.display()))?;
    write_string(path, &text)
}

pub fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = serde_json::to_string_pretty(value)
        .with_context(|| format!("serializing JSON {}", path.display()))?;
    write_string(path, &(text + "\n"))
}

pub fn read_json_if_exists<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match read_to_string_if_exists(path)? {
        Some(text) => {
            Ok(Some(serde_json::from_str(&text).with_context(|| {
                format!("parsing JSON {}", path.display())
            })?))
        }
        None => Ok(None),
    }
}

fn default_work_brief(run: &RunDoc) -> String {
    format!(
        "# Work Brief: {}\n\n\
Run: `{}`\n\n\
Mode: `{:?}`\n\n\
## Raw request\n\n{}\n\n\
## Normalized goal\n\n{}\n\n\
## What counts as responsible closure\n\n- Produce the mode-specific output artifacts.\n- Resolve or explicitly defer blocking questions.\n- Support material claims with evidence.\n- Record decisions and caveats.\n\n\
## Notes\n\nAdd observations here as the run evolves.\n",
        run.title, run.id, run.mode, run.request.raw, run.request.normalized_goal
    )
}
