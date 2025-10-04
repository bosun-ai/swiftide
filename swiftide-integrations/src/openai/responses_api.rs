use std::collections::HashMap;

use anyhow::{Context as _, Result};
use async_openai::types::ResponseFormatJsonSchema;
use async_openai::types::responses::{
    Content, CreateResponse, CreateResponseArgs, FunctionArgs, Input, InputContent, InputItem,
    InputMessageArgs, OutputContent, OutputItem, OutputMessage, OutputStatus, ReasoningConfigArgs,
    Response, ResponseEvent, ResponseMetadata, Role, Status, TextConfig, TextResponseFormat,
    ToolChoice, ToolChoiceMode, ToolDefinition, Usage as ResponsesUsage,
};
use serde_json::json;
use swiftide_core::chat_completion::{
    ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ToolCall, ToolOutput, ToolSpec,
    Usage, UsageBuilder,
};

use super::{GenericOpenAI, openai_error_to_language_model_error};
use crate::openai::LanguageModelError;

type LmResult<T> = Result<T, LanguageModelError>;

pub(super) fn build_responses_request_from_chat<C>(
    client: &GenericOpenAI<C>,
    request: &ChatCompletionRequest,
) -> LmResult<CreateResponse>
where
    C: async_openai::config::Config + Clone + Default,
{
    let model = client
        .options()
        .prompt_model
        .as_ref()
        .ok_or_else(|| LanguageModelError::PermanentError("Model not set".into()))?;

    let mut args = base_request_args(client, model)?;

    let input_items = chat_messages_to_input_items(request.messages())?;
    args.input(Input::Items(input_items));

    if !request.tools_spec().is_empty() {
        let tools = request
            .tools_spec()
            .iter()
            .map(tool_spec_to_responses_tool)
            .collect::<Result<Vec<_>>>()
            .map_err(LanguageModelError::permanent)?;

        args.tools(tools);
        if client.options().parallel_tool_calls.unwrap_or(true) {
            args.tool_choice(ToolChoice::Mode(ToolChoiceMode::Auto));
        }
    }

    args.build().map_err(openai_error_to_language_model_error)
}

fn base_request_args<C>(client: &GenericOpenAI<C>, model: &str) -> LmResult<CreateResponseArgs>
where
    C: async_openai::config::Config + Clone + Default,
{
    let mut args = CreateResponseArgs::default();
    args.model(model);

    let options = client.options();

    if let Some(parallel_tool_calls) = options.parallel_tool_calls {
        args.parallel_tool_calls(parallel_tool_calls);
    }

    if let Some(max_tokens) = options.max_completion_tokens {
        args.max_output_tokens(max_tokens);
    }

    if let Some(temperature) = options.temperature {
        args.temperature(temperature);
    }

    if let Some(reasoning_effort) = options.reasoning_effort.clone() {
        let reasoning = ReasoningConfigArgs::default()
            .effort(reasoning_effort)
            .build()
            .map_err(LanguageModelError::permanent)?;
        args.reasoning(reasoning);
    }

    if let Some(seed) = options.seed {
        tracing::warn!(
            seed,
            "`seed` is not supported by the Responses API; ignoring"
        );
    }

    if let Some(presence_penalty) = options.presence_penalty {
        tracing::warn!(
            presence_penalty,
            "`presence_penalty` is not supported by the Responses API; ignoring"
        );
    }

    if let Some(metadata) = options.metadata.as_ref() {
        if let Some(converted) = convert_metadata(metadata) {
            args.metadata(converted);
        } else {
            tracing::warn!("Responses metadata must be a flat map of string values; skipping");
        }
    }

    if let Some(user) = options.user.as_ref() {
        args.user(user.clone());
    }

    Ok(args)
}

fn convert_metadata(value: &serde_json::Value) -> Option<HashMap<String, String>> {
    match value {
        serde_json::Value::Object(map) => {
            let mut out = HashMap::with_capacity(map.len());
            for (key, val) in map {
                if let Some(s) = val.as_str() {
                    out.insert(key.clone(), s.to_owned());
                } else {
                    return None;
                }
            }
            Some(out)
        }
        _ => None,
    }
}

