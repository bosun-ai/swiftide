//! Update plan tool for maintaining a concise, ordered plan with statuses in memory.
//!
//! It helps agents keep track of their tasks and progress.
use std::{
    fmt,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use swiftide_core::{
    AgentContext,
    chat_completion::{Tool, ToolCall, ToolOutput, ToolSpec, errors::ToolError},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// A plan step accepted by [`UpdatePlan`].
///
/// # Examples
///
/// ```no_run
/// use swiftide_agents_contrib::tools::{PlanItem, PlanStatus};
/// let item = PlanItem { step: "Run the tests".into(), status: PlanStatus::Pending };
/// ```
pub struct PlanItem {
    /// Work described by this step.
    pub step: String,
    /// Progress of the step.
    pub status: PlanStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Progress of a plan step.
///
/// # Examples
///
/// ```no_run
/// use swiftide_agents_contrib::tools::PlanStatus;
/// assert_eq!(PlanStatus::InProgress.to_string(), "in_progress");
/// ```
pub enum PlanStatus {
    /// Work has not started.
    Pending,
    /// Work is underway.
    InProgress,
    /// Work is finished.
    Completed,
}

// Keep documentation out of the model-facing schema, as in the source tool.
impl JsonSchema for PlanItem {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("PlanItem")
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        #[derive(JsonSchema)]
        #[serde(deny_unknown_fields)]
        #[allow(dead_code)]
        struct PlanItem {
            step: String,
            status: PlanStatus,
        }
        PlanItem::json_schema(generator)
    }
}

impl JsonSchema for PlanStatus {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("PlanStatus")
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        #[derive(JsonSchema)]
        #[serde(rename_all = "snake_case")]
        #[allow(dead_code)]
        enum PlanStatus {
            Pending,
            InProgress,
            Completed,
        }
        PlanStatus::json_schema(generator)
    }
}

impl fmt::Display for PlanStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            PlanStatus::Pending => "pending",
            PlanStatus::InProgress => "in_progress",
            PlanStatus::Completed => "completed",
        };

        f.write_str(label)
    }
}

#[derive(Debug, Default)]
/// In-memory storage owned by a plan tool; clones of the tool share this storage.
///
/// # Examples
///
/// ```no_run
/// use swiftide_agents_contrib::tools::PlanMemory;
/// let memory = PlanMemory::default();
/// ```
pub struct PlanMemory {
    steps: Mutex<Vec<PlanItem>>,
}

impl PlanMemory {
    pub(crate) fn store(&self, steps: Vec<PlanItem>) {
        *self.steps.lock().unwrap() = steps;
    }
}

/// Maintain a plan with the same validation and messages as Fluyt's `update_plan` tool.
///
/// Both `items` and `explanation` are required. Empty items retrieve the current plan; clones
/// share state and fresh default tools start with an independent plan.
///
/// # Examples
///
/// ```no_run
/// use swiftide_agents::Agent;
/// use swiftide_agents_contrib::tools::UpdatePlan;
/// Agent::builder().add_tool(UpdatePlan::default());
/// ```
#[derive(Clone, Debug, Default)]
pub struct UpdatePlan {
    memory: Arc<PlanMemory>,
}

impl UpdatePlan {
    fn render_summary(&self, explanation: Option<&str>) -> String {
        let plan = self.memory.steps.lock().unwrap();
        let mut lines: Vec<String> = plan
            .iter()
            .enumerate()
            .map(|(idx, item)| format!("{}. [{}] {}", idx + 1, item.status, item.step))
            .collect();

        if let Some(explanation) = explanation {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push(explanation.to_string());
        }

        format!("# Current Plan\n{}", lines.join("\n"))
    }
}

impl UpdatePlan {
    #[allow(clippy::ref_option, reason = "needs to be fixed in swiftide")]
    fn update_plan(
        &self,
        _context: &dyn AgentContext,
        items: Vec<PlanItem>,
        explanation: &str,
    ) -> impl Future<Output = Result<ToolOutput, ToolError>> {
        if items.is_empty() {
            let summary = self.render_summary(None);
            return std::future::ready(Ok(summary.into()));
        }

        let mut in_progress_count = 0;
        let mut plan_items = Vec::with_capacity(items.len());

        for item in items {
            let step = item.step.trim();
            if step.is_empty() || item.step.contains(['\r', '\n']) {
                let summary = self.render_summary(None);
                return std::future::ready(Ok(ToolOutput::fail(format!(
                    "Plan steps cannot be empty or contain line breaks. Please update the plan properly\n\n# Current plan\n{summary}"
                ))));
            }

            if matches!(item.status, PlanStatus::InProgress) {
                in_progress_count += 1;
            }

            plan_items.push(PlanItem {
                step: step.to_string(),
                status: item.status,
            });
        }

        if in_progress_count > 1 {
            let summary = self.render_summary(None);
            return std::future::ready(Ok(ToolOutput::fail(format!(
                "Only one step may be marked in progress. Please update the plan properly\n\n# Current plan\n{summary}"
            ))));
        }

        let explanation = explanation.trim();
        let explanation = if explanation.is_empty() {
            None
        } else {
            Some(explanation)
        };
        self.memory.store(plan_items);
        let summary = self.render_summary(explanation);

        std::future::ready(Ok(ToolOutput::text(summary)))
    }
}

#[derive(Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
struct UpdatePlanArgs {
    #[schemars(
        description = "Array of plan steps with `step` and a status (pending, in_progress, completed). Call with an empty list to retrieve the current plan."
    )]
    items: Vec<PlanItem>,
    #[schemars(
        description = "Required explanation describing how the plan changed; use an empty string when none is needed."
    )]
    explanation: String,
}

#[async_trait]
impl Tool for UpdatePlan {
    async fn invoke(
        &self,
        context: &dyn AgentContext,
        call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        let args = call
            .args()
            .ok_or_else(|| ToolError::missing_arguments("No arguments provided for update_plan"))?;
        let args: UpdatePlanArgs = serde_json::from_str(args)?;
        self.update_plan(context, args.items, &args.explanation)
            .await
    }
    fn name(&self) -> Cow<'_, str> {
        "update_plan".into()
    }
    fn tool_spec(&self) -> ToolSpec {
        ToolSpec::builder()
            .name("update_plan")
            .description("Maintain a concise, ordered plan with statuses in memory.")
            .parameters_schema(schemars::schema_for!(UpdatePlanArgs))
            .build()
            .expect("valid plan schema")
    }
}
