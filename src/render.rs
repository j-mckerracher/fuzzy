use crate::models::*;
use chrono::Utc;

pub fn final_report(
    run: &RunDoc,
    questions: &OpenQuestionsDoc,
    hypotheses: &HypothesisLedgerDoc,
    evidence: &EvidenceLedgerDoc,
    decisions: &DecisionLogDoc,
) -> String {
    let blocking_open = questions
        .questions
        .iter()
        .filter(|q| q.blocking && q.status == QuestionStatus::Open)
        .count();
    let likely = hypotheses
        .hypotheses
        .iter()
        .filter(|h| {
            matches!(
                h.status,
                HypothesisStatus::Likely | HypothesisStatus::Confirmed
            )
        })
        .count();
    let mut out = String::new();
    out.push_str(&format!("# Fuzzy Run Report: {}\n\n", run.title));
    out.push_str(&format!("- Run: `{}`\n", run.id));
    out.push_str(&format!("- Mode: `{:?}`\n", run.mode));
    out.push_str(&format!("- Status: `{:?}`\n", run.status));
    out.push_str(&format!("- Generated: `{}`\n\n", Utc::now().to_rfc3339()));

    out.push_str("## Request\n\n");
    out.push_str(&format!("{}\n\n", run.request.raw));
    out.push_str("## Normalized goal\n\n");
    out.push_str(&format!("{}\n\n", run.request.normalized_goal));

    out.push_str("## Current readout\n\n");
    out.push_str(&format!("- Open blocking questions: `{blocking_open}`\n"));
    out.push_str(&format!(
        "- Questions total: `{}`\n",
        questions.questions.len()
    ));
    out.push_str(&format!(
        "- Evidence entries: `{}`\n",
        evidence.evidence.len()
    ));
    out.push_str(&format!("- Likely/confirmed hypotheses: `{likely}`\n"));
    out.push_str(&format!("- Decisions: `{}`\n\n", decisions.decisions.len()));

    out.push_str("## Open questions\n\n");
    if questions.questions.is_empty() {
        out.push_str("No questions recorded.\n\n");
    } else {
        for q in &questions.questions {
            out.push_str(&format!(
                "- `{}` [{:?}]{} {}\n",
                q.id,
                q.status,
                if q.blocking { " [blocking]" } else { "" },
                q.question
            ));
            if let Some(resolution) = &q.resolution {
                out.push_str(&format!("  - Resolution: {}\n", resolution));
            }
        }
        out.push('\n');
    }

    out.push_str("## Hypotheses\n\n");
    if hypotheses.hypotheses.is_empty() {
        out.push_str("No hypotheses recorded.\n\n");
    } else {
        for h in &hypotheses.hypotheses {
            out.push_str(&format!(
                "- `{}` [{:?}, {:.2}] {}\n",
                h.id, h.status, h.confidence, h.claim
            ));
        }
        out.push('\n');
    }

    out.push_str("## Evidence\n\n");
    if evidence.evidence.is_empty() {
        out.push_str("No evidence recorded.\n\n");
    } else {
        for e in &evidence.evidence {
            out.push_str(&format!("- `{}` [{:?}] {}\n", e.id, e.confidence, e.claim));
            out.push_str(&format!("  - Source: `{}` ({})\n", e.source, e.source_type));
            if let Some(excerpt) = &e.excerpt {
                out.push_str(&format!("  - Excerpt: {}\n", excerpt));
            }
        }
        out.push('\n');
    }

    out.push_str("## Decisions\n\n");
    if decisions.decisions.is_empty() {
        out.push_str("No decisions recorded.\n\n");
    } else {
        for d in &decisions.decisions {
            out.push_str(&format!("- `{}` {}\n", d.id, d.decision));
            if let Some(rationale) = &d.rationale {
                out.push_str(&format!("  - Rationale: {}\n", rationale));
            }
        }
        out.push('\n');
    }

    out.push_str("## Suggested next action\n\n");
    if blocking_open > 0 {
        out.push_str(
            "Resolve or explicitly defer blocking questions before remediation or handoff.\n",
        );
    } else if evidence.evidence.is_empty() {
        out.push_str("Gather at least one evidence entry before closing this run.\n");
    } else {
        out.push_str("Run `fuzzy gate` and choose a typed exit, such as diagnosis, decision, escalation, or delivery-story.\n");
    }
    out
}

pub fn print_run_summary(
    run: &RunDoc,
    q: &OpenQuestionsDoc,
    h: &HypothesisLedgerDoc,
    e: &EvidenceLedgerDoc,
    d: &DecisionLogDoc,
) {
    let blocking_open = q
        .questions
        .iter()
        .filter(|x| x.blocking && x.status == QuestionStatus::Open)
        .count();
    println!("Run: {}", run.id);
    println!("Title: {}", run.title);
    println!("Mode: {:?}", run.mode);
    println!("Status: {:?}", run.status);
    println!(
        "Open questions: {} (blocking: {})",
        q.questions
            .iter()
            .filter(|x| x.status == QuestionStatus::Open)
            .count(),
        blocking_open
    );
    println!("Hypotheses: {}", h.hypotheses.len());
    println!("Evidence: {}", e.evidence.len());
    println!("Decisions: {}", d.decisions.len());
}
