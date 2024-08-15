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

#[derive(Debug, Clone)]
pub struct Ragas {
    dataset: Arc<RwLock<EvaluationDataSet>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvaluationData {
    question: String,
    answer: String,
    contexts: Vec<String>,
    ground_truth: String,
}

#[derive(Debug, Clone)]
pub struct EvaluationDataSet(HashMap<String, EvaluationData>);

impl Ragas {
    pub fn from_prepared_questions(questions: impl Into<EvaluationDataSet>) -> Self {
        Ragas {
            dataset: Arc::new(RwLock::new(questions.into())),
        }
    }

    pub async fn questions(&self) -> Vec<Query<states::Pending>> {
        self.dataset.read().await.0.keys().map(Into::into).collect()
    }

    pub async fn record_answers_as_ground_truth(&self) {
        self.dataset.write().await.record_answers_as_ground_truth();
    }

    pub async fn to_json(&self) -> String {
        self.dataset.read().await.to_json()
    }
}

#[async_trait]
impl EvaluateQuery for Ragas {
    async fn evaluate(&self, query: QueryEvaluation) -> Result<()> {
        let mut dataset = self.dataset.write().await;
        dataset.upsert_evaluation(&query)
    }
}

impl EvaluationDataSet {
    pub fn record_answers_as_ground_truth(&mut self) {
        for data in self.0.values_mut() {
            data.ground_truth = data.answer.clone();
        }
    }

    pub fn upsert_evaluation(&mut self, query: &QueryEvaluation) -> Result<()> {
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

        data.contexts = query.documents().to_vec();
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
    pub fn to_json(&self) -> String {
        json!(self.0.values().collect::<Vec<_>>()).to_string()
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