fn tool_spec_to_responses_tool(spec: &ToolSpec) -> Result<ToolDefinition> {
    let mut properties = serde_json::Map::new();

    for param in &spec.parameters {
        properties.insert(
            param.name.clone(),
            json!({
                "type": param.ty.as_ref(),
                "description": param.description,
            }),
        );
    }

    let required: Vec<String> = spec
        .parameters
        .iter()
        .filter(|param| param.required)
        .map(|param| param.name.clone())
        .collect();

    let parameters = json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    });

    let function = FunctionArgs::default()
        .name(&spec.name)
        .description(&spec.description)
        .parameters(parameters)
        .strict(true)
        .build()?;

    Ok(ToolDefinition::Function(function))
}

fn chat_messages_to_input_items(messages: &[ChatMessage]) -> LmResult<Vec<InputItem>> {
    let mut items = Vec::with_capacity(messages.len());

    for message in messages {
        match message {
            ChatMessage::System(content) => {
                items.push(message_item(Role::System, content.clone())?);
            }
            ChatMessage::User(content) => {
                items.push(message_item(Role::User, content.clone())?);
            }
            ChatMessage::Assistant(content, tool_calls) => {
                if let Some(text) = content {
                    items.push(message_item(Role::Assistant, text.clone())?);
                }

                if let Some(tool_calls) = tool_calls {
                    for tool_call in tool_calls {
                        let id = tool_call.id().to_owned();
                        let arguments = tool_call.args().unwrap_or_default().to_owned();
                        let function_call = async_openai::types::responses::FunctionCall {
                            id: id.clone(),
                            call_id: id.clone(),
                            name: tool_call.name().to_owned(),
                            arguments,
                            status: OutputStatus::InProgress,
                        };

                        let value =
                            serde_json::to_value(OutputContent::FunctionCall(function_call))
                                .map_err(LanguageModelError::permanent)?;
                        items.push(InputItem::Custom(value));
                    }
                }
            }
            ChatMessage::ToolOutput(tool_call, tool_output) => {
                let mut payload = serde_json::Map::new();
                payload.insert(
                    "type".into(),
                    serde_json::Value::String("function_call_output".into()),
                );
                payload.insert(
                    "call_id".into(),
                    serde_json::Value::String(tool_call.id().to_owned()),
                );

                let output_value = match tool_output {
                    ToolOutput::FeedbackRequired(value) => {
                        value.clone().unwrap_or(serde_json::Value::Null)
                    }
                    ToolOutput::Text(text) | ToolOutput::Fail(text) => {
                        serde_json::Value::String(text.clone())
                    }
                    ToolOutput::Stop(message) | ToolOutput::AgentFailed(message) => {
                        serde_json::Value::String(message.clone().unwrap_or_default().into_owned())
                    }
                    _ => serde_json::Value::Null,
                };

                payload.insert("output".into(), output_value);

                if matches!(
                    tool_output,
                    ToolOutput::Fail(_) | ToolOutput::AgentFailed(_)
                ) {
                    payload.insert("is_error".into(), serde_json::Value::Bool(true));
                }

                items.push(InputItem::Custom(serde_json::Value::Object(payload)));
            }
            ChatMessage::Summary(content) => {
                items.push(message_item(Role::Assistant, content.clone())?);
            }
        }
    }

    Ok(items)
}

fn message_item(role: Role, content: String) -> LmResult<InputItem> {
    Ok(InputItem::Message(
        InputMessageArgs::default()
            .role(role)
            .content(InputContent::TextInput(content))
            .build()
            .map_err(LanguageModelError::permanent)?,
    ))
}

#[derive(Default)]
pub(super) struct ResponsesStreamAccumulator {
    response: ChatCompletionResponse,
    tool_index_by_item_id: HashMap<String, usize>,
}

