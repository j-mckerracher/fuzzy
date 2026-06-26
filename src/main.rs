mod adapters;
mod approval;
mod budget;
mod chat;
mod commands;
mod context;
mod llm;
mod models;
mod ops;
mod owl;
mod protocol;
mod redaction;
mod render;
mod repl;
mod store;
mod tools;
mod transcript;
mod util;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use models::{BannerMode, Confidence, ExitType, HypothesisStatus, WorkMode};
use std::path::PathBuf;
use store::Store;

#[derive(Parser, Debug)]
#[command(name = "fuzzy")]
#[command(version)]
#[command(about = "CLI-first harness for uncertain work", long_about = None)]
struct Cli {
    #[arg(
        long,
        global = true,
        help = "Project root containing .fuzzy/config.toml"
    )]
    root: Option<PathBuf>,

    /// Default agent backend (env: FUZZY_AGENT_BACKEND).
    #[arg(long, global = true)]
    agent_backend: Option<String>,

    /// Banner display mode for the interactive shell.
    #[arg(long, global = true, value_enum, default_value_t = BannerMode::Auto)]
    banner: BannerMode,

    /// Suppress the owl banner.
    #[arg(long, global = true)]
    no_banner: bool,

    /// Emit machine-readable JSON where supported (also suppresses the banner).
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: CommandGroup,
}

#[derive(Subcommand, Debug)]
enum CommandGroup {
    /// Initialize fuzzy harness metadata in the current project.
    Init {
        #[arg(long)]
        force: bool,
    },

    /// Check local harness configuration and external adapters.
    Doctor,

    /// Start the interactive LLM-orchestrated chat shell.
    Chat(ChatArgs),

    /// Inspect orchestrator internals (context, envelope parsing).
    Debug {
        #[command(subcommand)]
        command: DebugCommand,
    },

    /// Read or update local configuration.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },

    /// Start a new fuzzy work run.
    Start(StartArgs),

    /// List runs.
    List,

    /// Show run status.
    Status {
        #[arg(long)]
        run: Option<String>,
    },

    /// Print the run directory path.
    Open {
        #[arg(long)]
        run: Option<String>,
    },

    /// Manage open questions.
    Question {
        #[command(subcommand)]
        command: QuestionCommand,
    },

    /// Manage hypotheses.
    Hypothesis {
        #[command(subcommand)]
        command: HypothesisCommand,
    },

    /// Manage evidence.
    Evidence {
        #[command(subcommand)]
        command: EvidenceCommand,
    },

    /// Manage decisions.
    Decision {
        #[command(subcommand)]
        command: DecisionCommand,
    },

    /// Query the Reference Librarian. The librarian owns knowledge access.
    Librarian {
        #[command(subcommand)]
        command: LibrarianCommand,
    },

    /// Run focused Information Explorer probes.
    Explorer {
        #[command(subcommand)]
        command: ExplorerCommand,
    },

    /// Run the uncertainty gate for a fuzzy run.
    Gate {
        #[arg(long)]
        run: Option<String>,
    },

    /// Generate a Markdown final report.
    Report {
        #[arg(long)]
        run: Option<String>,
        #[arg(long)]
        out: Option<PathBuf>,
    },

    /// Record a typed exit such as diagnosis, escalation, or delivery-story.
    Exit {
        #[arg(long)]
        run: Option<String>,
        #[arg(long = "type", value_enum)]
        exit_type: ExitType,
        #[arg(trailing_var_arg = true)]
        note: Vec<String>,
    },

    /// Promote selected artifacts to a downstream system.
    Promote {
        #[command(subcommand)]
        command: PromoteCommand,
    },

    /// Write an example story.json into outputs for testing Workbench handoff.
    ExampleStory {
        #[arg(long)]
        run: Option<String>,
    },

    /// Export a run folder as a tar.gz archive.
    Export {
        #[arg(long)]
        run: Option<String>,
        #[arg(long)]
        output: Option<PathBuf>,
    },

