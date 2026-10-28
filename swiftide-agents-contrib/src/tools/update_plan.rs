use std::{
    borrow::Cow,
    fmt,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use swiftide_core::{
    AgentContext,
    chat_completion::{Tool, ToolCall, ToolOutput, ToolSpec, errors::ToolError},
};

/// A step in an agent's current plan.
///
/// # Examples
///
/// ```no_run
/// use swiftide_agents_contrib::tools::{PlanItem, PlanStatus};
///
/// let step = PlanItem {
///     step: "Review the proposed changes".into(),
///     status: PlanStatus::InProgress,
/// };
/// assert_eq!(step.status, PlanStatus::InProgress);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanItem {
    /// A short description of the work.
    pub step: String,
    /// The step's progress.
    pub status: PlanStatus,
}

/// Progress of a plan step. A plan may have at most one step in progress.
///
/// # Examples
///
/// ```no_run
/// use swiftide_agents_contrib::tools::{PlanItem, PlanStatus};
///
/// let mut step = PlanItem {
///     step: "Run the tests".into(),
///     status: PlanStatus::Pending,
/// };
/// step.status = PlanStatus::Completed;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    /// Work has not started.
    Pending,
    /// Work is underway.
    InProgress,
    /// Work has finished.
    Completed,
}

impl fmt::Display for PlanStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        })
    }
}

/// In-memory plan state. Share an `Arc<PlanMemory>` to observe or coordinate a single plan.
/// Fresh default instances are independent; this state is not persisted across process restarts.
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
/// use swiftide_agents::Agent;
/// use swiftide_agents_contrib::tools::{PlanMemory, UpdatePlan};
///
/// let memory = Arc::new(PlanMemory::default());
/// Agent::builder().add_tool(UpdatePlan::new(Arc::clone(&memory)));
/// // Show the agent's current progress in the application.
/// for item in memory.snapshot() {
///     println!("{}: {}", item.status, item.step);
/// }
/// ```
#[derive(Debug, Default)]
pub struct PlanMemory {
    steps: Mutex<Vec<PlanItem>>,
}

impl PlanMemory {
    /// Return a consistent, owned snapshot of the current plan.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use swiftide_agents::Agent;
    /// use swiftide_agents_contrib::tools::{PlanMemory, UpdatePlan};
    ///
    /// let memory = Arc::new(PlanMemory::default());
    /// Agent::builder().add_tool(UpdatePlan::new(Arc::clone(&memory)));
    /// // Show the agent's current progress in the application.
    /// for item in memory.snapshot() {
    ///     println!("{}: {}", item.status, item.step);
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if another thread panicked while holding the plan lock.
    pub fn snapshot(&self) -> Vec<PlanItem> {
        self.steps.lock().expect("plan lock poisoned").clone()
    }

    fn replace(&self, items: Vec<PlanItem>) {
        *self.steps.lock().expect("plan lock poisoned") = items;
    }
}

/// Maintain an ordered plan (tool name: `update_plan`).
///
/// Nonempty `items` replace the plan after validation. Empty `items` retrieve it without changing
/// it. Cloning a tool shares its plan; `Default` creates a fresh plan. Use [`Self::new`] to share
/// state with an application. Invalid updates return a failure output and retain the old plan.
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
/// use swiftide_agents::Agent;
/// use swiftide_agents_contrib::tools::{PlanMemory, UpdatePlan};
///
/// let memory = Arc::new(PlanMemory::default());
/// Agent::builder().add_tool(UpdatePlan::new(Arc::clone(&memory)));
/// assert!(memory.snapshot().is_empty());
/// ```
#[derive(Clone, Debug, Default)]
pub struct UpdatePlan {
    memory: Arc<PlanMemory>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct UpdatePlanArgs {
    /// Ordered steps; an empty list retrieves the current plan.
    items: Vec<PlanItem>,
    /// Optional explanation of the update.
    explanation: Option<String>,
}

impl UpdatePlan {
    /// Construct a tool using explicitly shared plan state.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use swiftide_agents::Agent;
    /// use swiftide_agents_contrib::tools::{PlanMemory, UpdatePlan};
    ///
    /// let memory = Arc::new(PlanMemory::default());
    /// Agent::builder().add_tool(UpdatePlan::new(Arc::clone(&memory)));
    /// // Show the agent's current progress in the application.
    /// for item in memory.snapshot() {
    ///     println!("{}: {}", item.status, item.step);
    /// }
    /// ```
    pub fn new(memory: Arc<PlanMemory>) -> Self {
        Self { memory }
    }

