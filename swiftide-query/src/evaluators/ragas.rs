//! The Ragas evaluator allows you to export a RAGAS compatible JSON dataset.
//!
//! RAGAS requires a ground truth to compare to. You can either record the answers for an initial
//! dataset, or provide the ground truth yourself.
//!
//! Refer to the ragas documentation on how to use the dataset or take a look at a more involved
//! example at [swiftide-tutorials](https://github.com/bosun-ai/swiftide-tutorial).
//!
//! # Example
//!
//! ```ignore
//! # use swiftide_query::*;
//! # use anyhow::{Result, Context};
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//!
//! let openai = swiftide::integrations::openai::OpenAi::default();
//! let qdrant = swiftide::integrations::qdrant::Qdrant::default();
//!
//! let ragas = evaluators::ragas::Ragas::from_prepared_questions(questions);
//!
//! let pipeline = query::Pipeline::default()
//! .evaluate_with(ragas.clone())
//! .then_transform_query(query_transformers::GenerateSubquestions::from_client(openai.clone()))
//! .then_transform_query(query_transformers::Embed::from_client(
//! openai.clone(),
//! ))
//! .then_retrieve(qdrant.clone())
//! .then_answer(answers::Simple::from_client(openai.clone()));
//!
//! pipeline.query_all(ragas.questions().await).await.unwrap();
//!
//! std::fs::write("output.json", ragas.to_json().await).unwrap();
//! # Ok(())
//! # }
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::HashMap, str::FromStr, sync::Arc};
use tokio::sync::RwLock;

use swiftide_core::{
    querying::{states, Query, QueryEvaluation},
    EvaluateQuery,
};

/// Ragas evaluator to be used in a pipeline
#[derive(Debug, Clone)]
pub struct Ragas {
    dataset: Arc<RwLock<EvaluationDataSet>>,
}

/// Row structure for RAGAS compatible JSON
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvaluationData {
    question: String,
    answer: String,
    contexts: Vec<String>,
    ground_truth: String,
}

/// Dataset for RAGAS compatible JSON, indexed by question
#[derive(Debug, Clone)]
pub struct EvaluationDataSet(HashMap<String, EvaluationData>);

impl Ragas {
    /// Builds a new Ragas evaluator from a list of questions or a list of tuples with questions and
    /// ground truths. You can also call `parse` to load a dataset from a JSON string.
    pub fn from_prepared_questions(questions: impl Into<EvaluationDataSet>) -> Self {
        Ragas {
            dataset: Arc::new(RwLock::new(questions.into())),
        }
    }

    pub async fn questions(&self) -> Vec<Query<states::Pending>> {
        self.dataset.read().await.0.keys().map(Into::into).collect()
    }

    /// Records the current answers as ground truths in the dataset
    pub async fn record_answers_as_ground_truth(&self) {
        self.dataset.write().await.record_answers_as_ground_truth();
    }

    /// Outputs the dataset as a JSON string compatible with RAGAS
    pub async fn to_json(&self) -> String {
        self.dataset.read().await.to_json()
    }
}

#[async_trait]
impl EvaluateQuery for Ragas {
    #[tracing::instrument(skip_all)]
    async fn evaluate(&self, query: QueryEvaluation) -> Result<()> {
        let mut dataset = self.dataset.write().await;
        dataset.upsert_evaluation(&query)
    }
}

impl EvaluationDataSet {
    pub(crate) fn record_answers_as_ground_truth(&mut self) {
        for data in self.0.values_mut() {
            data.ground_truth.clone_from(&data.answer);
        }
    }

    pub(crate) fn upsert_evaluation(&mut self, query: &QueryEvaluation) -> Result<()> {
        match query {
            QueryEvaluation::RetrieveDocuments(query) => self.upsert_retrieved_documents(query),
            QueryEvaluation::AnswerQuery(query) => self.upsert_answer(query),
        }
    }

    // For each upsort, check if it exists and update it, or return an error
    fn upsert_retrieved_documents(&mut self, query: &Query<states::Retrieved>) -> Result<()> {
        let question = query.original();
        let data = self
            .0
            .get_mut(question)
            .ok_or_else(|| anyhow::anyhow!("Question not found"))?;

        data.contexts = query
            .documents()
            .iter()
            .map(|d| d.content().to_string())
            .collect::<Vec<_>>();
        Ok(())
    }

