//! Asks the maintainer on the terminal whether to write the proposed notes.
use std::{
    io::{self, BufRead as _, IsTerminal as _, Write as _},
    path::{Path, PathBuf},
};

use anyhow::Result;

use crate::{
    session::{Reviewer, Verdict},
    tools::WriteChangelogArgs,
};

/// Reviews proposals on stdin. The draft itself has already been streamed to the terminal.
pub struct TerminalReviewer {
    notes_file: PathBuf,
}

impl TerminalReviewer {
    pub fn new(notes_file: &Path) -> Self {
        Self {
            notes_file: notes_file.to_path_buf(),
        }
    }

    fn ask(question: &str) -> Result<String> {
        print!("{question}");
        io::stdout().flush()?;
        let mut answer = String::new();
        let stdin = io::stdin();
        stdin.lock().read_line(&mut answer)?;
        let answer = answer.trim();
        // Piped answers are not echoed by the terminal; echo them so the transcript reads well.
        if !stdin.is_terminal() {
            println!("{answer}");
        }
        Ok(answer.to_owned())
    }
}

impl Reviewer for TerminalReviewer {
    fn review(&mut self, proposal: &WriteChangelogArgs) -> Result<Verdict> {
        let placement = if self.notes_file.exists() {
            "above the existing notes in"
        } else {
            "into a new file,"
        };
        println!(
            "\n── proposed: {} lines {placement} {}",
            proposal.markdown.lines().count(),
            self.notes_file.display()
        );
        let answer = Self::ask("Write these release notes? [y/N] ")?;
        if answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes") {
            return Ok(Verdict::Approve);
        }

        let feedback = Self::ask("What should change? (empty to stop) ")?;
        Ok(if feedback.is_empty() {
            Verdict::Reject
        } else {
            Verdict::Revise(feedback)
        })
    }
}
