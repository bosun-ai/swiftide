//! The release notes the agent produces, and how they end up on disk.
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use swiftide::agents::tools::control::StopWithArgs;

// ANCHOR: stop_schema
/// Structured release notes. The agent hands these over when it calls `stop`.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ReleaseNotes {
    /// The version being released, without a leading `v`.
    pub version: String,
    /// One or two sentences on what this release is about, written for users.
    pub summary: String,
    /// New features and notable improvements.
    pub highlights: Vec<Item>,
    /// Changes that require users to update their code. Empty if there are none.
    pub breaking_changes: Vec<Item>,
    /// Bug fixes users may notice.
    pub fixes: Vec<Item>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct Item {
    /// Short, user-facing title.
    pub title: String,
    /// What changed and why it matters, in one or two sentences.
    pub details: String,
    /// Pull requests or commits as plain text, e.g. `#1207` or `90bcadc7`, not Markdown links.
    pub references: Vec<String>,
}

impl ReleaseNotes {
    /// A `stop` tool whose arguments must match [`ReleaseNotes`].
    ///
    /// With a custom schema the arguments are forwarded as-is, and end up in
    /// `StopReason::RequestedByTool`.
    pub fn stop_tool() -> StopWithArgs {
        StopWithArgs::with_parameters_schema(schema_for!(ReleaseNotes))
    }
}
// ANCHOR_END: stop_schema

// ANCHOR: render
impl fmt::Display for ReleaseNotes {
    /// A compact terminal summary; the full notes are what the agent wrote to disk.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{} — {}", self.version, self.summary)?;
        for (label, items) in [
            ("highlights", &self.highlights),
            ("breaking changes", &self.breaking_changes),
            ("fixes", &self.fixes),
        ] {
            if items.is_empty() {
                continue;
            }
            writeln!(f, "  {label} ({})", items.len())?;
            for item in items {
                writeln!(
                    f,
                    "    • {} [{}]",
                    item.title,
                    item.reference_ids().join(", ")
                )?;
            }
        }
        Ok(())
    }
}

impl Item {
    /// The pull request numbers (`#1207`) and short commit hashes in [`Item::references`], once
    /// each. Models do not always follow the schema and send Markdown links instead.
    fn reference_ids(&self) -> Vec<&str> {
        let mut ids: Vec<&str> = Vec::new();
        for id in self
            .references
            .iter()
            .flat_map(|reference| references(reference))
        {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        ids
    }
}
// ANCHOR_END: render

/// Places freshly written notes above any notes already in the file.
pub fn prepend_notes(new_notes: &str, existing: &str) -> String {
    let new_notes = new_notes.trim();
    let existing = existing.trim_start();
    if existing.is_empty() {
        format!("{new_notes}\n")
    } else {
        format!("{new_notes}\n\n{existing}")
    }
}

/// Pull requests (`#1207`) and short commit hashes that more than one entry of the Markdown notes
/// refers to, with the number of entries.
///
/// An entry is a top-level list item, together with anything below it up to the next item or
/// heading.
pub fn repeated_references(markdown: &str) -> Vec<(String, usize)> {
    let mut entries: Vec<BTreeSet<&str>> = Vec::new();
    let mut in_entry = false;
    for line in markdown.lines() {
        if line.starts_with("- ") || line.starts_with("* ") {
            entries.push(BTreeSet::new());
            in_entry = true;
        } else if line.starts_with('#') {
            in_entry = false;
        }
        if let (true, Some(entry)) = (in_entry, entries.last_mut()) {
            entry.extend(references(line));
        }
    }

    let mut counts = BTreeMap::new();
    for reference in entries.into_iter().flatten() {
        *counts.entry(reference).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .filter(|&(_, entries)| entries > 1)
        .map(|(reference, entries)| (reference.to_owned(), entries))
        .collect()
}

/// `#1207` and short commit hashes (7 to 12 hex characters, with both digits and letters).
fn references(line: &str) -> impl Iterator<Item = &str> {
    line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '#'))
        .filter(|word| {
            let pull_request = word.strip_prefix('#').is_some_and(|number| {
                !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())
            });
            let short_hash = (7..=12).contains(&word.len())
                && word.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
                && word.bytes().any(|b| b.is_ascii_digit())
                && word.bytes().any(|b| b.is_ascii_alphabetic());
            pull_request || short_hash
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepends_above_existing_notes() {
        assert_eq!(
            prepend_notes("## 0.2.0\n", "## 0.1.0\n- first\n"),
            "## 0.2.0\n\n## 0.1.0\n- first\n"
        );
    }

    #[test]
    fn starts_a_new_file() {
        assert_eq!(prepend_notes("\n## 0.1.0\n\n", ""), "## 0.1.0\n");
    }

    #[test]
    fn finds_a_pull_request_in_two_entries() {
        let notes = "## 0.34.0\n\n### Highlights\n\n\
            - **Contrib crate**: new. ([#1207](https://github.com/o/r/pull/1207))\n\
            - **File tools**: in contrib. ([#1207](https://github.com/o/r/pull/1207), #1210)\n\
            \n### Fixes\n\n\
            - **Bedrock**: fixed. (#1215, 3b1ad22e)\n  Also see 3b1ad22e.\n";
        assert_eq!(repeated_references(notes), vec![("#1207".to_owned(), 2)]);
    }

    #[test]
    fn ignores_references_outside_entries() {
        let notes = "## 0.34.0\n\nIncludes #1207.\n\n### Highlights\n\n- **A**: (#1207)\n";
        assert!(repeated_references(notes).is_empty());
    }

    #[test]
    fn summary_leaves_out_empty_sections() {
        let notes = ReleaseNotes {
            version: "0.34.0".into(),
            summary: "Small.".into(),
            highlights: vec![],
            breaking_changes: vec![],
            fixes: vec![Item {
                title: "Fixed".into(),
                details: "It works.".into(),
                references: vec!["#1".into()],
            }],
        };
        assert_eq!(
            notes.to_string(),
            "0.34.0 — Small.\n  fixes (1)\n    • Fixed [#1]\n"
        );
    }

    #[test]
    fn summary_shows_markdown_link_references_as_ids() {
        let item = Item {
            title: "Contrib".into(),
            details: "New.".into(),
            references: vec![
                "[#1207](https://github.com/bosun-ai/swiftide/pull/1207)".into(),
                "#1207".into(),
                "90bcadc7".into(),
            ],
        };
        assert_eq!(item.reference_ids(), ["#1207", "90bcadc7"]);
    }

    #[test]
    fn stop_tool_requires_every_section() {
        let schema = serde_json::to_value(schema_for!(ReleaseNotes)).unwrap();
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 5);
    }
}
