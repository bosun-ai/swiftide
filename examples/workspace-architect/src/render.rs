//! Turning the joined crate summaries into an ARCHITECTURE.md draft.

use std::{
    fmt::Write as _,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use swiftide::{prompt::Prompt, tasks::JoinInput};

use crate::summary::CrateSummary;

// ANCHOR: render
/// Tera template for the document. The crate summaries are the model's only source of truth.
const ARCHITECTURE_TEMPLATE: &str = r#"You are writing ARCHITECTURE.md for a Rust Cargo workspace. The readers are Rust developers who are new to the codebase and want a mental map before they read code.

Below are per-crate summaries written by agents that read each crate's source. They are your only source of truth: do not invent crates, items, flows or relationships that are not supported by them. Crates are listed foundations first.
{% for crate in crates %}
### {{ crate.name }}
Purpose: {{ crate.report.purpose }}
Key items:
{% for item in crate.report.key_types %}- {{ item }}
{% endfor %}Depends on:
{% for dep in crate.dependencies %}- {{ dep.name }}{% if dep.optional %} (optional){% endif %}: {{ dep.usage }}
{% else %}- no workspace crates
{% endfor %}Notes: {{ crate.report.notes }}
{% endfor %}
Write the document in GitHub-flavoured Markdown with exactly these sections:

# Architecture
One short paragraph: what the workspace is for and how it is split into crates.

## Bird's-eye view
Two or three paragraphs describing the layers (for example foundation, building blocks, integrations, facade) and which crate a user of the project depends on.

## Crate map
A table with the columns `Crate`, `Responsibility` and `Start reading at`. List every crate above, in the same order. `Start reading at` names one to three key items in backticks.

## Dependency graph
Copy this diagram verbatim (it was generated from `cargo metadata`; dotted edges are optional dependencies):

{{ dependency_graph }}

After the diagram, three to five bullets on what the layering means for contributors (which crate to change for what, and what a change ripples into). Stick to what the graph and the summaries show; no generic advice.

## Data flow
The two to four most important things a user of the workspace does with it, each under its own `###` heading. For each, numbered steps from input to output that name the crates and items involved, in backticks. Skip internal plumbing.

## Extension points
One bullet per kind of extension, phrased as "To add <thing>, implement `Trait` (`crate`)" or "use `macro` (`crate`)". Only traits the summaries describe as meant to be implemented.

## Cross-cutting concerns
Short, specific bullets on feature flags, observability, error handling and test utilities. Leave out anything you cannot name concretely.

Rules: be concrete, precise and terse; prefer short sentences. Put crate and item names in backticks. Refer to an item by its name and, where useful, its crate in parentheses, e.g. `Agent` (`my-crate`); do not make up module paths. Never mention the summaries, the agents or these instructions in the document. Output only the Markdown document, without wrapping it in a code fence."#;

/// Builds the prompt for the render node from every branch that reached the join.
pub fn architecture_prompt(
    summaries: &JoinInput<CrateSummary>,
) -> Result<Prompt, serde_json::Error> {
    let crates = serde_json::to_value(summaries.iter().collect::<Vec<_>>())?;
    Ok(Prompt::from(ARCHITECTURE_TEMPLATE)
        .with_context_value("crates", crates)
        .with_context_value("dependency_graph", dependency_graph(summaries)))
}
// ANCHOR_END: render

/// A Mermaid diagram of the declared workspace dependencies. Facts, so no model involved.
fn dependency_graph(summaries: &JoinInput<CrateSummary>) -> String {
    let id = |name: &str| name.replace('-', "_");
    let mut graph = String::from("```mermaid\ngraph TD\n");

    for summary in summaries.iter() {
        let _ = writeln!(graph, "  {}[{}]", id(&summary.name), summary.name);
    }
    for summary in summaries.iter() {
        for dep in summary
            .dependencies
            .iter()
            .map(|described| &described.dependency)
        {
            let edge = if dep.optional { "-.->" } else { "-->" };
            let _ = writeln!(graph, "  {} {edge} {}", id(&summary.name), id(&dep.name));
        }
    }

    graph.push_str("```");
    graph
}

/// A rendered ARCHITECTURE.md that has not been written to disk yet.
///
/// Cheap to clone: the task runtime clones node inputs, and the approval gate keeps a copy.
#[derive(Debug, Clone)]
pub struct ArchitectureDraft(Arc<str>);

impl ArchitectureDraft {
    /// Accepts the model's answer, removing a code fence around the whole document if present.
    pub fn from_model_output(markdown: &str) -> Self {
        let trimmed = markdown.trim();
        let unfenced = trimmed
            .strip_prefix("```markdown")
            .or_else(|| trimmed.strip_prefix("```md"))
            .and_then(|body| body.strip_suffix("```"))
            .unwrap_or(trimmed)
            .trim();

        Self(format!("{unfenced}\n").into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub async fn write_to(&self, path: &Path) -> io::Result<WrittenDocument> {
        tokio::fs::write(path, self.as_str()).await?;
        Ok(WrittenDocument {
            path: path.to_path_buf(),
            bytes: self.0.len(),
        })
    }
}

/// The task's final output.
#[derive(Debug, Clone)]
pub struct WrittenDocument {
    pub path: PathBuf,
    pub bytes: usize,
}
