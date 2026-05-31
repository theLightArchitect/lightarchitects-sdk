//! CLI transport for the `question` tool — presents `inquire` prompts on the
//! operator's terminal when running inside Claude Code or another interactive
//! CLI session.
//!
//! Invoked from [`super::question::run`] when the `CLAUDE_CODE_INTERACTIVE`
//! environment variable is set.  Falls back to headless policy when the
//! terminal is unavailable (e.g. CI, piped stdin).

use inquire::{InquireError, MultiSelect, Select};

use crate::error::GatewayError;

use super::question::{QuestionAnswer, QuestionInput};

/// Drive the operator through all questions in `input` via terminal prompts.
///
/// Returns a [`QuestionAnswer`] with one `Vec<String>` per question.
/// Single-select questions use [`inquire::Select`]; multi-select questions
/// use [`inquire::MultiSelect`].
///
/// # Errors
///
/// Returns [`GatewayError::InvalidRequest`] when the user cancels a prompt
/// (Ctrl-C / ESC) or when the terminal is unavailable.
pub fn prompt_cli(input: &QuestionInput) -> Result<QuestionAnswer, GatewayError> {
    let mut answers: Vec<Vec<String>> = Vec::with_capacity(input.questions.len());

    for q in &input.questions {
        let labels: Vec<String> = q.options.iter().map(|o| o.label.clone()).collect();
        let help = format!("[{}]", q.header);

        let selected: Vec<String> = if q.multi_select {
            MultiSelect::new(q.question.as_str(), labels)
                .with_help_message(help.as_str())
                .prompt()
                .map_err(map_inquire_err)?
        } else {
            let choice = Select::new(q.question.as_str(), labels)
                .with_help_message(help.as_str())
                .prompt()
                .map_err(map_inquire_err)?;
            vec![choice]
        };

        answers.push(selected);
    }

    Ok(QuestionAnswer { answers })
}

fn map_inquire_err(e: InquireError) -> GatewayError {
    GatewayError::InvalidRequest(format!("question: cli prompt aborted: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_tools::question::{HeadlessPolicy, Question, QuestionOption};

    fn make_input(multi: bool) -> QuestionInput {
        QuestionInput {
            questions: vec![Question {
                question: "What should we do?".to_owned(),
                header: "Decision".to_owned(),
                multi_select: multi,
                options: vec![
                    QuestionOption {
                        label: "Proceed".to_owned(),
                        description: "Continue".to_owned(),
                    },
                    QuestionOption {
                        label: "Abort".to_owned(),
                        description: "Stop".to_owned(),
                    },
                ],
            }],
            headless_policy: Some(HeadlessPolicy::AutoFirst),
        }
    }

    #[test]
    fn input_with_two_questions_has_correct_structure() {
        let input = QuestionInput {
            questions: vec![
                Question {
                    question: "Q1".to_owned(),
                    header: "H1".to_owned(),
                    multi_select: false,
                    options: vec![QuestionOption {
                        label: "Yes".to_owned(),
                        description: String::new(),
                    }],
                },
                Question {
                    question: "Q2".to_owned(),
                    header: "H2".to_owned(),
                    multi_select: true,
                    options: vec![
                        QuestionOption {
                            label: "A".to_owned(),
                            description: String::new(),
                        },
                        QuestionOption {
                            label: "B".to_owned(),
                            description: String::new(),
                        },
                    ],
                },
            ],
            headless_policy: None,
        };
        assert_eq!(input.questions.len(), 2);
        assert!(!input.questions[0].multi_select);
        assert!(input.questions[1].multi_select);
    }

    #[test]
    fn make_input_helper_single_select() {
        let input = make_input(false);
        assert!(!input.questions[0].multi_select);
        assert_eq!(input.questions[0].options.len(), 2);
    }

    #[test]
    fn make_input_helper_multi_select() {
        let input = make_input(true);
        assert!(input.questions[0].multi_select);
    }
}
