//! Evaluation: run a fixed question set through the same pipeline, export RAGAS data, score it.

use std::collections::{HashMap, HashSet};

use anyhow::{Context as _, Result};
use futures_util::{StreamExt as _, TryStreamExt as _, stream};
use serde::Deserialize;
use swiftide::query::evaluators::ragas::{EvaluationDataSet, Ragas};

use crate::{
    judge::{Judge, Scores},
    query::{Answer, DocsQa},
};

/// How many answers the judge scores at the same time.
const JUDGE_CONCURRENCY: usize = 4;

/// A question with the answer we expect, as written in `questions.yaml`.
#[derive(Debug, Clone, Deserialize)]
pub struct EvalQuestion {
    pub question: String,
    pub ground_truth: String,
}

/// The questions to evaluate, in the order they are reported.
#[derive(Debug, Clone)]
pub struct QuestionSet(Vec<EvalQuestion>);

impl QuestionSet {
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        let questions: Vec<EvalQuestion> =
            serde_yaml_ng::from_str(yaml).context("Invalid questions file")?;
        anyhow::ensure!(!questions.is_empty(), "The questions file has no questions");

        // Answers are matched back to their question by text, so every question must be unique
        let mut seen = HashSet::with_capacity(questions.len());
        if let Some(duplicate) = questions.iter().find(|q| !seen.insert(q.question.as_str())) {
            anyhow::bail!(
                "The question `{}` appears more than once",
                duplicate.question
            );
        }

        Ok(Self(questions))
    }

    /// Where `question` sits in this set, or `None` for a question it does not contain.
    fn position(&self, question: &str) -> Option<usize> {
        self.0.iter().position(|q| q.question == question)
    }

    /// Reorders a RAGAS export into question order. Swiftide exports the rows in hash-map order,
    /// which changes from run to run and makes exports hard to diff.
    fn sort_ragas_json(&self, ragas_json: &str) -> Result<String> {
        let mut rows: Vec<serde_json::Value> =
            serde_json::from_str(ragas_json).context("Swiftide exported invalid RAGAS JSON")?;
        rows.sort_by_cached_key(|row| row["question"].as_str().and_then(|q| self.position(q)));
        Ok(serde_json::to_string_pretty(&rows)?)
    }
}

/// The questions with their ground truths, as Swiftide's Ragas evaluator expects them.
impl From<&QuestionSet> for EvaluationDataSet {
    fn from(questions: &QuestionSet) -> Self {
        questions
            .0
            .iter()
            .map(|q| (q.question.clone(), q.ground_truth.clone()))
            .collect::<Vec<_>>()
            .into()
    }
}

/// One evaluated question.
pub struct EvalRow {
    pub answer: Answer,
    pub scores: Scores,
}

/// The outcome of an evaluation run.
pub struct Evaluation {
    pub rows: Vec<EvalRow>,
    /// The dataset in the JSON format the Python `ragas` toolkit reads.
    pub ragas_json: String,
}

pub async fn evaluate(qa: &DocsQa, questions: QuestionSet, judge: &Judge) -> Result<Evaluation> {
    // ANCHOR: eval
    let ragas = Ragas::from_prepared_questions(&questions);

    let answered = qa
        .evaluated_pipeline(ragas.clone())
        .query_all(ragas.questions().await)
        .await?;

    let ragas_json = ragas.to_json().await;
    // ANCHOR_END: eval
    let ragas_json = questions.sort_ragas_json(&ragas_json)?;

    // Answers arrive in completion order; report them in question order.
    let mut by_question: HashMap<String, Answer> = answered
        .into_iter()
        .map(|query| (query.original().to_string(), Answer::from(query)))
        .collect();

    let rows = stream::iter(questions.0)
        .map(|question| {
            let answer = by_question.remove(&question.question);
            async move {
                let answer =
                    answer.with_context(|| format!("No answer for `{}`", question.question))?;
                let scores = judge.score(&answer, &question.ground_truth).await?;
                anyhow::Ok(EvalRow { answer, scores })
            }
        })
        .buffered(JUDGE_CONCURRENCY)
        .try_collect()
        .await?;

    Ok(Evaluation { rows, ragas_json })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_questions_are_rejected() {
        let yaml =
            "- {question: Why?, ground_truth: Because.}\n- {question: Why?, ground_truth: So.}";

        let error = QuestionSet::from_yaml(yaml).unwrap_err();

        assert!(error.to_string().contains("more than once"), "{error}");
    }

    #[test]
    fn the_ragas_export_follows_question_order() {
        let questions = QuestionSet::from_yaml(
            "- {question: First?, ground_truth: One.}\n- {question: Second?, ground_truth: Two.}",
        )
        .unwrap();
        let export =
            r#"[{"question": "Second?", "answer": "B"}, {"question": "First?", "answer": "A"}]"#;

        let sorted: Vec<serde_json::Value> =
            serde_json::from_str(&questions.sort_ragas_json(export).unwrap()).unwrap();

        let order: Vec<_> = sorted.iter().map(|row| &row["question"]).collect();
        assert_eq!(order, ["First?", "Second?"]);
    }

    #[test]
    fn the_bundled_questions_are_valid() {
        QuestionSet::from_yaml(include_str!("../questions.yaml")).unwrap();
    }
}