    fn upsert_answer(&mut self, query: &Query<states::Answered>) -> Result<()> {
        let question = query.original();
        let data = self
            .0
            .get_mut(question)
            .ok_or_else(|| anyhow::anyhow!("Question not found"))?;

        data.answer = query.answer().to_string();

        Ok(())
    }

    /// Outputs json for ragas
    ///
    /// # Format
    ///
    /// ```json
    /// [
    ///   {
    ///   "question": "What is the capital of France?",
    ///   "answer": "Paris",
    ///   "contexts": ["Paris is the capital of France"],
    ///   "ground_truth": "Paris"
    ///   },
    ///   {
    ///   "question": "What is the capital of France?",
    ///   "answer": "Paris",
    ///   "contexts": ["Paris is the capital of France"],
    ///   "ground_truth": "Paris"
    ///   }
    /// ]
    /// ```
    pub(crate) fn to_json(&self) -> String {
        let json_value = json!(self.0.values().collect::<Vec<_>>());
        serde_json::to_string_pretty(&json_value).unwrap_or_else(|_| json_value.to_string())
    }
}

// Can just do a list of questions leaving ground truth, answers, contexts empty
impl From<Vec<String>> for EvaluationDataSet {
    fn from(val: Vec<String>) -> Self {
        EvaluationDataSet(
            val.into_iter()
                .map(|question| {
                    (
                        question.clone(),
                        EvaluationData {
                            question,
                            ..EvaluationData::default()
                        },
                    )
                })
                .collect(),
        )
    }
}

impl From<&[String]> for EvaluationDataSet {
    fn from(val: &[String]) -> Self {
        EvaluationDataSet(
            val.iter()
                .map(|question| {
                    (
                        question.to_string(),
                        EvaluationData {
                            question: question.to_string(),
                            ..EvaluationData::default()
                        },
                    )
                })
                .collect(),
        )
    }
}

// Can take a list of tuples for questions and ground truths
impl From<Vec<(String, String)>> for EvaluationDataSet {
    fn from(val: Vec<(String, String)>) -> Self {
        EvaluationDataSet(
            val.into_iter()
                .map(|(question, ground_truth)| {
                    (
                        question.clone(),
                        EvaluationData {
                            question,
                            ground_truth,
                            ..EvaluationData::default()
                        },
                    )
                })
                .collect(),
        )
    }
}

/// Parse an existing dataset from a JSON string
impl FromStr for EvaluationDataSet {
    type Err = serde_json::Error;