pub(super) struct StreamChunk {
    pub response: ChatCompletionResponse,
    pub finished: bool,
}

impl ResponsesStreamAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply_event(
        &mut self,
        event: ResponseEvent,
        stream_full: bool,
    ) -> LmResult<Option<StreamChunk>> {
        match event {
            ResponseEvent::ResponseOutputTextDelta(delta) => {
                self.response
                    .append_message_delta(Some(delta.delta.as_str()));
                return Ok(Some(self.snapshot(stream_full, false)));
            }
            ResponseEvent::ResponseContentPartAdded(part) => {
                if let Some(text) = part.part.text.as_ref() {
                    self.response.append_message_delta(Some(text.as_str()));
                    return Ok(Some(self.snapshot(stream_full, false)));
                }
            }
            ResponseEvent::ResponseFunctionCallArgumentsDelta(delta) => {
                let index = self.ensure_tool_index(&delta.item_id, delta.output_index as usize);
                self.response
                    .append_tool_call_delta(index, None, None, Some(delta.delta.as_str()));
                return Ok(Some(self.snapshot(stream_full, false)));
            }
            ResponseEvent::ResponseOutputItemAdded(event) => match event.item {
                OutputItem::FunctionCall(function_call) => {
                    let idx = event.output_index as usize;
                    self.tool_index_by_item_id
                        .insert(function_call.id.clone(), idx);
                    if !function_call.call_id.is_empty() {
                        self.tool_index_by_item_id
                            .insert(function_call.call_id.clone(), idx);
                    }

                    let id = if function_call.call_id.is_empty() {
                        function_call.id.clone()
                    } else {
                        function_call.call_id.clone()
                    };

                    let arguments = if function_call.arguments.is_empty() {
                        None
                    } else {
                        Some(function_call.arguments.as_str())
                    };

                    self.response.append_tool_call_delta(
                        idx,
                        Some(id.as_str()),
                        Some(function_call.name.as_str()),
                        arguments,
                    );

                    return Ok(Some(self.snapshot(stream_full, false)));
                }
                OutputItem::Message(message) => {
                    if let Some(text) = collect_message_text_from_message(&message) {
                        self.response.append_message_delta(Some(text.as_str()));
                        return Ok(Some(self.snapshot(stream_full, false)));
                    }
                }
                _ => {}
            },
            ResponseEvent::ResponseOutputItemDone(event) => {
                if let OutputItem::FunctionCall(function_call) = event.item {
                    let idx = event.output_index as usize;
                    self.tool_index_by_item_id
                        .insert(function_call.id.clone(), idx);
                    if !function_call.call_id.is_empty() {
                        self.tool_index_by_item_id
                            .insert(function_call.call_id.clone(), idx);
                    }

                    let id = if function_call.call_id.is_empty() {
                        function_call.id
                    } else {
                        function_call.call_id
                    };
                    self.response.append_tool_call_delta(
                        idx,
                        Some(id.as_str()),
                        Some(function_call.name.as_str()),
                        None,
                    );
                    return Ok(Some(self.snapshot(stream_full, false)));
                }
            }
            ResponseEvent::ResponseFunctionCallArgumentsDone(done) => {
                let index = self.ensure_tool_index(&done.item_id, done.output_index as usize);
                if !done.arguments.is_empty() {
                    let duplicate = self
                        .response
                        .tool_calls
                        .as_ref()
                        .and_then(|calls| calls.get(index))
                        .and_then(|tc| tc.args())
                        .map(|existing| existing == done.arguments)
                        .unwrap_or(false);

                    if !duplicate {
                        self.response.append_tool_call_delta(
                            index,
                            None,
                            None,
                            Some(done.arguments.as_str()),
                        );
                    }
                }
                return Ok(Some(self.snapshot(stream_full, false)));
            }
            ResponseEvent::ResponseCompleted(completed) => {
                metadata_to_chat_completion(&completed.response, &mut self.response)?;
                self.response.delta = None;
                return Ok(Some(self.snapshot(stream_full, true)));
            }
            ResponseEvent::ResponseIncomplete(incomplete) => {
                metadata_to_chat_completion(&incomplete.response, &mut self.response)?;
                self.response.delta = None;
                return Ok(Some(self.snapshot(stream_full, true)));
            }
            ResponseEvent::ResponseFailed(failed) => {
                let message = failed
                    .response
                    .error
                    .as_ref()
                    .map(|err| format!("{}: {}", err.code, err.message))
                    .unwrap_or_else(|| "Responses API stream failed".to_string());
                return Err(LanguageModelError::permanent(message));
            }
            ResponseEvent::ResponseError(error) => {
                return Err(LanguageModelError::permanent(error.message));
            }
            _ => {}
        }

        Ok(None)
    }

    pub fn snapshot(&self, stream_full: bool, finished: bool) -> StreamChunk {
        let mut response = if stream_full || finished {
            self.response.clone()
        } else {
            ChatCompletionResponse {
                id: self.response.id,
                message: None,
                tool_calls: None,
                usage: None,
                delta: self.response.delta.clone(),
            }
        };

        if !stream_full && finished {
            response.usage = self.response.usage.clone();
        }

        StreamChunk { response, finished }
    }

    fn ensure_tool_index(&mut self, item_id: &str, output_index: usize) -> usize {
        *self
            .tool_index_by_item_id
            .entry(item_id.to_owned())
            .or_insert(output_index)
    }
}

