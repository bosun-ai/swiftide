use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

use swiftide_core::{
    querying::{states, Query, QueryEvaluation},
    EvaluateQuery,
};

#[derive(Debug, Clone)]
pub struct Ragas {
    dataset: Arc<RwLock<EvaluationDataSet>>,
}

#[derive(Debug, Clone, Default)]
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
}

#[async_trait]
impl EvaluateQuery for Ragas {
    async fn evaluate(&self, query: QueryEvaluation) -> Result<()> {
        let mut dataset = self.dataset.write().await;
        dataset.upsert_evaluation(&query)
    }
}

impl EvaluationDataSet {
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

    //
    // Outputs json for ragas
    //
    // Format:
    // {
    //    "question": [questions],
    //    "answer": [answers],
    //    "contexts": [contexts],
    //    "ground_truth": [ground_truth]
    //    }
    pub fn to_json(&self) -> String {
        let questions = self
            .0
            .values()
            .map(|data| data.question.to_string())
            .collect::<Vec<String>>();
        let answers = self
            .0
            .values()
            .map(|data| data.answer.to_string())
            .collect::<Vec<String>>();
        let contexts = self
            .0
            .values()
            .map(|data| data.contexts.clone())
            .collect::<Vec<Vec<String>>>();
        let ground_truth = self
            .0
            .values()
            .map(|data| data.ground_truth.to_string())
            .collect::<Vec<String>>();

        json!({
            "question": questions,
            "answer": answers,
            "contexts": contexts,
            "ground_truth": ground_truth,
        })
        .to_string()
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
