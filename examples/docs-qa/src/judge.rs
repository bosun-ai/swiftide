//! A small LLM judge that scores answers with RAGAS-style, claim-based metrics.
//!
//! The Ragas evaluator in Swiftide exports a dataset for the Python `ragas` toolkit; it does not
//! compute scores itself. To print real numbers without leaving Rust, the judge breaks the
//! reference answer and the generated answer into claims, checks each one, and the scores are
//! simple ratios over those checks.

use std::{fmt, future::Future, time::Duration};

use anyhow::Result;
use schemars::JsonSchema;
use serde::Deserialize;
use swiftide::{
    chat_completion::errors::LanguageModelError, integrations::openai::OpenAI, prompt::Prompt,
    traits::StructuredPrompt as _,
};

use crate::query::Answer;

/// Swiftide's backoff decorator does not wrap `StructuredPrompt`, so the judge retries transient
/// errors itself, with a linearly growing delay.
const MAX_ATTEMPTS: u32 = 5;
const RETRY_DELAY: Duration = Duration::from_secs(15);

const JUDGE_PROMPT: &str = "You grade an answer produced by a retrieval-augmented system, the \
way RAGAS does.

## Question
{{ question }}

## Reference answer
{{ ground_truth }}

## Generated answer
{{ answer }}

## Retrieved context
{{ contexts }}

## Task
1. Split the reference answer into its atomic factual claims. For each claim decide whether the \
generated answer states it (`in_answer`) and whether the retrieved context supports it \
(`in_context`).
2. Split the generated answer into its atomic factual claims, ignoring formatting and \
restatements of the question. For each claim decide whether the retrieved context supports it \
(`supported_by_context`).

Paraphrases count as a match; guesses and partial matches do not.";

/// A fraction between 0 and 1, shown with two decimals.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Score(f64);

impl Score {
    fn ratio(hits: usize, total: usize) -> Self {
        if total == 0 {
            return Self(0.0);
        }
        #[allow(clippy::cast_precision_loss)]
        Self(hits as f64 / total as f64)
    }

    pub fn mean(scores: impl ExactSizeIterator<Item = Self>) -> Self {
        let count = scores.len();
        if count == 0 {
            return Self(0.0);
        }
        #[allow(clippy::cast_precision_loss)]
        Self(scores.map(|score| score.0).sum::<f64>() / count as f64)
    }
}

impl fmt::Display for Score {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}", self.0)
    }
}

/// RAGAS-style metrics for one answered question.
#[derive(Debug, Clone, Copy)]
pub struct Scores {
    /// Share of the answer's claims that the retrieved context supports.
    pub faithfulness: Score,
    /// Share of the reference answer's claims that the answer states.
    pub answer_recall: Score,
    /// Share of the reference answer's claims that the retrieved context supports.
    pub context_recall: Score,
}

// ANCHOR: judge_schema
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Judgement {
    reference_claims: Vec<ReferenceClaim>,
    answer_claims: Vec<AnswerClaim>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReferenceClaim {
    // Never read; asking for the claim first makes the model commit to it before judging it.
    #[expect(dead_code)]
    claim: String,
    in_answer: bool,
    in_context: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AnswerClaim {
    #[expect(dead_code)]
    claim: String,
    supported_by_context: bool,
}
// ANCHOR_END: judge_schema

impl From<&Judgement> for Scores {
    fn from(judgement: &Judgement) -> Self {
        let answer = &judgement.answer_claims;
        let reference = &judgement.reference_claims;

        Self {
            faithfulness: Score::ratio(
                answer
                    .iter()
                    .filter(|claim| claim.supported_by_context)
                    .count(),
                answer.len(),
            ),
            answer_recall: Score::ratio(
                reference.iter().filter(|claim| claim.in_answer).count(),
                reference.len(),
            ),
            context_recall: Score::ratio(
                reference.iter().filter(|claim| claim.in_context).count(),
                reference.len(),
            ),
        }
    }
}

/// Grades answers with a structured prompt.
pub struct Judge {
    llm: OpenAI,
}

impl Judge {
    pub fn new(llm: OpenAI) -> Self {
        Self { llm }
    }

    pub async fn score(&self, answer: &Answer, ground_truth: &str) -> Result<Scores> {
        // ANCHOR: judge
        let contexts = answer.contexts().collect::<Vec<_>>().join("\n---\n");
        let prompt = Prompt::from(JUDGE_PROMPT)
            .with_context_value("question", answer.question())
            .with_context_value("ground_truth", ground_truth)
            .with_context_value("answer", answer.text())
            .with_context_value("contexts", contexts);

        // The JSON schema of `Judgement` is sent along; the reply is parsed into it
        let judgement: Judgement =
            retry_transient(|| self.llm.structured_prompt(prompt.clone())).await?;
        // ANCHOR_END: judge
        tracing::debug!(?judgement, question = answer.question(), "Judged answer");

        Ok(Scores::from(&judgement))
    }
}

async fn retry_transient<T, F, Fut>(mut call: F) -> Result<T, LanguageModelError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, LanguageModelError>>,
{
    let mut attempt = 1;
    loop {
        match call().await {
            Err(LanguageModelError::TransientError(error)) if attempt < MAX_ATTEMPTS => {
                tracing::warn!(%error, attempt, "Judge failed, retrying");
                tokio::time::sleep(RETRY_DELAY * attempt).await;
                attempt += 1;
            }
            result => return result,
        }
    }
}
