//! Agent hooks that show what the agent is doing, as it happens.
use std::{
    collections::HashMap,
    io::{Write as _, stdout},
    sync::{Arc, Mutex, PoisonError},
};

use swiftide::{
    agents::AgentBuilder,
    chat_completion::{ToolCall, ToolOutput, errors::ToolError},
};

use crate::{draft::DraftPreview, notes::ReleaseNotes, tools::WRITE_CHANGELOG};

// ANCHOR: hooks
/// Prints each tool call and its result on one line.
pub fn report_tool_calls(builder: &mut AgentBuilder) {
    builder
        .before_tool(|_agent, tool_call| {
            println!("  → {}", describe_call(tool_call));
            Box::pin(async { Ok(()) })
        })
        .after_tool(|_agent, tool_call, result| {
            println!("  {} {}", tool_call.name(), describe_result(result));
            Box::pin(async { Ok(()) })
        });
}
// ANCHOR_END: hooks

// ANCHOR: stream
/// Streams the model's text, and the release notes while they are being drafted.
pub fn stream_draft(builder: &mut AgentBuilder) {
    let preview = Arc::new(Mutex::new(DraftPreview::default()));

    // Registering `on_stream` switches the agent to streaming completions.
    builder.on_stream(move |_agent, response| {
        let Some(delta) = &response.delta else {
            return Box::pin(async { Ok(()) });
        };
        let mut out = stdout().lock();
        if let Some(text) = &delta.message_chunk {
            let _ = write!(out, "{text}");
        }
        // The notes arrive as the (JSON) argument of `write_changelog`; show them as they
        // are written.
        let drafts = delta.tool_calls_chunk.iter().flat_map(HashMap::values);
        for call in drafts.filter(|call| call.name.as_deref() == Some(WRITE_CHANGELOG)) {
            if let (Some(id), Some(args)) = (&call.id, &call.arguments) {
                let mut preview = preview.lock().unwrap_or_else(PoisonError::into_inner);
                if let Some(text) = preview.advance(id, args) {
                    let _ = write!(out, "{text}");
                }
            }
        }
        let _ = out.flush();
        Box::pin(async { Ok(()) })
    });
}
// ANCHOR_END: stream

const MAX_ARG_CHARS: usize = 48;

/// `git_diff v0.33.0..HEAD swiftide-agents/src/hooks.rs`
fn describe_call(tool_call: &ToolCall) -> String {
    let raw_args = tool_call.args().unwrap_or_default();
    if let Ok(notes) = serde_json::from_str::<ReleaseNotes>(raw_args) {
        return format!(
            "{} (version {}: highlights {}, breaking {}, fixes {})",
            tool_call.name(),
            notes.version,
            notes.highlights.len(),
            notes.breaking_changes.len(),
            notes.fixes.len()
        );
    }
    let args = serde_json::from_str::<serde_json::Map<_, _>>(raw_args).unwrap_or_default();

    let shown = args.values().filter_map(|value| match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(text) if text.lines().nth(1).is_some() => {
            Some(format!("<{} lines>", text.lines().count()))
        }
        serde_json::Value::String(text) => Some(shorten(text)),
        serde_json::Value::Array(items) => Some(format!("<{} items>", items.len())),
        serde_json::Value::Object(_) => Some("{…}".to_owned()),
        other => Some(other.to_string()),
    });

    std::iter::once(tool_call.name().to_owned())
        .chain(shown)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Long values are cut; paths keep their end, which is the part that tells them apart.
fn shorten(text: &str) -> String {
    let count = text.chars().count();
    if count <= MAX_ARG_CHARS {
        text.to_owned()
    } else if text.contains('/') {
        let tail: String = text.chars().skip(count - MAX_ARG_CHARS).collect();
        format!("…{tail}")
    } else {
        let head: String = text.chars().take(MAX_ARG_CHARS).collect();
        format!("{head}…")
    }
}

/// `✓ 312 lines`, `⏸ waiting for approval`, `✗ <error>`
fn describe_result(result: &Result<ToolOutput, ToolError>) -> String {
    match result {
        Ok(ToolOutput::Text(text)) if text.lines().nth(1).is_none() => format!("✓ {text}"),
        Ok(ToolOutput::Text(text)) => format!("✓ {} lines", text.lines().count()),
        Ok(ToolOutput::FeedbackRequired(_)) => "⏸ waiting for approval".to_owned(),
        Ok(ToolOutput::Stop(_)) => "■ done".to_owned(),
        Ok(ToolOutput::Fail(error)) => format!("✗ {error}"),
        Ok(other) => format!("✓ {other}"),
        Err(error) => format!("✗ {}", error.to_string().lines().next().unwrap_or_default()),
    }
}