    fn from_str(val: &str) -> std::prelude::v1::Result<Self, Self::Err> {
        let data: Vec<EvaluationData> = serde_json::from_str(val)?;
        Ok(EvaluationDataSet(
            data.into_iter()
                .map(|data| (data.question.clone(), data))
                .collect(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use swiftide_core::querying::{Query, QueryEvaluation};
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn test_ragas_from_prepared_questions() {
        let questions = vec!["What is Rust?".to_string(), "What is Tokio?".to_string()];
        let ragas = Ragas::from_prepared_questions(questions.clone());

        let stored_questions = ragas.questions().await;
        assert_eq!(stored_questions.len(), questions.len());

        for question in questions {
            assert!(stored_questions.iter().any(|q| q.original() == question));
        }
    }

    #[tokio::test]
    async fn test_ragas_record_answers_as_ground_truth() {
        let dataset = Arc::new(RwLock::new(EvaluationDataSet::from(vec![(
            "What is Rust?".to_string(),
            "A programming language".to_string(),
        )])));
        let ragas = Ragas {
            dataset: dataset.clone(),
        };

        {
            let mut lock = dataset.write().await;
            let data = lock.0.get_mut("What is Rust?").unwrap();
            data.answer = "A systems programming language".to_string();
        }

        ragas.record_answers_as_ground_truth().await;

        let updated_data = ragas.dataset.read().await;
        let data = updated_data.0.get("What is Rust?").unwrap();
        assert_eq!(data.ground_truth, "A systems programming language");
    }

    #[tokio::test]
    async fn test_ragas_to_json() {
        let dataset = EvaluationDataSet::from(vec![(
            "What is Rust?".to_string(),
            "A programming language".to_string(),
        )]);
        let ragas = Ragas {
            dataset: Arc::new(RwLock::new(dataset)),
        };

        let json_output = ragas.to_json().await;
        let expected_json = "[\n  {\n    \"answer\": \"\",\n    \"contexts\": [],\n    \"ground_truth\": \"A programming language\",\n    \"question\": \"What is Rust?\"\n  }\n]";
        assert_eq!(json_output, expected_json);
    }

    #[tokio::test]
    async fn test_evaluate_query_upsert_retrieved_documents() {
        let dataset = EvaluationDataSet::from(vec!["What is Rust?".to_string()]);
        let ragas = Ragas {
            dataset: Arc::new(RwLock::new(dataset.clone())),
        };

        let query = Query::builder()
            .original("What is Rust?")
            .documents(vec!["Rust is a language".into()])
            .build()
            .unwrap();
        let evaluation = QueryEvaluation::RetrieveDocuments(query.clone());

        ragas.evaluate(evaluation).await.unwrap();

        let updated_data = ragas.dataset.read().await;
        let data = updated_data.0.get("What is Rust?").unwrap();
        assert_eq!(data.contexts, vec!["Rust is a language"]);
    }

    #[tokio::test]
    async fn test_evaluate_query_upsert_answer() {
        let dataset = EvaluationDataSet::from(vec!["What is Rust?".to_string()]);
        let ragas = Ragas {
            dataset: Arc::new(RwLock::new(dataset.clone())),
        };

        let query = Query::builder()
            .original("What is Rust?")
            .current("A systems programming language")
            .build()
            .unwrap();
        let evaluation = QueryEvaluation::AnswerQuery(query.clone());

        ragas.evaluate(evaluation).await.unwrap();

        let updated_data = ragas.dataset.read().await;
        let data = updated_data.0.get("What is Rust?").unwrap();
        assert_eq!(data.answer, "A systems programming language");
    }

    #[tokio::test]
    async fn test_evaluation_dataset_record_answers_as_ground_truth() {
        let mut dataset = EvaluationDataSet::from(vec!["What is Rust?".to_string()]);
        let data = dataset.0.get_mut("What is Rust?").unwrap();
        data.answer = "A programming language".to_string();

        dataset.record_answers_as_ground_truth();

        let data = dataset.0.get("What is Rust?").unwrap();
        assert_eq!(data.ground_truth, "A programming language");
    }

    #[tokio::test]
    async fn test_evaluation_dataset_to_json() {
        let dataset = EvaluationDataSet::from(vec![(
            "What is Rust?".to_string(),
            "A programming language".to_string(),
        )]);

        let json_output = dataset.to_json();
        let expected_json = "[\n  {\n    \"answer\": \"\",\n    \"contexts\": [],\n    \"ground_truth\": \"A programming language\",\n    \"question\": \"What is Rust?\"\n  }\n]";
        assert_eq!(json_output, expected_json);
    }

    #[tokio::test]
    async fn test_evaluation_dataset_upsert_retrieved_documents() {
        let mut dataset = EvaluationDataSet::from(vec!["What is Rust?".to_string()]);

        let query = Query::builder()
            .original("What is Rust?")
            .documents(vec!["Rust is a language".into()])
            .build()
            .unwrap();
        dataset
            .upsert_evaluation(&QueryEvaluation::RetrieveDocuments(query.clone()))
            .unwrap();

        let data = dataset.0.get("What is Rust?").unwrap();
        assert_eq!(data.contexts, vec!["Rust is a language"]);
    }

    #[tokio::test]
    async fn test_evaluation_dataset_upsert_answer() {
        let mut dataset = EvaluationDataSet::from(vec!["What is Rust?".to_string()]);

        let query = Query::builder()
            .original("What is Rust?")
            .current("A systems programming language")
            .build()
            .unwrap();
        dataset
            .upsert_evaluation(&QueryEvaluation::AnswerQuery(query.clone()))
            .unwrap();

        let data = dataset.0.get("What is Rust?").unwrap();
        assert_eq!(data.answer, "A systems programming language");
    }
}