    /// Create a common output artifact template.
    Template {
        #[arg(long)]
        run: Option<String>,
        artifact: String,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCommand {
    Show,
    Set { key: String, value: String },
}

#[derive(Args, Debug)]
struct ChatArgs {
    /// Backend for this chat session (overrides --agent-backend).
    #[arg(long)]
    backend: Option<String>,

    /// Bind the session to an existing run.
    #[arg(long)]
    run: Option<String>,

    /// Run a single turn with the given text, then exit.
    #[arg(long)]
    one_shot: Option<String>,

    /// Validate startup wiring without contacting a backend.
    #[arg(long)]
    dry_start: bool,

    /// Print the assembled context and exit (no backend call).
    #[arg(long)]
    print_context: bool,

    /// Build and print one turn's request without executing it.
    #[arg(long)]
    dry_run_turn: Option<String>,

    /// Banner mode override for this session.
    #[arg(long, value_enum)]
    banner: Option<BannerMode>,

    /// Require interactive confirmation before risky tools (default: auto-approve).
    #[arg(long)]
    require_approval: bool,
}

#[derive(Subcommand, Debug)]
enum DebugCommand {
    /// Print the assembled orchestrator system prompt for a run.
    Context {
        #[arg(long)]
        run: Option<String>,
    },

    /// Parse and validate an action-envelope JSON file.
    Envelope {
        #[arg(long)]
        file: PathBuf,
    },
}

#[derive(Args, Debug)]
struct StartArgs {
    #[arg(long, value_enum)]
    mode: WorkMode,

    #[arg(long)]
    title: Option<String>,

    #[arg(required = true, trailing_var_arg = true)]
    request: Vec<String>,
}

#[derive(Subcommand, Debug)]
enum QuestionCommand {
    Add {
        #[arg(long)]
        run: Option<String>,
        #[arg(long)]
        blocking: bool,
        #[arg(long)]
        owner: Option<String>,
        #[arg(required = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    List {
        #[arg(long)]
        run: Option<String>,
    },
    Resolve {
        #[arg(long)]
        run: Option<String>,
        id: String,
        #[arg(long = "evidence")]
        evidence: Vec<String>,
        #[arg(trailing_var_arg = true)]
        resolution: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
enum HypothesisCommand {
    Add {
        #[arg(long)]
        run: Option<String>,
        #[arg(long, default_value_t = 0.3)]
        confidence: f32,
        #[arg(long)]
        falsification: Option<String>,
        #[arg(required = true, trailing_var_arg = true)]
        claim: Vec<String>,
    },
    Update {
        #[arg(long)]
        run: Option<String>,
        id: String,
        #[arg(long, value_enum)]
        status: Option<HypothesisStatus>,
        #[arg(long)]
        confidence: Option<f32>,
        #[arg(long = "evidence")]
        evidence: Vec<String>,
    },
    List {
        #[arg(long)]
        run: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum EvidenceCommand {
    Add {
        #[arg(long)]
        run: Option<String>,
        #[arg(long, default_value = "manual")]
        source_type: String,
        #[arg(long)]
        source: Option<String>,
        #[arg(long, value_enum, default_value = "medium")]
        confidence: Confidence,
        #[arg(long)]
        excerpt: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long = "used-by")]
        used_by: Vec<String>,
        #[arg(required = true, trailing_var_arg = true)]
        claim: Vec<String>,
    },
    List {
        #[arg(long)]
        run: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum DecisionCommand {
    Add {
        #[arg(long)]
        run: Option<String>,
        #[arg(long)]
        rationale: Option<String>,
        #[arg(long = "evidence")]
        evidence: Vec<String>,
        #[arg(required = true, trailing_var_arg = true)]
        decision: Vec<String>,
    },
    List {
        #[arg(long)]
        run: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum LibrarianCommand {
    Ask {
        #[arg(long)]
        run: Option<String>,
        #[arg(long)]
        force_explorer: bool,
        #[arg(long)]
        no_explorer: bool,
        #[arg(required = true, trailing_var_arg = true)]
        query: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
enum ExplorerCommand {
    Run {
        #[arg(long)]
        run: Option<String>,
        #[arg(long)]
        scope: Option<PathBuf>,
        #[arg(long)]
        record_evidence: bool,
        #[arg(required = true, trailing_var_arg = true)]
        query: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
enum PromoteCommand {
    Workbench {
        #[arg(long)]
        run: Option<String>,
        #[arg(long)]
        story: Option<PathBuf>,
        #[arg(long)]
        exec: bool,
    },
    Openviking {
        #[arg(long)]
        run: Option<String>,
        #[arg(long)]
        exec: bool,
        #[arg(long)]
        wait: bool,
    },
    Autocontext {
        #[arg(long)]
        run: Option<String>,
        #[arg(long)]
        exec: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        CommandGroup::Init { force } => {
            let root = cli.root.unwrap_or(std::env::current_dir()?);
            let store = Store::init_at(root, force)?;
            println!("initialized fuzzy harness at {}", store.root.display());
            println!("config: {}", store.config_path().display());
        }
        CommandGroup::Doctor => {
            let store = Store::open(cli.root).ok();
            commands::cmd_doctor(store.as_ref())?;
        }
        CommandGroup::Chat(args) => {
            let opts = repl::ChatOptions {
                root: cli.root,
                backend: args.backend,
                agent_backend: cli.agent_backend,
                run: args.run,
                one_shot: args.one_shot,
                dry_start: args.dry_start,
                print_context: args.print_context,
                dry_run_turn: args.dry_run_turn,
                banner: args.banner.unwrap_or(cli.banner),
                no_banner: cli.no_banner,
                json: cli.json,
                approvals: !args.require_approval,
            };
            repl::run_chat(opts)?;
        }
        CommandGroup::Debug { command } => match command {
            DebugCommand::Context { run } => repl::debug_context(cli.root, run)?,
            DebugCommand::Envelope { file } => repl::debug_envelope(file)?,
        },
        CommandGroup::Config { command } => {
            let store = Store::open(cli.root)?;
            match command {
                ConfigCommand::Show => commands::cmd_config_show(&store)?,
                ConfigCommand::Set { key, value } => commands::cmd_config_set(store, key, value)?,
            }
        }
        CommandGroup::Start(args) => {
            let store = Store::open(cli.root)?;
            commands::cmd_start(&store, args.mode, args.title, args.request)?;
        }
        CommandGroup::List => {
            let store = Store::open(cli.root)?;
            commands::cmd_list(&store)?;
        }
        CommandGroup::Status { run } => {
            let store = Store::open(cli.root)?;
            commands::cmd_status(&store, run)?;
        }
        CommandGroup::Open { run } => {
            let store = Store::open(cli.root)?;
            commands::cmd_open_path(&store, run)?;
        }
        CommandGroup::Question { command } => {
            let store = Store::open(cli.root)?;
            match command {
                QuestionCommand::Add {
                    run,
                    text,
                    blocking,
                    owner,
                } => commands::cmd_question_add(&store, run, text, blocking, owner)?,
                QuestionCommand::List { run } => commands::cmd_question_list(&store, run)?,
                QuestionCommand::Resolve {
                    run,
                    id,
                    resolution,
                    evidence,
                } => commands::cmd_question_resolve(&store, run, id, resolution, evidence)?,
            }
        }
        CommandGroup::Hypothesis { command } => {
            let store = Store::open(cli.root)?;
            match command {
                HypothesisCommand::Add {
                    run,
                    claim,
                    confidence,
                    falsification,
                } => commands::cmd_hypothesis_add(&store, run, claim, confidence, falsification)?,
                HypothesisCommand::Update {
                    run,
                    id,
                    status,
                    confidence,
                    evidence,
                } => {
                    commands::cmd_hypothesis_update(&store, run, id, status, confidence, evidence)?
                }
                HypothesisCommand::List { run } => commands::cmd_hypothesis_list(&store, run)?,
            }
        }
        CommandGroup::Evidence { command } => {
            let store = Store::open(cli.root)?;
            match command {
                EvidenceCommand::Add {
                    run,
                    claim,
                    source_type,
                    source,
                    confidence,
                    excerpt,
                    file,
                    used_by,
                } => commands::cmd_evidence_add(
                    &store,
                    run,
                    claim,
                    source_type,
                    source,
                    confidence,
                    excerpt,
                    file,
                    used_by,
                )?,
                EvidenceCommand::List { run } => commands::cmd_evidence_list(&store, run)?,
            }
        }
        CommandGroup::Decision { command } => {
            let store = Store::open(cli.root)?;
            match command {
                DecisionCommand::Add {
                    run,
                    decision,
                    rationale,
                    evidence,
                } => commands::cmd_decision_add(&store, run, decision, rationale, evidence)?,
                DecisionCommand::List { run } => commands::cmd_decision_list(&store, run)?,
            }
        }
        CommandGroup::Librarian { command } => {
            let store = Store::open(cli.root)?;
            match command {
                LibrarianCommand::Ask {
                    run,
                    query,
                    force_explorer,
                    no_explorer,
                } => commands::cmd_librarian_ask(&store, run, query, force_explorer, no_explorer)?,
            }
        }
        CommandGroup::Explorer { command } => {
            let store = Store::open(cli.root)?;
            match command {
                ExplorerCommand::Run {
                    run,
                    query,
                    scope,
                    record_evidence,
                } => commands::cmd_explorer_run(&store, run, query, scope, record_evidence)?,
            }
        }
        CommandGroup::Gate { run } => {
            let store = Store::open(cli.root)?;
            let _ = commands::cmd_gate(&store, run)?;
        }
        CommandGroup::Report { run, out } => {
            let store = Store::open(cli.root)?;
            commands::cmd_report(&store, run, out)?;
        }
        CommandGroup::Exit {
            run,
            exit_type,
            note,
        } => {
            let store = Store::open(cli.root)?;
            commands::cmd_exit(&store, run, exit_type, note)?;
        }
        CommandGroup::Promote { command } => {
            let store = Store::open(cli.root)?;
            match command {
                PromoteCommand::Workbench { run, story, exec } => {
                    commands::cmd_promote_workbench(&store, run, story, exec)?
                }
                PromoteCommand::Openviking { run, exec, wait } => {
                    commands::cmd_promote_openviking(&store, run, exec, wait)?
                }
                PromoteCommand::Autocontext { run, exec } => {
                    commands::cmd_promote_autocontext(&store, run, exec)?
                }
            }
        }
        CommandGroup::ExampleStory { run } => {
            let store = Store::open(cli.root)?;
            commands::write_example_story(&store, run)?;
        }
        CommandGroup::Export { run, output } => {
            let store = Store::open(cli.root)?;
            commands::cmd_export_run(&store, run, output)?;
        }
        CommandGroup::Template { run, artifact } => {
            let store = Store::open(cli.root)?;
            commands::cmd_new_template(&store, run, artifact)?;
        }
    }
    Ok(())
}