pub(super) fn response_to_chat_completion(response: Response) -> LmResult<ChatCompletionResponse> {
    if matches!(response.status, Status::Failed) {
        let error = response
            .error
            .as_ref()
            .map(|err| format!("{}: {}", err.code, err.message))
            .unwrap_or_else(|| "OpenAI Responses API returned failure".to_string());
        return Err(LanguageModelError::permanent(error));
    }

    let mut builder = ChatCompletionResponse::builder();

    if let Some(text) = response.output_text.clone().filter(|s| !s.is_empty()) {
        builder.message(text);
    } else if let Some(text) = collect_message_text(&response.output) {
        builder.message(text);
    }

    let tool_calls = collect_tool_calls(&response.output)?;
    if !tool_calls.is_empty() {
        builder.tool_calls(tool_calls);
    }

    if let Some(usage) = response.usage.as_ref() {
        builder.usage(convert_usage(usage)?);
    }

    builder.build().map_err(LanguageModelError::from)
}

pub(super) fn metadata_to_chat_completion(
    metadata: &ResponseMetadata,
    accumulator: &mut ChatCompletionResponse,
) -> LmResult<()> {
    if let Some(usage) = metadata.usage.as_ref() {
        accumulator.usage = Some(convert_usage(usage)?);
    }

    if accumulator.message.is_none() {
        if let Some(output) = metadata.output.as_ref() {
            if let Some(text) = collect_message_text_from_items(output) {
                accumulator.message = Some(text);
            }
        }
    }

    if accumulator.tool_calls.is_none() {
        if let Some(output) = metadata.output.as_ref() {
            let tool_calls = collect_tool_calls_from_items(output)?;
            if !tool_calls.is_empty() {
                accumulator.tool_calls = Some(tool_calls);
            }
        }
    }

    Ok(())
}

fn convert_usage(usage: &ResponsesUsage) -> LmResult<Usage> {
    UsageBuilder::default()
        .prompt_tokens(usage.input_tokens)
        .completion_tokens(usage.output_tokens)
        .total_tokens(usage.total_tokens)
        .build()
        .map_err(LanguageModelError::permanent)
}

fn collect_message_text(output: &[OutputContent]) -> Option<String> {
    let mut buffer = String::new();

    for item in output {
        if let OutputContent::Message(OutputMessage { content, .. }) = item {
            for part in content {
                if let Content::OutputText(text) = part {
                    if !buffer.is_empty() {
                        buffer.push('\n');
                    }
                    buffer.push_str(&text.text);
                }
            }
        }
    }

    if buffer.is_empty() {
        None
    } else {
        Some(buffer)
    }
}

