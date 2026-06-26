use chrono::{DateTime, Utc};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum, Default)]
#[serde(rename_all = "kebab-case")]
pub enum WorkMode {
    #[default]
    Investigate,
    Troubleshoot,
    Debug,
    Research,
    Triage,
    Design,
    Audit,
    Incident,
    Experiment,
    Scope,
    Deliver,
}

impl WorkMode {
    pub fn expected_outputs(self) -> Vec<String> {
        match self {
            WorkMode::Troubleshoot => vec!["diagnosis_report".into(), "remediation_plan".into()],
            WorkMode::Debug => vec!["repro_notes".into(), "fix_candidate".into()],
            WorkMode::Research => vec!["sourced_findings".into(), "recommendations".into()],
            WorkMode::Triage => vec!["triage_packet".into(), "next_action".into()],
            WorkMode::Design => vec!["option_matrix".into(), "decision_record".into()],
            WorkMode::Audit => vec!["findings".into(), "recommendations".into()],
            WorkMode::Incident => vec!["timeline".into(), "incident_report".into()],
            WorkMode::Experiment => vec!["experiment_plan".into(), "result_log".into()],
            WorkMode::Scope => vec!["story_or_spec".into()],
            WorkMode::Deliver => vec!["delivery_story".into()],
            WorkMode::Investigate => vec!["investigation_report".into()],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RunStatus {
    #[default]
    Active,
    Blocked,
    Completed,
    Abandoned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Confidence {
    Full,
    Partial,
    None,
    High,
    #[default]
    Medium,
    Low,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum, Default)]
#[serde(rename_all = "kebab-case")]
pub enum QuestionStatus {
    #[default]
    Open,
    Resolved,
    Deferred,
    Superseded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum, Default)]
#[serde(rename_all = "kebab-case")]
pub enum HypothesisStatus {
    #[default]
    Open,
    Likely,
    Confirmed,
    Rejected,
    Superseded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionLevel {
    #[default]
    ObserveOnly,
    ScratchOnly,
    BranchWrite,
    RepoWrite,
    Remediate,
    Handoff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ExitType {
    #[default]
    InvestigationReport,
    Diagnosis,
    Decision,
    Escalation,
    ExperimentResult,
    RunbookUpdate,
    DeliveryStory,
    Abandon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum, Default)]
#[serde(rename_all = "kebab-case")]
pub enum KnowledgeMode {
    OpenViking,
    #[default]
    FlatFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum, Default)]
#[serde(rename_all = "kebab-case")]
pub enum BannerMode {
    #[default]
    Auto,
    Always,
    Never,
}

impl std::str::FromStr for BannerMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(BannerMode::Auto),
            "always" => Ok(BannerMode::Always),
            "never" => Ok(BannerMode::Never),
            other => Err(format!("invalid banner mode: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigDoc {
    pub schema_version: String,
    pub runs_dir: String,
    pub default_agent_backend: String,
    pub knowledge_backend: String,
    pub ov_binary: String,
    pub pi_binary: String,
    pub autoctx_binary: String,
    pub workbench_path: Option<String>,
    pub default_permission_level: PermissionLevel,
    #[serde(default = "default_ollama_base_url")]
    pub ollama_base_url: String,
    #[serde(default = "default_ollama_model")]
    pub ollama_model: String,
    #[serde(default)]
    pub ollama_direct_cloud_api: bool,
    #[serde(default = "default_ollama_api_key_env")]
    pub ollama_api_key_env: String,
}

fn default_ollama_base_url() -> String {
    "http://localhost:11434".into()
}
fn default_ollama_model() -> String {
    "glm-5.2:cloud".into()
}
fn default_ollama_api_key_env() -> String {
    "OLLAMA_API_KEY".into()
}

impl Default for ConfigDoc {
    fn default() -> Self {
        Self {
            schema_version: "fuzzy.config.v1".into(),
            runs_dir: "fuzzy-runs".into(),
            default_agent_backend: "none".into(),
            knowledge_backend: "openviking".into(),
            ov_binary: "ov".into(),
            pi_binary: "pi".into(),
            autoctx_binary: "autoctx".into(),
            workbench_path: None,
            default_permission_level: PermissionLevel::ObserveOnly,
            ollama_base_url: default_ollama_base_url(),
            ollama_model: default_ollama_model(),
            ollama_direct_cloud_api: false,
            ollama_api_key_env: default_ollama_api_key_env(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunDoc {
    pub schema_version: String,
    pub id: String,
    pub title: String,
    pub mode: WorkMode,
    pub status: RunStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub request: WorkRequest,
    pub uncertainty: Uncertainty,
    pub policies: RunPolicies,
    pub outputs: OutputExpectations,
    pub integrations: Integrations,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkRequest {
    pub raw: String,
    pub normalized_goal: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Uncertainty {
    pub problem_clarity: String,
    pub cause_clarity: String,
    pub path_clarity: String,
    pub success_clarity: String,
}

impl Default for Uncertainty {
    fn default() -> Self {
        Self {
            problem_clarity: "medium".into(),
            cause_clarity: "low".into(),
            path_clarity: "low".into(),
            success_clarity: "medium".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunPolicies {
    pub permission_level: PermissionLevel,
    pub repo_writes_allowed: bool,
    pub scratch_writes_allowed: bool,
    pub git_commit_allowed: bool,
    pub git_push_allowed: bool,
    pub pr_creation_allowed: bool,
    pub network_allowed: bool,
    pub human_checkpoint_required_before_fix: bool,
}

impl Default for RunPolicies {
    fn default() -> Self {
        Self {
            permission_level: PermissionLevel::ObserveOnly,
            repo_writes_allowed: false,
            scratch_writes_allowed: true,
            git_commit_allowed: false,
            git_push_allowed: false,
            pr_creation_allowed: false,
            network_allowed: true,
            human_checkpoint_required_before_fix: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputExpectations {
    pub expected: Vec<String>,
    pub optional: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Integrations {
    pub knowledge_backend: String,
    pub agent_backend: String,
    pub downstreams: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenQuestionsDoc {
    pub schema_version: String,
    pub questions: Vec<Question>,
}

impl Default for OpenQuestionsDoc {
    fn default() -> Self {
        Self {
            schema_version: "fuzzy.open_questions.v1".into(),
            questions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub id: String,
    pub question: String,
    pub owner: Option<String>,
    pub blocking: bool,
    pub status: QuestionStatus,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub hypotheses: Vec<String>,
    pub evidence: Vec<String>,
    pub resolution: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HypothesisLedgerDoc {
    pub schema_version: String,
    pub hypotheses: Vec<Hypothesis>,
}

impl Default for HypothesisLedgerDoc {
    fn default() -> Self {
        Self {
            schema_version: "fuzzy.hypotheses.v1".into(),
            hypotheses: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hypothesis {
    pub id: String,
    pub claim: String,
    pub status: HypothesisStatus,
    pub confidence: f32,
    pub evidence: Vec<String>,
    pub falsification: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceLedgerDoc {
    pub schema_version: String,
    pub evidence: Vec<Evidence>,
}

impl Default for EvidenceLedgerDoc {
    fn default() -> Self {
        Self {
            schema_version: "fuzzy.evidence.v1".into(),
            evidence: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub id: String,
    pub claim: String,
    pub source_type: String,
    pub source: String,
    pub confidence: Confidence,
    pub excerpt: Option<String>,
    pub file_copy: Option<String>,
    pub used_by: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionLogDoc {
    pub schema_version: String,
    pub decisions: Vec<Decision>,
}

impl Default for DecisionLogDoc {
    fn default() -> Self {
        Self {
            schema_version: "fuzzy.decisions.v1".into(),
            decisions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    pub decision: String,
    pub rationale: Option<String>,
    pub evidence: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub superseded_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionLogDoc {
    pub schema_version: String,
    pub actions: Vec<ActionItem>,
}

impl Default for ActionLogDoc {
    fn default() -> Self {
        Self {
            schema_version: "fuzzy.actions.v1".into(),
            actions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    pub id: String,
    pub action: String,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub result: Option<String>,
}

// ---------- Phase 5 ledgers: risks, constraints, non-goals, options ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskLogDoc {
    pub schema_version: String,
    pub risks: Vec<RiskItem>,
}

impl Default for RiskLogDoc {
    fn default() -> Self {
        Self {
            schema_version: "fuzzy.risks.v1".into(),
            risks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskItem {
    pub id: String,
    pub risk: String,
    pub severity: String,
    pub mitigation: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintsDoc {
    pub schema_version: String,
    pub constraints: Vec<ConstraintItem>,
}

impl Default for ConstraintsDoc {
    fn default() -> Self {
        Self {
            schema_version: "fuzzy.constraints.v1".into(),
            constraints: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintItem {
    pub id: String,
    pub constraint: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NonGoalsDoc {
    pub schema_version: String,
    pub non_goals: Vec<NonGoalItem>,
}

impl Default for NonGoalsDoc {
    fn default() -> Self {
        Self {
            schema_version: "fuzzy.non_goals.v1".into(),
            non_goals: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NonGoalItem {
    pub id: String,
    pub non_goal: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionMatrixDoc {
    pub schema_version: String,
    pub options: Vec<OptionRow>,
    pub updated_at: DateTime<Utc>,
}

impl Default for OptionMatrixDoc {
    fn default() -> Self {
        Self {
            schema_version: "fuzzy.option_matrix.v1".into(),
            options: Vec::new(),
            updated_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionRow {
    pub option: String,
    pub pros: Vec<String>,
    pub cons: Vec<String>,
    pub recommendation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub timestamp: DateTime<Utc>,
    pub run_id: Option<String>,
    pub event_type: String,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgePacket {
    pub id: String,
    pub query: String,
    pub knowledge_mode: KnowledgeMode,
    pub confidence: Confidence,
    pub answer_summary: String,
    pub sources: Vec<KnowledgeSource>,
    pub unresolved_gaps: Vec<String>,
    pub next_actions: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSource {
    pub source_type: String,
    pub source: String,
    pub tier: String,
    pub excerpt: String,
    pub relevance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorationReport {
    pub exploration_id: String,
    pub query: String,
    pub knowledge_mode: KnowledgeMode,
    pub answer_summary: String,
    pub confidence: Confidence,
    pub evidence: Vec<KnowledgeSource>,
    pub key_file_paths: Vec<String>,
    pub canonical_sources_checked: Vec<CanonicalSourceCheck>,
    pub unresolved_gaps: Vec<UnresolvedGap>,
    pub next_suggestions: Vec<NextSuggestion>,
    pub metacognitive_context: MetacognitiveContext,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalSourceCheck {
    pub source: String,
    pub result: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnresolvedGap {
    pub gap: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextSuggestion {
    pub action: String,
    pub priority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetacognitiveContext {
    pub decision_rationale: String,
    pub alternatives_discarded: Vec<RejectedAlternative>,
    pub knowledge_gaps: Vec<String>,
    pub tool_anomalies: Vec<ToolAnomaly>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectedAlternative {
    pub approach: String,
    pub reason_rejected: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolAnomaly {
    pub tool: String,
    pub anomaly: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateReport {
    pub run_id: String,
    pub mode: WorkMode,
    pub pass: bool,
    pub score: f32,
    pub checks: Vec<GateCheck>,
    pub next_actions: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateCheck {
    pub name: String,
    pub pass: bool,
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffDoc {
    pub schema_version: String,
    pub run_id: String,
    pub exit_type: ExitType,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub artifacts: Vec<String>,
    pub notes: Vec<String>,
}
