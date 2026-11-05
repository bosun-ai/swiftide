//! What each crate analyst hands back to the task graph.
//!
//! The model fills in a [`CrateReport`]. [`CrateReport::reconciled_with`] then checks it against
//! the facts from `cargo metadata` and turns it into a [`CrateSummary`].

use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use swiftide::agents::tools::control::StopWithArgs;

use crate::workspace::{WorkspaceCrate, WorkspaceDependency};

// ANCHOR: stop_schema
/// What an analyst agent reports about one crate when it calls `stop`.
///
/// The doc comments become field descriptions in the JSON schema the model sees, so they double
/// as instructions.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CrateReport {
    /// Two to four sentences: the problem this crate solves and the role it plays in the
    /// workspace. Describe responsibilities, not files.
    pub purpose: String,
    /// Four to eight of the most important public items a newcomer must know (traits, structs,
    /// enums, macros or functions), each formatted as "`Name`: what it is for". Use names exactly
    /// as they appear in the source.
    pub key_types: Vec<String>,
    /// One entry per workspace crate listed as a dependency in the brief. Empty if the brief
    /// lists none.
    pub depends_on: Vec<DependencyUsage>,
    /// One to three sentences on feature flags, extension points (traits meant to be
    /// implemented) and non-obvious design decisions.
    pub notes: String,
}

/// How the documented crate uses one of its workspace dependencies.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DependencyUsage {
    /// The dependency's crate name, exactly as listed in the brief.
    pub crate_name: String,
    /// What the documented crate uses it for.
    pub usage: String,
}

/// A `stop` tool whose arguments must match [`CrateReport`].
pub fn summary_stop_tool() -> StopWithArgs {
    StopWithArgs::with_parameters_schema(schema_for!(CrateReport))
}
// ANCHOR_END: stop_schema

/// A crate report checked against `cargo metadata`: what each branch hands to the join.
#[derive(Debug, Clone, Serialize)]
pub struct CrateSummary {
    pub name: String,
    /// The model's report. Its `depends_on` has been moved into `dependencies`.
    pub report: CrateReport,
    /// Exactly the workspace dependencies declared in Cargo.toml, with the model's usage notes.
    pub dependencies: Vec<DescribedDependency>,
}

/// A declared workspace dependency and what the crate uses it for.
#[derive(Debug, Clone, Serialize)]
pub struct DescribedDependency {
    #[serde(flatten)]
    pub dependency: WorkspaceDependency,
    pub usage: String,
}

const USAGE_NOT_DESCRIBED: &str = "usage not described";

impl CrateReport {
    /// Aligns the model's answer with the facts from `cargo metadata`.
    ///
    /// The dependencies come from the manifest. The model's description of how each is used is
    /// kept, and dependencies the model invented are dropped.
    pub fn reconciled_with(mut self, member: &WorkspaceCrate) -> CrateSummary {
        let mut described = std::mem::take(&mut self.depends_on);
        let dependencies = member
            .workspace_deps
            .iter()
            .map(|dependency| {
                let usage = described
                    .iter()
                    .position(|entry| entry.refers_to(&dependency.name))
                    .map_or_else(
                        || USAGE_NOT_DESCRIBED.to_string(),
                        |index| described.swap_remove(index).usage,
                    );
                DescribedDependency {
                    dependency: dependency.clone(),
                    usage,
                }
            })
            .collect();

        CrateSummary {
            name: member.name.clone(),
            report: self,
            dependencies,
        }
    }
}

impl DependencyUsage {
    /// Whether this entry names `crate_name`, tolerating the decorations of the brief
    /// (backticks and the `(optional)` marker).
    fn refers_to(&self, crate_name: &str) -> bool {
        let named = self.crate_name.trim().trim_matches('`');
        let named = named.strip_suffix("(optional)").unwrap_or(named);
        named.trim().trim_matches('`') == crate_name
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn usage(crate_name: &str, usage: &str) -> DependencyUsage {
        DependencyUsage {
            crate_name: crate_name.into(),
            usage: usage.into(),
        }
    }

    fn report(depends_on: Vec<DependencyUsage>) -> CrateReport {
        CrateReport {
            purpose: String::new(),
            key_types: Vec::new(),
            depends_on,
            notes: String::new(),
        }
    }

    fn app() -> WorkspaceCrate {
        WorkspaceCrate {
            name: "app".into(),
            dir: PathBuf::from("app"),
            description: None,
            workspace_deps: vec![
                WorkspaceDependency {
                    name: "core".into(),
                    optional: false,
                },
                WorkspaceDependency {
                    name: "extras".into(),
                    optional: true,
                },
            ],
            features: Vec::new(),
        }
    }

    fn usages(summary: &CrateSummary) -> Vec<(&str, bool, &str)> {
        summary
            .dependencies
            .iter()
            .map(|described| {
                (
                    described.dependency.name.as_str(),
                    described.dependency.optional,
                    described.usage.as_str(),
                )
            })
            .collect()
    }

    #[test]
    fn keeps_described_usage_completes_declared_dependencies_and_drops_invented_ones() {
        let summary = report(vec![
            usage("`core`", "shared traits"),
            usage("invented", "nothing"),
        ])
        .reconciled_with(&app());

        assert_eq!(summary.name, "app");
        assert!(summary.report.depends_on.is_empty());
        assert_eq!(
            usages(&summary),
            [
                ("core", false, "shared traits"),
                ("extras", true, USAGE_NOT_DESCRIBED)
            ]
        );
    }

    #[test]
    fn matches_a_dependency_echoed_with_the_optional_marker_from_the_brief() {
        let summary = report(vec![
            usage("extras (optional)", "extra tools"),
            usage(" `core` ", "shared traits"),
        ])
        .reconciled_with(&app());

        assert_eq!(
            usages(&summary),
            [
                ("core", false, "shared traits"),
                ("extras", true, "extra tools")
            ]
        );
    }

    #[test]
    fn nested_stop_arguments_are_rejected_by_name() {
        let nested = serde_json::json!({
            "arguments": { "purpose": "", "key_types": [], "depends_on": [], "notes": "" }
        });

        let error = serde_json::from_value::<CrateReport>(nested).unwrap_err();

        assert!(
            error.to_string().contains("unknown field `arguments`"),
            "{error}"
        );
    }

    #[test]
    fn the_schema_the_model_sees_forbids_unknown_fields() {
        let schema = serde_json::to_value(schema_for!(CrateReport)).unwrap();

        assert_eq!(schema["additionalProperties"], serde_json::json!(false));
    }
}
