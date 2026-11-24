//! Terminal output for answers and evaluation results.

use std::fmt::Write as _;

use crate::{
    eval::EvalRow,
    judge::{Score, Scores},
    query::Answer,
};

const QUESTION_WIDTH: usize = 52;

// ANCHOR: render
/// The answer followed by a numbered list of the pages and sections it was based on.
pub fn answer(answer: &Answer) -> String {
    let mut out = format!("{}\n\nSources:\n", answer.text().trim());
    for (number, source) in answer.sources().iter().enumerate() {
        let _ = writeln!(
            out,
            "  [{}] {} › {}  ({})",
            number + 1,
            source.page,
            source.section,
            source.origin
        );
    }
    out
}
// ANCHOR_END: render

/// A compact table with one row per question and the mean scores at the bottom.
pub fn eval_table(rows: &[EvalRow]) -> String {
    let mut out = format!(
        "{:<3} {:<QUESTION_WIDTH$} {:>4} {:>8} {:>8} {:>8}\n",
        "#", "question", "ctx", "faithful", "answer", "context"
    );
    let _ = writeln!(
        out,
        "{:<3} {:<QUESTION_WIDTH$} {:>4} {:>8} {:>8} {:>8}",
        "", "", "", "", "recall", "recall"
    );

    for (number, row) in rows.iter().enumerate() {
        let _ = writeln!(
            out,
            "{:<3} {:<QUESTION_WIDTH$} {:>4} {}",
            number + 1,
            truncate(row.answer.question(), QUESTION_WIDTH),
            row.answer.contexts().count(),
            score_columns(&row.scores),
        );
    }

    let mean =
        |metric: fn(&Scores) -> Score| Score::mean(rows.iter().map(|row| metric(&row.scores)));
    let _ = writeln!(
        out,
        "{:<3} {:<QUESTION_WIDTH$} {:>4} {}",
        "",
        "mean",
        "",
        score_columns(&Scores {
            faithfulness: mean(|scores| scores.faithfulness),
            answer_recall: mean(|scores| scores.answer_recall),
            context_recall: mean(|scores| scores.context_recall),
        }),
    );
    out
}

fn score_columns(scores: &Scores) -> String {
    format!(
        "{:>8} {:>8} {:>8}",
        scores.faithfulness.to_string(),
        scores.answer_recall.to_string(),
        scores.context_recall.to_string()
    )
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let mut truncated: String = text.chars().take(width - 1).collect();
    truncated.push('…');
    truncated
}