fn collect_message_text_from_message(message: &OutputMessage) -> Option<String> {
    let mut buffer = String::new();

    for part in &message.content {
        if let Content::OutputText(text) = part {
            if !buffer.is_empty() {
                buffer.push('\n');
            }
            buffer.push_str(&text.text);
        }
    }

    if buffer.is_empty() {
        None
    } else {
        Some(buffer)
    }
}

fn collect_message_text_from_items(output: &[OutputItem]) -> Option<String> {
    let mut buffer = String::new();

    for item in output {
        if let OutputItem::Message(message) = item {
            if let Some(text) = collect_message_text_from_message(message) {
                if !buffer.is_empty() {
                    buffer.push('\n');
                }
                buffer.push_str(&text);
            }
        }
    }

    if buffer.is_empty() {
        None
    } else {
        Some(buffer)
    }
}

fn collect_tool_calls(output: &[OutputContent]) -> LmResult<Vec<ToolCall>> {
    let mut tool_calls = Vec::new();

    for item in output {
        if let OutputContent::FunctionCall(function_call) = item {
            tool_calls.push(tool_call_from_function_call(function_call)?);
        }
    }

    Ok(tool_calls)
}

fn collect_tool_calls_from_items(output: &[OutputItem]) -> LmResult<Vec<ToolCall>> {
    let mut tool_calls = Vec::new();

    for item in output {
        if let OutputItem::FunctionCall(function_call) = item {
            tool_calls.push(tool_call_from_function_call(function_call)?);
        }
    }

    Ok(tool_calls)
}

fn tool_call_from_function_call(
    function_call: &async_openai::types::responses::FunctionCall,
) -> LmResult<ToolCall> {
    let id = if !function_call.call_id.is_empty() {
        function_call.call_id.clone()
    } else {
        function_call.id.clone()
    };

    let mut builder = ToolCall::builder();
    builder.id(id);
    builder.name(function_call.name.clone());
    if !function_call.arguments.is_empty() {
        builder.maybe_args(Some(function_call.arguments.clone()));
    }
    builder
        .build()
        .context("Failed to build tool call")
        .map_err(LanguageModelError::permanent)
}

pub(super) fn build_responses_request_from_prompt<C>(
    client: &GenericOpenAI<C>,
    prompt_text: String,
) -> LmResult<CreateResponse>
where
    C: async_openai::config::Config + Clone + Default,
{
    let model = client
        .options()
        .prompt_model
        .as_ref()
        .ok_or_else(|| LanguageModelError::PermanentError("Model not set".into()))?;

    let mut args = base_request_args(client, model)?;
    args.input(Input::Items(vec![InputItem::Message(
        InputMessageArgs::default()
            .role(Role::User)
            .content(InputContent::TextInput(prompt_text))
            .build()
            .map_err(LanguageModelError::permanent)?,
    )]));

    args.build().map_err(openai_error_to_language_model_error)
}

pub(super) fn build_responses_request_from_prompt_with_schema<C>(
    client: &GenericOpenAI<C>,
    prompt_text: String,
    schema: serde_json::Value,
) -> LmResult<CreateResponse>
where
    C: async_openai::config::Config + Clone + Default,
{
    let model = client
        .options()
        .prompt_model
        .as_ref()
        .ok_or_else(|| LanguageModelError::PermanentError("Model not set".into()))?;

    let mut args = base_request_args(client, model)?;
    args.input(Input::Items(vec![InputItem::Message(
        InputMessageArgs::default()
            .role(Role::User)
            .content(InputContent::TextInput(prompt_text))
            .build()
            .map_err(LanguageModelError::permanent)?,
    )]));

    args.text(TextConfig {
        format: TextResponseFormat::JsonSchema(ResponseFormatJsonSchema {
            description: None,
            name: "swiftide_structured_output".into(),
            schema: Some(schema),
            strict: Some(true),
        }),
    });

    args.build().map_err(openai_error_to_language_model_error)
}
