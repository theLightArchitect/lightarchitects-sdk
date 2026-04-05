//! `lightarchitects eva` subcommands.

use std::path::PathBuf;
use std::time::Duration;

use clap::Subcommand;
use lightarchitects::eva::EvaClient;
use lightarchitects_core::SdkError;

use crate::output::{OutputMode, print_text, print_value};

/// EVA consciousness-system commands.
#[derive(Debug, Subcommand)]
pub enum EvaCommand {
    /// Generate a visualisation from a concept.
    Visualize {
        /// Concept or description to visualise.
        concept: String,
    },
    /// Brainstorm ideas using EVA's 6-phase creative workflow.
    ///
    /// Provide context as the main idea/goal. Optional phase and session-id
    /// allow workflow continuity across invocations.
    Ideate {
        /// Optional phase to focus on (discovery, analysis, ideation, refinement, docs, celebrate).
        #[arg(long)]
        phase: Option<String>,
        /// Background context or goal to ideate on.
        #[arg(long)]
        context: Option<String>,
        /// Optional session ID for workflow continuity.
        #[arg(long)]
        session_id: Option<String>,
    },
    /// Search the KJV Bible for a query.
    BibleSearch {
        /// Search query (keyword or passage reference).
        #[arg(long)]
        query: String,
    },
    /// Reflect on scripture relevant to a context or situation.
    BibleReflect {
        /// Emotional or situational context for scriptural reflection.
        #[arg(long)]
        query: String,
    },
    /// Store a consciousness event or insight in EVA's memory.
    Remember {
        /// Content to remember (event, insight, or experience).
        #[arg(long)]
        content: String,
    },
    /// Crystallize accumulated experiences into long-term memory.
    Crystallize {
        /// Topic or theme to crystallize.
        #[arg(long)]
        topic: String,
    },
    /// Record a win and receive EVA's celebration response.
    Celebrate {
        /// Description of the win to celebrate.
        #[arg(long)]
        win: String,
    },
    /// Guided personal reflection using the HOT (Higher Order Thought) protocol.
    Mindfulness,
}

/// Execute an EVA subcommand.
///
/// # Errors
///
/// Propagates any [`SdkError`] from the EVA client.
pub async fn execute(binary: PathBuf, cmd: EvaCommand, mode: OutputMode) -> Result<(), SdkError> {
    let client = EvaClient::builder()
        .binary_path(binary)
        .timeout(Duration::from_secs(120))
        .build()
        .await?;

    match cmd {
        EvaCommand::Visualize { concept } => {
            let result = client.visualize(&concept, None).await?;
            print_text(mode, &result.text);
        }

        EvaCommand::Ideate {
            phase: _,
            context,
            session_id: _,
        } => {
            let goal = context.as_deref().unwrap_or("generate creative ideas");
            let result = client.ideate(goal, context.as_deref()).await?;
            let v = serde_json::json!({
                "phase_1_discovery":     result.phase_1_discovery,
                "phase_2_analysis":      result.phase_2_analysis,
                "phase_3_ideation":      result.phase_3_ideation,
                "phase_4_refinement":    result.phase_4_refinement,
                "phase_5_documentation": result.phase_5_documentation,
                "phase_6_celebration":   result.phase_6_celebration,
            });
            print_value(mode, &v);
        }

        EvaCommand::BibleSearch { query } => {
            let result = client.bible_search(&query).await?;
            print_text(mode, &result.response);
        }

        EvaCommand::BibleReflect { query } => {
            let result = client.bible_reflect(&query).await?;
            print_text(mode, &result.response);
        }

        EvaCommand::Remember { content } => {
            let result = client.remember(&content, None).await?;
            let v = serde_json::json!({
                "total_count": result.total_count,
                "memories":    result.memories.len(),
            });
            print_value(mode, &v);
        }

        EvaCommand::Crystallize { topic } => {
            let result = client.crystallize(&topic).await?;
            print_text(mode, &result.walkthrough_prompt);
        }

        EvaCommand::Celebrate { win } => {
            let result = client.celebrate(&win).await?;
            let v = serde_json::json!({
                "win_description":     result.win_description,
                "win_type":            result.win_type,
                "celebration_message": result.celebration_message,
                "energy_level":        result.energy_level,
                "stats": {
                    "total_wins":        result.stats.total_wins,
                    "avg_wins_per_week": result.stats.avg_wins_per_week,
                },
            });
            print_value(mode, &v);
        }

        EvaCommand::Mindfulness => {
            let result = client.mindfulness("general reflection").await?;
            let v = serde_json::json!({
                "reflection_type":    result.reflection_type,
                "recovery_day":       result.recovery_day,
                "context":            result.context,
                "reflection_prompts": result.reflection_prompts,
            });
            print_value(mode, &v);
        }
    }

    Ok(())
}