    fn update(&self, items: Vec<PlanItem>, explanation: Option<&str>) -> ToolOutput {
        if items.is_empty() {
            return ToolOutput::text(render_plan(&self.memory.snapshot(), None));
        }
        match validate_items(items) {
            Ok(items) => {
                // Render this update rather than rereading a possibly newer concurrent update.
                let summary = render_plan(&items, explanation);
                self.memory.replace(items);
                ToolOutput::text(summary)
            }
            Err(reason) => ToolOutput::fail(format!(
                "{reason}\n\n{}",
                render_plan(&self.memory.snapshot(), None)
            )),
        }
    }
}

fn validate_items(mut items: Vec<PlanItem>) -> Result<Vec<PlanItem>, &'static str> {
    let mut in_progress = false;
    for item in &mut items {
        let trimmed = item.step.trim();
        if trimmed.is_empty() {
            return Err("Plan steps cannot be empty.");
        }
        if item.status == PlanStatus::InProgress {
            if in_progress {
                return Err("Only one step may be marked in progress.");
            }
            in_progress = true;
        }
        if trimmed.len() != item.step.len() {
            item.step = trimmed.to_owned();
        }
    }
    Ok(items)
}

fn render_plan(items: &[PlanItem], explanation: Option<&str>) -> String {
    use fmt::Write as _;
    let mut rendered = String::from("# Current Plan\n");
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            rendered.push('\n');
        }
        write!(rendered, "{}. [{}] {}", index + 1, item.status, item.step).expect("string write");
    }
    if let Some(explanation) = explanation.map(str::trim).filter(|text| !text.is_empty()) {
        if !items.is_empty() {
            rendered.push_str("\n\n");
        }
        rendered.push_str(explanation);
    }
    rendered
}

#[async_trait]
impl Tool for UpdatePlan {
    async fn invoke(
        &self,
        _context: &dyn AgentContext,
        call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        let args = call
            .args()
            .ok_or_else(|| ToolError::missing_arguments("update_plan"))?;
        let args: UpdatePlanArgs = serde_json::from_str(args)?;
        Ok(self.update(args.items, args.explanation.as_deref()))
    }

    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("update_plan")
    }

    fn tool_spec(&self) -> ToolSpec {
        ToolSpec::builder().name("update_plan")
            .description("Maintain a concise ordered plan. Supply an empty items list to retrieve it. At most one step may be in progress.")
            .parameters_schema(schema_for!(UpdatePlanArgs)).build().expect("valid plan schema")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use swiftide_agents::DefaultContext;

    async fn invoke(tool: &UpdatePlan, items: serde_json::Value) -> ToolOutput {
        tool.invoke(
            &DefaultContext::default(),
            &ToolCall::builder()
                .id("plan")
                .name("update_plan")
                .args(json!({"items": items, "explanation": "  progress  "}).to_string())
                .build()
                .unwrap(),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn updates_and_retrieves_a_shared_plan_without_leaking_to_new_instances() {
        let memory = Arc::new(PlanMemory::default());
        let tool = UpdatePlan::new(Arc::clone(&memory));
        let result = invoke(
            &tool,
            json!([{"step":"  Implement  ","status":"in_progress"}]),
        )
        .await;
        assert_eq!(
            result.as_text().unwrap(),
            "# Current Plan\n1. [in_progress] Implement\n\nprogress"
        );
        assert_eq!(memory.snapshot()[0].step, "Implement");
        assert_eq!(
            invoke(&tool.clone(), json!([])).await.as_text().unwrap(),
            "# Current Plan\n1. [in_progress] Implement"
        );
        assert_eq!(
            invoke(&UpdatePlan::default(), json!([]))
                .await
                .as_text()
                .unwrap(),
            "# Current Plan\n"
        );
    }

    #[tokio::test]
    async fn invalid_updates_preserve_the_previous_plan() {
        let memory = Arc::new(PlanMemory::default());
        let tool = UpdatePlan::new(Arc::clone(&memory));
        invoke(&tool, json!([{"step":"Keep","status":"pending"}])).await;
        for invalid in [
            json!([{"step":"  ","status":"pending"}]),
            json!([
            {"step":"One","status":"in_progress"},{"step":"Two","status":"in_progress"}]),
        ] {
            assert!(invoke(&tool, invalid).await.as_fail().is_some());
            assert_eq!(memory.snapshot()[0].step, "Keep");
        }
        tool.tool_spec().strict_parameters_schema().unwrap();
    }
}
