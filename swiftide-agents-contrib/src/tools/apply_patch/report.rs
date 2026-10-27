use super::execution::OperationOutcome;

#[derive(Debug, Default)]
pub(super) struct ApplyPatchSummary {
    pub(super) operations: Vec<OperationOutcome>,
}

impl ApplyPatchSummary {
    pub(super) fn render(self) -> String {
        let total = self.operations.len();
        let reached = self
            .operations
            .iter()
            .filter(|operation| {
                matches!(
                    operation,
                    OperationOutcome::Applied(_) | OperationOutcome::AlreadyPresent(_)
                )
            })
            .count();
        let heading = if reached == total {
            format!(
                "Patch applied successfully ({reached}/{total} operations reached requested state)."
            )
        } else if reached == 0 {
            format!("Patch not applied (0/{total} operations reached requested state).")
        } else {
            format!(
                "Patch partially applied ({reached}/{total} operations reached requested state)."
            )
        };

        let mut sections = Vec::new();
        append_operation_section(
            &mut sections,
            "Applied",
            self.operations
                .iter()
                .filter_map(|operation| match operation {
                    OperationOutcome::Applied(description) => Some(description.as_str()),
                    _ => None,
                }),
        );
        append_operation_section(
            &mut sections,
            "Already present",
            self.operations
                .iter()
                .filter_map(|operation| match operation {
                    OperationOutcome::AlreadyPresent(description) => Some(description.as_str()),
                    _ => None,
                }),
        );

        let not_applied = self
            .operations
            .iter()
            .filter_map(|operation| match operation {
                OperationOutcome::NotApplied { operation, reason } => {
                    Some(format!("{operation}: {reason}"))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if !not_applied.is_empty() {
            sections.push(format!(
                "Not applied:\n{}",
                not_applied
                    .iter()
                    .map(|operation| format!("- {operation}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

        if sections.is_empty() {
            heading
        } else {
            format!("{heading}\n{}", sections.join("\n"))
        }
    }
}

fn append_operation_section<'a>(
    sections: &mut Vec<String>,
    heading: &str,
    operations: impl Iterator<Item = &'a str>,
) {
    let operations = operations.collect::<Vec<_>>();
    if !operations.is_empty() {
        sections.push(format!(
            "{heading}:\n{}",
            operations
                .iter()
                .map(|operation| format!("- {operation}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
}
