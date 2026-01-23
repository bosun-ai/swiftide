use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::{Context as _, Result};
use async_openai::types::responses::{
    CreateResponse, CreateResponseArgs, EasyInputContent, EasyInputMessageArgs, FunctionCallOutput,
    FunctionCallOutputItemParam, FunctionTool, FunctionToolCall,
    ImageDetail as ResponsesImageDetail, IncludeEnum, InputContent, InputImageContent, InputItem,
    InputParam, InputTextContent, MessageType, OutputContent, OutputItem, OutputMessage,
    OutputMessageContent, OutputStatus, ReasoningArgs, ReasoningSummary, Response,
    ResponseFormatJsonSchema, ResponseStream, ResponseStreamEvent, ResponseTextParam,
    ResponseUsage as ResponsesUsage, Role, Status, TextResponseFormatConfiguration, Tool,
    ToolChoiceOptions, ToolChoiceParam,
};
use futures_util::Stream;
use serde_json::json;
use swiftide_core::chat_completion::{
    ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ChatMessageContent,
    ChatMessageContentPart, ImageDetail as CoreImageDetail, ReasoningItem, ToolCall, ToolOutput,
    ToolSpec, Usage, UsageBuilder,
};

use super::{
    GenericOpenAI, ensure_tool_schema_additional_properties_false,
    ensure_tool_schema_required_matches_properties, openai_error_to_language_model_error,
};
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

    let options = client.options();
    let include_reasoning = options.reasoning_effort.is_some();
    let input_items = chat_messages_to_input_items(request.messages(), include_reasoning)?;
    args.input(InputParam::Items(input_items));

    if !request.tools_spec().is_empty() {
        let tools = request
            .tools_spec()
            .iter()
            .map(tool_spec_to_responses_tool)
            .collect::<Result<Vec<_>>>()
            .map_err(LanguageModelError::permanent)?;

        args.tools(tools);
        if client.options().parallel_tool_calls.unwrap_or(true) {
            args.tool_choice(ToolChoiceParam::Mode(ToolChoiceOptions::Auto));
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
        let mut reasoning = ReasoningArgs::default();
        reasoning.effort(reasoning_effort);

        if options.reasoning_features.unwrap_or(true) {
            reasoning.summary(ReasoningSummary::Auto);
            args.include(vec![IncludeEnum::ReasoningEncryptedContent]);
        }

        let reasoning = reasoning.build().map_err(LanguageModelError::permanent)?;
        args.reasoning(reasoning);

        // Reasoning models should always be stateless in Responses API usage.
        args.store(false);
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

fn tool_spec_to_responses_tool(spec: &ToolSpec) -> Result<Tool> {
    let mut parameters = match &spec.parameters_schema {
        Some(schema) => {
            serde_json::to_value(schema).context("failed to serialize tool parameters schema")?
        }
        None => json!({
            "type": "object",
            "properties": {},
            "required": [],
            "additionalProperties": false,
        }),
    };

    ensure_tool_schema_additional_properties_false(&mut parameters)
        .context("tool schema must allow no additional properties")?;
    ensure_tool_schema_required_matches_properties(&mut parameters)
        .context("tool schema must list required properties")?;

    let function = FunctionTool {
        name: spec.name.clone(),
        parameters: Some(parameters),
        strict: Some(true),
        description: Some(spec.description.clone()),
    };

    Ok(Tool::Function(function))
}

fn chat_messages_to_input_items(
    messages: &[ChatMessage],
    include_reasoning: bool,
) -> LmResult<Vec<InputItem>> {
    let mut items = Vec::with_capacity(messages.len());

    for message in messages {
        match message {
            ChatMessage::System(content) => {
                items.push(message_item(Role::System, content.clone())?);
            }
            ChatMessage::User(content) => {
                let content = user_content_to_easy_input_content(content);
                items.push(message_item_with_content(Role::User, content)?);
            }
            ChatMessage::Assistant(content, tool_calls) => {
                if let Some(text) = content.as_ref() {
                    items.push(message_item(Role::Assistant, text.clone())?);
                }

                if let Some(tool_calls) = tool_calls.as_ref() {
                    for tool_call in tool_calls {
                        let call_id = normalize_responses_function_call_id(tool_call.id());
                        let arguments = tool_call.args().unwrap_or_default().to_owned();

                        let function_call = FunctionToolCall {
                            arguments,
                            call_id: call_id.clone(),
                            name: tool_call.name().to_owned(),
                            id: None,
                            status: Some(OutputStatus::InProgress),
                        };

                        items.push(InputItem::Item(
                            async_openai::types::responses::Item::FunctionCall(function_call),
                        ));
                    }
                }
            }
            ChatMessage::ToolOutput(tool_call, tool_output) => {
                let output = match tool_output {
                    ToolOutput::FeedbackRequired(value)
                    | ToolOutput::Stop(value)
                    | ToolOutput::AgentFailed(value) => FunctionCallOutput::Text(
                        value
                            .as_ref()
                            .map_or_else(String::new, serde_json::Value::to_string),
                    ),
                    ToolOutput::Text(text) | ToolOutput::Fail(text) => {
                        FunctionCallOutput::Text(text.clone())
                    }
                    _ => FunctionCallOutput::Text(String::new()),
                };

                let function_output = FunctionCallOutputItemParam {
                    call_id: normalize_responses_function_call_id(tool_call.id()),
                    output,
                    id: None,
                    status: Some(OutputStatus::Completed),
                };

                items.push(InputItem::Item(
                    async_openai::types::responses::Item::FunctionCallOutput(function_output),
                ));
            }
            ChatMessage::Reasoning(item) => {
                if !include_reasoning
                    || item.encrypted_content.is_none()
                    || item
                        .encrypted_content
                        .as_ref()
                        .is_some_and(String::is_empty)
                {
                    continue;
                }

                let reasoning_item = async_openai::types::responses::ReasoningItem {
                    id: item.id.clone(),
                    summary: Vec::new(),
                    content: None,
                    encrypted_content: item.encrypted_content.clone(),
                    status: None,
                };

                items.push(InputItem::Item(
                    async_openai::types::responses::Item::Reasoning(reasoning_item),
                ));
            }
            ChatMessage::Summary(content) => {
                items.push(message_item(Role::Assistant, content.clone())?);
            }
        }
    }

    Ok(items)
}

fn message_item(role: Role, content: String) -> LmResult<InputItem> {
    message_item_with_content(role, EasyInputContent::Text(content))
}

fn message_item_with_content(role: Role, content: EasyInputContent) -> LmResult<InputItem> {
    Ok(InputItem::EasyMessage(
        EasyInputMessageArgs::default()
            .r#type(MessageType::Message)
            .role(role)
            .content(content)
            .build()
            .map_err(LanguageModelError::permanent)?,
    ))
}

fn user_content_to_easy_input_content(content: &ChatMessageContent) -> EasyInputContent {
    match content {
        ChatMessageContent::Text(text) => EasyInputContent::Text(text.clone()),
        ChatMessageContent::Parts(parts) => {
            let mapped = parts.iter().map(part_to_input_content).collect();
            EasyInputContent::ContentList(mapped)
        }
    }
}

fn part_to_input_content(part: &ChatMessageContentPart) -> InputContent {
    match part {
        ChatMessageContentPart::Text { text } => {
            InputContent::from(InputTextContent::from(text.as_str()))
        }
        ChatMessageContentPart::ImageUrl { url, detail } => {
            let image = InputImageContent {
                detail: detail.map(map_image_detail).unwrap_or_default(),
                file_id: None,
                image_url: Some(url.clone()),
            };
            InputContent::from(image)
        }
    }
}

fn map_image_detail(detail: CoreImageDetail) -> ResponsesImageDetail {
    match detail {
        CoreImageDetail::Auto => ResponsesImageDetail::Auto,
        CoreImageDetail::Low => ResponsesImageDetail::Low,
        CoreImageDetail::High => ResponsesImageDetail::High,
    }
}

fn normalize_responses_function_call_id(id: &str) -> String {
    if id.starts_with("fc_") {
        id.to_owned()
    } else if let Some(stripped) = id.strip_prefix("call_") {
        format!("fc_{stripped}")
    } else {
        id.to_owned()
    }
}

#[derive(Default)]
pub(super) struct ResponsesStreamState {
    response: ChatCompletionResponse,
    finished: bool,
}

#[derive(Debug, Clone)]
pub(super) struct ResponsesStreamItem {
    pub response: ChatCompletionResponse,
    pub finished: bool,
}

impl ResponsesStreamState {
    #[allow(clippy::too_many_lines)]
    fn apply_event(
        &mut self,
        event: ResponseStreamEvent,
        stream_full: bool,
    ) -> LmResult<Option<ResponsesStreamItem>> {
        if self.finished {
            return Ok(None);
        }

        let maybe_item = match event {
            ResponseStreamEvent::ResponseOutputTextDelta(delta) => {
                self.response
                    .append_message_delta(Some(delta.delta.as_str()));
                Some(self.emit(stream_full, false))
            }
            ResponseStreamEvent::ResponseContentPartAdded(part) => match &part.part {
                OutputContent::OutputText(text) => {
                    self.response.append_message_delta(Some(text.text.as_str()));
                    Some(self.emit(stream_full, false))
                }
                _ => None,
            },
            ResponseStreamEvent::ResponseOutputItemAdded(event) => match event.item {
                OutputItem::FunctionCall(function_call) => {
                    let index = event.output_index as usize;
                    let id = function_call_identifier(&function_call);
                    let arguments = (!function_call.arguments.is_empty())
                        .then_some(function_call.arguments.as_str());
                    self.response.append_tool_call_delta(
                        index,
                        Some(id),
                        Some(function_call.name.as_str()),
                        arguments,
                    );
                    Some(self.emit(stream_full, false))
                }
                OutputItem::Message(message) => {
                    collect_message_text_from_message(&message).map(|text| {
                        self.response.append_message_delta(Some(text.as_str()));
                        self.emit(stream_full, false)
                    })
                }
                _ => None,
            },
            ResponseStreamEvent::ResponseOutputItemDone(event) => {
                if let OutputItem::FunctionCall(function_call) = event.item {
                    let index = event.output_index as usize;
                    let id = function_call_identifier(&function_call);
                    self.response.append_tool_call_delta(
                        index,
                        Some(id),
                        Some(function_call.name.as_str()),
                        None,
                    );
                    Some(self.emit(stream_full, false))
                } else {
                    None
                }
            }
            ResponseStreamEvent::ResponseFunctionCallArgumentsDelta(delta) => {
                let index = delta.output_index as usize;
                self.response
                    .append_tool_call_delta(index, None, None, Some(delta.delta.as_str()));
                Some(self.emit(stream_full, false))
            }
            ResponseStreamEvent::ResponseFunctionCallArgumentsDone(done) => {
                let index = done.output_index as usize;

                let name = done.name.as_deref().filter(|n| !n.is_empty());

                let mut arguments = None;
                if !done.arguments.is_empty() {
                    let new_args = done.arguments.as_str();
                    let duplicate = self
                        .response
                        .tool_calls
                        .as_ref()
                        .and_then(|calls| calls.get(index))
                        .and_then(|tc| tc.args())
                        .is_some_and(|existing| existing == new_args);
                    if !duplicate {
                        arguments = Some(new_args);
                    }
                }

                if name.is_some() || arguments.is_some() {
                    self.response
                        .append_tool_call_delta(index, None, name, arguments);
                    Some(self.emit(stream_full, false))
                } else {
                    None
                }
            }
            ResponseStreamEvent::ResponseCompleted(completed) => {
                metadata_to_chat_completion(&completed.response, &mut self.response)?;
                self.response.delta = None;
                self.finished = true;
                Some(self.emit(stream_full, true))
            }
            ResponseStreamEvent::ResponseIncomplete(incomplete) => {
                metadata_to_chat_completion(&incomplete.response, &mut self.response)?;
                self.response.delta = None;
                self.finished = true;
                Some(self.emit(stream_full, true))
            }
            ResponseStreamEvent::ResponseFailed(failed) => {
                self.finished = true;
                let message = failed.response.error.as_ref().map_or_else(
                    || "Responses API stream failed".to_string(),
                    |err| format!("{}: {}", err.code, err.message),
                );
                return Err(LanguageModelError::permanent(message));
            }
            ResponseStreamEvent::ResponseError(error) => {
                self.finished = true;
                return Err(LanguageModelError::permanent(error.message));
            }
            _ => None,
        };

        Ok(maybe_item)
    }

    fn emit(&mut self, stream_full: bool, finished: bool) -> ResponsesStreamItem {
        let response = if finished {
            // Stream is complete; move the accumulated response out of state.
            let mut response = std::mem::take(&mut self.response);
            response.delta = None;
            response
        } else if stream_full {
            self.response.clone()
        } else {
            ChatCompletionResponse {
                id: self.response.id,
                message: None,
                tool_calls: None,
                usage: None,
                reasoning: None,
                delta: self.response.delta.clone(),
            }
        };

        ResponsesStreamItem { response, finished }
    }

    fn take_final(&mut self, stream_full: bool) -> Option<ResponsesStreamItem> {
        if self.finished {
            None
        } else {
            self.finished = true;
            Some(self.emit(stream_full, true))
        }
    }
}

pub(super) fn responses_stream_adapter(
    stream: ResponseStream,
    stream_full: bool,
) -> ResponsesStreamAdapter {
    ResponsesStreamAdapter::new(stream, stream_full)
}

pub(super) struct ResponsesStreamAdapter {
    inner: ResponseStream,
    state: ResponsesStreamState,
    stream_full: bool,
    finished: bool,
}

impl ResponsesStreamAdapter {
    fn new(stream: ResponseStream, stream_full: bool) -> Self {
        Self {
            inner: stream,
            state: ResponsesStreamState::default(),
            stream_full,
            finished: false,
        }
    }
}

impl Stream for ResponsesStreamAdapter {
    type Item = LmResult<ResponsesStreamItem>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if this.finished {
            return Poll::Ready(None);
        }

        loop {
            match this.inner.as_mut().poll_next(cx) {
                Poll::Ready(Some(result)) => {
                    let event = match result {
                        Ok(event) => event,
                        Err(err) => {
                            this.finished = true;
                            return Poll::Ready(Some(Err(openai_error_to_language_model_error(
                                err,
                            ))));
                        }
                    };

                    match this.state.apply_event(event, this.stream_full) {
                        Ok(Some(item)) => {
                            if item.finished {
                                this.finished = true;
                            }
                            return Poll::Ready(Some(Ok(item)));
                        }
                        Ok(None) => {}
                        Err(err) => {
                            this.finished = true;
                            return Poll::Ready(Some(Err(err)));
                        }
                    }
                }
                Poll::Ready(None) => {
                    this.finished = true;
                    if let Some(item) = this.state.take_final(this.stream_full) {
                        return Poll::Ready(Some(Ok(item)));
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

pub(super) fn response_to_chat_completion(response: &Response) -> LmResult<ChatCompletionResponse> {
    if matches!(response.status, Status::Failed) {
        let error = response.error.as_ref().map_or_else(
            || "OpenAI Responses API returned failure".to_string(),
            |err| format!("{}: {}", err.code, err.message),
        );
        return Err(LanguageModelError::permanent(error));
    }

    let mut builder = ChatCompletionResponse::builder();

    let reasoning_items = collect_reasoning_items_from_items(&response.output);
    if !reasoning_items.is_empty() {
        builder.reasoning(reasoning_items);
    }

    if let Some(text) = response.output_text().filter(|s| !s.is_empty()) {
        builder.message(text);
    } else if let Some(text) = collect_message_text_from_items(&response.output) {
        builder.message(text);
    }

    let tool_calls = collect_tool_calls_from_items(&response.output)?;
    if !tool_calls.is_empty() {
        builder.tool_calls(tool_calls);
    }

    if let Some(usage) = response.usage.as_ref() {
        builder.usage(convert_usage(usage)?);
    }

    builder.build().map_err(LanguageModelError::from)
}

pub(super) fn metadata_to_chat_completion(
    metadata: &Response,
    accumulator: &mut ChatCompletionResponse,
) -> LmResult<()> {
    if let Some(usage) = metadata.usage.as_ref() {
        accumulator.usage = Some(convert_usage(usage)?);
    }

    if accumulator.message.is_none()
        && let Some(text) = collect_message_text_from_items(&metadata.output)
    {
        accumulator.message = Some(text);
    }

    if accumulator.tool_calls.is_none() {
        let tool_calls = collect_tool_calls_from_items(&metadata.output)?;
        if !tool_calls.is_empty() {
            accumulator.tool_calls = Some(tool_calls);
        }
    }

    if accumulator.reasoning.is_none() {
        let reasoning_items = collect_reasoning_items_from_items(&metadata.output);
        if !reasoning_items.is_empty() {
            accumulator.reasoning = Some(reasoning_items);
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

fn collect_message_text_from_items(output: &[OutputItem]) -> Option<String> {
    let mut buffer = String::new();

    for item in output {
        if let OutputItem::Message(OutputMessage { content, .. }) = item {
            for part in content {
                if let OutputMessageContent::OutputText(text) = part {
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
        if let OutputMessageContent::OutputText(text) = part {
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

fn collect_tool_calls_from_items(output: &[OutputItem]) -> LmResult<Vec<ToolCall>> {
    let calls = output.iter().filter_map(|item| match item {
        OutputItem::FunctionCall(function_call) => Some(function_call),
        _ => None,
    });

    tool_calls_from_iter(calls)
}

fn collect_reasoning_items_from_items(output: &[OutputItem]) -> Vec<ReasoningItem> {
    output
        .iter()
        .filter_map(|item| match item {
            OutputItem::Reasoning(reasoning) => Some(ReasoningItem {
                id: reasoning.id.clone(),
                summary: reasoning
                    .summary
                    .iter()
                    .map(|part| match part {
                        async_openai::types::responses::SummaryPart::SummaryText(summary) => {
                            summary.text.clone()
                        }
                    })
                    .collect(),
                content: reasoning
                    .content
                    .as_ref()
                    .map(|c| c.iter().map(|c| c.text.clone()).collect()),
                status: {
                    if let Some(status) = &reasoning.status {
                        match status {
                            OutputStatus::Completed => {
                                Some(swiftide_core::chat_completion::ReasoningStatus::Completed)
                            }
                            OutputStatus::InProgress => {
                                Some(swiftide_core::chat_completion::ReasoningStatus::InProgress)
                            }
                            OutputStatus::Incomplete => {
                                Some(swiftide_core::chat_completion::ReasoningStatus::Incomplete)
                            }
                        }
                    } else {
                        None
                    }
                },
                encrypted_content: reasoning.encrypted_content.clone(),
            }),
            _ => None,
        })
        .collect()
}

fn tool_call_from_function_call(function_call: &FunctionToolCall) -> LmResult<ToolCall> {
    let id = if function_call.call_id.is_empty() {
        function_call.id.as_deref().unwrap_or_default().to_string()
    } else {
        function_call.call_id.clone()
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

fn tool_calls_from_iter<'a, I>(calls: I) -> LmResult<Vec<ToolCall>>
where
    I: IntoIterator<Item = &'a FunctionToolCall>,
{
    calls
        .into_iter()
        .map(tool_call_from_function_call)
        .collect::<Result<Vec<_>, _>>()
}

fn function_call_identifier(function_call: &FunctionToolCall) -> &str {
    if function_call.call_id.is_empty() {
        function_call
            .id
            .as_deref()
            .unwrap_or(function_call.call_id.as_str())
    } else {
        function_call.call_id.as_str()
    }
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
    args.input(InputParam::Items(vec![InputItem::EasyMessage(
        EasyInputMessageArgs::default()
            .r#type(MessageType::Message)
            .role(Role::User)
            .content(EasyInputContent::Text(prompt_text))
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
    args.input(InputParam::Items(vec![InputItem::EasyMessage(
        EasyInputMessageArgs::default()
            .r#type(MessageType::Message)
            .role(Role::User)
            .content(EasyInputContent::Text(prompt_text))
            .build()
            .map_err(LanguageModelError::permanent)?,
    )]));

    args.text(ResponseTextParam {
        format: TextResponseFormatConfiguration::JsonSchema(ResponseFormatJsonSchema {
            description: None,
            name: "swiftide_structured_output".into(),
            schema: Some(schema),
            strict: Some(true),
        }),
        verbosity: None,
    });

    args.build().map_err(openai_error_to_language_model_error)
}

#[allow(clippy::items_after_statements)]
#[cfg(test)]
mod tests {
    use super::*;
    use async_openai::types::responses::{
        AssistantRole, FunctionToolCall, IncludeEnum, InputTokenDetails, OutputItem, OutputMessage,
        OutputMessageContent, OutputStatus, OutputTextContent, OutputTokenDetails, ReasoningEffort,
        ReasoningSummary, ResponseCompletedEvent, ResponseErrorEvent, ResponseFailedEvent,
        ResponseFunctionCallArgumentsDeltaEvent, ResponseFunctionCallArgumentsDoneEvent,
        ResponseOutputItemAddedEvent, ResponseOutputItemDoneEvent, ResponseStreamEvent,
        ResponseTextDeltaEvent, ResponseUsage as ResponsesUsage, Tool,
    };
    use serde_json::{json, to_value};
    use std::collections::HashSet;
    use swiftide_core::chat_completion::{
        ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ChatMessageContent,
        ChatMessageContentPart, ImageDetail as CoreImageDetail, ReasoningItem, ToolCall, ToolSpec,
        Usage,
    };

    use crate::openai::{OpenAI, Options};

    fn expect_emit(
        state: &mut ResponsesStreamState,
        event: ResponseStreamEvent,
        stream_full: bool,
    ) -> ResponsesStreamItem {
        state
            .apply_event(event, stream_full)
            .unwrap()
            .expect("expected emission")
    }

    fn expect_no_emit(
        state: &mut ResponsesStreamState,
        event: ResponseStreamEvent,
        stream_full: bool,
    ) {
        assert!(
            state.apply_event(event, stream_full).unwrap().is_none(),
            "expected no emission"
        );
    }

    fn sample_usage() -> ResponsesUsage {
        ResponsesUsage {
            input_tokens: 5,
            input_tokens_details: InputTokenDetails { cached_tokens: 0 },
            output_tokens: 3,
            output_tokens_details: OutputTokenDetails {
                reasoning_tokens: 0,
            },
            total_tokens: 8,
        }
    }

    #[allow(dead_code)]
    #[derive(schemars::JsonSchema)]
    struct WeatherArgs {
        _city: String,
    }

    fn sample_tool_spec() -> ToolSpec {
        ToolSpec::builder()
            .name("get_weather")
            .description("Retrieve weather data")
            .parameters_schema(schemars::schema_for!(WeatherArgs))
            .build()
            .unwrap()
    }

    #[test]
    fn test_user_content_to_easy_input_content_with_image() {
        let content = ChatMessageContent::parts(vec![
            ChatMessageContentPart::text("Describe this image."),
            ChatMessageContentPart::image_url(
                "https://example.com/image.png",
                Some(CoreImageDetail::Low),
            ),
        ]);

        let easy = user_content_to_easy_input_content(&content);
        let value = to_value(easy).expect("serialize easy content");
        let parts = value.as_array().expect("expected content list array");

        assert_eq!(parts[0]["type"], "input_text");
        assert_eq!(parts[0]["text"], "Describe this image.");
        assert_eq!(parts[1]["type"], "input_image");
        assert_eq!(parts[1]["image_url"], "https://example.com/image.png");
        assert_eq!(parts[1]["detail"], "low");
    }

    fn output_message(id: &str, parts: &[&str]) -> OutputMessage {
        OutputMessage {
            content: parts
                .iter()
                .map(|text| {
                    OutputMessageContent::OutputText(OutputTextContent {
                        annotations: Vec::new(),
                        logprobs: None,
                        text: (*text).to_string(),
                    })
                })
                .collect(),
            id: id.to_string(),
            role: AssistantRole::Assistant,
            status: OutputStatus::Completed,
        }
    }

    fn response_with_message_tool_reasoning(message: &str) -> Response {
        let output_message = OutputItem::Message(output_message("msg", &[message]));
        let output = vec![
            serde_json::to_value(output_message).expect("output message serializes"),
            json!({
                "type": "function_call",
                "id": "call",
                "call_id": "call",
                "name": "metadata_tool",
                "arguments": "{\"ok\":true}",
                "status": "completed"
            }),
            json!({
                "type": "reasoning",
                "id": "reasoning_meta",
                "summary": [
                    {"type": "summary_text", "text": "metadata summary"}
                ]
            }),
        ];

        serde_json::from_value(json!({
            "created_at": 0,
            "id": "resp",
            "model": "gpt-4.1",
            "object": "response",
            "status": "completed",
            "output": output,
            "usage": sample_usage(),
        }))
        .expect("valid response json")
    }

    #[test]
    fn test_build_responses_request_includes_tools_and_options() {
        let openai = OpenAI::builder()
            .default_prompt_model("gpt-4.1")
            .parallel_tool_calls(Some(true))
            .default_options(
                Options::builder()
                    .metadata(json!({"tag": "demo"}))
                    .user("tester")
                    .temperature(0.2),
            )
            .build()
            .unwrap();

        let tool_spec = sample_tool_spec();
        let mut tools = HashSet::new();
        tools.insert(tool_spec);

        let request = ChatCompletionRequest::builder()
            .messages(vec![ChatMessage::User("hi".into())])
            .tool_specs(tools)
            .build()
            .unwrap();

        let create = build_responses_request_from_chat(&openai, &request).unwrap();

        assert_eq!(create.model.as_deref(), Some("gpt-4.1"));
        assert_eq!(create.temperature, Some(0.2));
        assert_eq!(create.parallel_tool_calls, Some(true));
        assert_eq!(
            create
                .metadata
                .as_ref()
                .and_then(|m| m.get("tag"))
                .map(String::as_str),
            Some("demo"),
        );

        let InputParam::Items(items) = &create.input else {
            panic!("expected items input");
        };
        assert_eq!(items.len(), 1);

        let tools = create.tools.expect("tools present");
        assert_eq!(tools.len(), 1);
        assert_eq!(
            create.tool_choice,
            Some(ToolChoiceParam::Mode(ToolChoiceOptions::Auto))
        );
    }

    #[test]
    fn test_build_responses_request_sets_additional_properties_false_for_custom_tool_schema() {
        let openai = OpenAI::builder()
            .default_prompt_model("gpt-4.1")
            .build()
            .unwrap();

        let mut tools = HashSet::new();
        tools.insert(sample_tool_spec());

        let request = ChatCompletionRequest::builder()
            .messages(vec![ChatMessage::User("hi".into())])
            .tool_specs(tools)
            .build()
            .unwrap();

        let create = build_responses_request_from_chat(&openai, &request).unwrap();

        let tools = create.tools.expect("tools present");
        assert_eq!(tools.len(), 1);

        let Tool::Function(function) = &tools[0] else {
            panic!("expected function tool");
        };

        let additional_properties = function
            .parameters
            .as_ref()
            .and_then(|params| params.get("additionalProperties").cloned());

        #[allow(dead_code)]
        #[derive(schemars::JsonSchema)]
        #[serde(deny_unknown_fields)]
        #[schemars(title = "WeatherArgs")]
        struct WeatherArgsCorrect {
            _city: String,
        }

        assert_eq!(
            additional_properties,
            Some(serde_json::Value::Bool(false)),
            "OpenAI requires additionalProperties to be set to false for tool parameters, got {}",
            serde_json::to_string_pretty(&function.parameters).unwrap()
        );

        assert_eq!(
            function.parameters,
            Some(serde_json::to_value(schemars::schema_for!(WeatherArgsCorrect)).unwrap())
        );
    }

    #[test]
    fn test_build_responses_request_reasoning_is_stateless_with_summary_and_encrypted_content() {
        let openai = OpenAI::builder()
            .default_prompt_model("gpt-4.1")
            .default_options(Options::builder().reasoning_effort(ReasoningEffort::Low))
            .build()
            .unwrap();

        let request = ChatCompletionRequest::builder()
            .messages(vec![ChatMessage::User("hi".into())])
            .build()
            .unwrap();

        let create = build_responses_request_from_chat(&openai, &request).unwrap();

        assert_eq!(create.store, Some(false));
        assert_eq!(
            create.reasoning.as_ref().and_then(|r| r.summary),
            Some(ReasoningSummary::Auto)
        );
        assert!(
            create
                .include
                .as_ref()
                .is_some_and(|items| items.contains(&IncludeEnum::ReasoningEncryptedContent))
        );
    }

    #[test]
    fn test_chat_messages_to_input_items_keeps_tool_calls_without_content() {
        let tool_call = ToolCall::builder()
            .id("call_123")
            .name("lookup")
            .maybe_args(Some("{\"q\":\"rust\"}".to_string()))
            .build()
            .unwrap();

        let message = ChatMessage::Assistant(None, Some(vec![tool_call]));

        let items = chat_messages_to_input_items(&[message], true).expect("conversion succeeds");
        assert_eq!(items.len(), 1);

        let InputItem::Item(async_openai::types::responses::Item::FunctionCall(function_call)) =
            &items[0]
        else {
            panic!("expected function call item");
        };

        assert_eq!(function_call.call_id, "fc_123");
        assert_eq!(function_call.name, "lookup");
        assert_eq!(function_call.arguments, "{\"q\":\"rust\"}");
        assert_eq!(function_call.status, Some(OutputStatus::InProgress));
    }

    #[test]
    fn test_chat_messages_to_input_items_includes_reasoning_with_encrypted_content() {
        let message = ChatMessage::Reasoning(ReasoningItem {
            id: "reasoning_1".to_string(),
            summary: vec!["First".to_string(), "Second".to_string()],
            encrypted_content: Some("encrypted".to_string()),
            ..Default::default()
        });

        let items = chat_messages_to_input_items(&[message], true).expect("conversion succeeds");
        assert_eq!(items.len(), 1);

        let InputItem::Item(async_openai::types::responses::Item::Reasoning(reasoning_item)) =
            &items[0]
        else {
            panic!("expected reasoning item");
        };

        assert_eq!(reasoning_item.id, "reasoning_1");
        assert!(reasoning_item.summary.is_empty());
        assert_eq!(
            reasoning_item.encrypted_content.as_deref(),
            Some("encrypted")
        );
    }

    #[test]
    fn test_chat_messages_to_input_items_ignores_empty_assistant() {
        let message = ChatMessage::Assistant(None, None);

        let items = chat_messages_to_input_items(&[message], true).expect("conversion succeeds");
        assert!(items.is_empty());
    }

    #[test]
    fn test_tool_call_from_function_call_uses_id_when_call_id_missing() {
        let function_call = FunctionToolCall {
            arguments: String::new(),
            call_id: String::new(),
            name: "lookup".to_string(),
            id: Some("call_456".to_string()),
            status: Some(OutputStatus::Completed),
        };

        let tool_call = tool_call_from_function_call(&function_call).expect("tool call");
        assert_eq!(tool_call.id(), "call_456");
        assert_eq!(tool_call.name(), "lookup");
        assert!(tool_call.args().is_none());
    }

    #[test]
    fn test_collect_message_text_helpers_join_parts() {
        let output = vec![
            OutputItem::Message(output_message("msg_1", &["First", "Second"])),
            OutputItem::FunctionCall(FunctionToolCall {
                arguments: "{}".to_string(),
                call_id: "call".to_string(),
                name: "noop".to_string(),
                id: None,
                status: Some(OutputStatus::Completed),
            }),
            OutputItem::Message(output_message("msg_2", &["Third"])),
        ];

        let collected = collect_message_text_from_items(&output).expect("text present");
        assert_eq!(collected, "First\nSecond\nThird");

        let message = output_message("msg_single", &["Line one", "Line two"]);
        let collected_message =
            collect_message_text_from_message(&message).expect("message text present");
        assert_eq!(collected_message, "Line one\nLine two");
    }

    #[test]
    fn test_metadata_to_chat_completion_respects_existing_fields() {
        let metadata = response_with_message_tool_reasoning("metadata message");

        let mut empty = ChatCompletionResponse::default();
        metadata_to_chat_completion(&metadata, &mut empty).expect("metadata applies");
        assert_eq!(empty.message.as_deref(), Some("metadata message"));
        assert!(empty.tool_calls.is_some());
        assert!(empty.reasoning.is_some());
        assert!(empty.usage.is_some());

        let existing_tool = ToolCall::builder()
            .id("existing")
            .name("existing_tool")
            .maybe_args(Some("{\"keep\":true}".to_string()))
            .build()
            .unwrap();

        let existing_reasoning = ReasoningItem {
            id: "existing_reasoning".to_string(),
            summary: vec!["keep".to_string()],
            encrypted_content: None,
            ..Default::default()
        };

        let existing_usage = Usage {
            prompt_tokens: 1,
            completion_tokens: 1,
            total_tokens: 2,
        };

        let mut existing = ChatCompletionResponse::builder()
            .message("existing message")
            .tool_calls(vec![existing_tool.clone()])
            .reasoning(vec![existing_reasoning.clone()])
            .usage(existing_usage)
            .build()
            .unwrap();

        metadata_to_chat_completion(&metadata, &mut existing).expect("metadata applies");
        assert_eq!(existing.message.as_deref(), Some("existing message"));
        assert_eq!(
            existing
                .tool_calls
                .as_ref()
                .and_then(|calls| calls.first())
                .map(ToolCall::id),
            Some("existing")
        );
        assert_eq!(
            existing
                .reasoning
                .as_ref()
                .and_then(|items| items.first())
                .map(|item| item.id.as_str()),
            Some("existing_reasoning")
        );
        assert_eq!(
            existing.usage.as_ref().map(|usage| usage.total_tokens),
            Some(sample_usage().total_tokens)
        );
    }

    #[test]
    fn test_tool_output_preserves_structured_values() {
        let tool_call = ToolCall::builder()
            .id("fc_test")
            .name("demo")
            .maybe_args(Some("{\"ok\":true}".to_owned()))
            .build()
            .unwrap();

        let messages = vec![
            ChatMessage::ToolOutput(
                tool_call.clone(),
                ToolOutput::Stop(Some(json!({"foo": "bar"}))),
            ),
            ChatMessage::ToolOutput(
                tool_call.clone(),
                ToolOutput::FeedbackRequired(Some(json!({"nested": {"a": 1}}))),
            ),
            ChatMessage::ToolOutput(
                tool_call.clone(),
                ToolOutput::AgentFailed(Some(json!([1, 2, 3]))),
            ),
        ];

        let items = chat_messages_to_input_items(&messages, true).expect("conversion succeeds");
        assert_eq!(items.len(), 3);

        for (item, expected) in
            items
                .iter()
                .zip([r#"{"foo":"bar"}"#, r#"{"nested":{"a":1}}"#, r"[1,2,3]"])
        {
            let InputItem::Item(async_openai::types::responses::Item::FunctionCallOutput(
                function_output,
            )) = item
            else {
                panic!("expected function call output item");
            };

            assert_eq!(function_output.call_id, "fc_test");
            assert_eq!(
                function_output.output,
                FunctionCallOutput::Text(expected.to_string())
            );
        }
    }

    #[test]
    fn test_response_to_chat_completion_maps_outputs() {
        let usage = sample_usage();
        let response: Response = serde_json::from_value(json!({
            "created_at": 0,
            "id": "resp",
            "model": "gpt-4.1",
            "object": "response",
            "status": "completed",
            "output": [
                {
                    "type": "message",
                    "id": "msg",
                    "role": "assistant",
                    "status": "completed",
                    "content": [
                        {"type": "output_text", "text": "Assistant reply", "annotations": []}
                    ]
                },
                {
                    "type": "function_call",
                    "id": "tool",
                    "call_id": "tool",
                    "name": "get_weather",
                    "arguments": "{\"city\":\"Oslo\"}",
                    "status": "completed"
                }
            ],
            "usage": usage,
        }))
        .expect("valid response json");

        let completion = response_to_chat_completion(&response).unwrap();
        assert_eq!(completion.message(), Some("Assistant reply"));

        let tool_calls = completion.tool_calls().expect("tool calls present");
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name(), "get_weather");
        assert_eq!(tool_calls[0].args(), Some("{\"city\":\"Oslo\"}"));

        let usage = completion.usage.expect("usage");
        assert_eq!(usage.prompt_tokens, 5);
        assert_eq!(usage.completion_tokens, 3);
        assert_eq!(usage.total_tokens, 8);
    }

    #[test]
    fn test_response_to_chat_completion_collects_reasoning_summary_and_encrypted_content() {
        let usage = sample_usage();
        let response: Response = serde_json::from_value(json!({
            "created_at": 0,
            "id": "resp",
            "model": "gpt-4.1",
            "object": "response",
            "status": "completed",
            "output": [
                {
                    "type": "reasoning",
                    "id": "reasoning_1",
                    "summary": [
                        {"type": "summary_text", "text": "First"},
                        {"type": "summary_text", "text": "Second"}
                    ],
                    "encrypted_content": "encrypted"
                }
            ],
            "usage": usage,
        }))
        .expect("valid response json");

        let completion = response_to_chat_completion(&response).unwrap();
        let reasoning = completion.reasoning.expect("reasoning items present");

        assert_eq!(reasoning.len(), 1);
        assert_eq!(reasoning[0].id, "reasoning_1");
        assert_eq!(
            reasoning[0].summary,
            vec!["First".to_string(), "Second".to_string()]
        );
        assert_eq!(reasoning[0].encrypted_content.as_deref(), Some("encrypted"));
    }

    #[test]
    fn test_stream_accumulator_handles_text_and_tool_events() {
        let mut state = ResponsesStreamState::default();

        let delta: ResponseTextDeltaEvent = serde_json::from_value(json!({
            "sequence_number": 0,
            "item_id": "msg_1",
            "output_index": 0,
            "content_index": 0,
            "delta": "Hello"
        }))
        .unwrap();

        let chunk = expect_emit(
            &mut state,
            ResponseStreamEvent::ResponseOutputTextDelta(delta),
            false,
        );

        assert_eq!(
            chunk
                .response
                .delta
                .as_ref()
                .and_then(|d| d.message_chunk.as_deref()),
            Some("Hello")
        );

        let item_added: ResponseOutputItemAddedEvent = serde_json::from_value(json!({
            "sequence_number": 1,
            "output_index": 0,
            "item": {
                "type": "function_call",
                "id": "call",
                "call_id": "call",
                "name": "lookup",
                "arguments": "",
                "status": "in_progress"
            }
        }))
        .unwrap();

        expect_emit(
            &mut state,
            ResponseStreamEvent::ResponseOutputItemAdded(item_added),
            false,
        );

        let args_delta: ResponseFunctionCallArgumentsDeltaEvent = serde_json::from_value(json!({
            "sequence_number": 2,
            "item_id": "call",
            "output_index": 0,
            "delta": "{\"q\":\"rust\"}"
        }))
        .unwrap();

        expect_emit(
            &mut state,
            ResponseStreamEvent::ResponseFunctionCallArgumentsDelta(args_delta),
            false,
        );

        let args_done: ResponseFunctionCallArgumentsDoneEvent = serde_json::from_value(json!({
            "sequence_number": 3,
            "item_id": "call",
            "output_index": 0,
            "name": "lookup",
            "arguments": "{\"q\":\"rust\"}"
        }))
        .unwrap();

        expect_emit(
            &mut state,
            ResponseStreamEvent::ResponseFunctionCallArgumentsDone(args_done),
            false,
        );

        let usage = sample_usage();
        let completed: ResponseCompletedEvent = serde_json::from_value(json!({
            "sequence_number": 4,
            "response": {
                "id": "resp",
                "object": "response",
                "created_at": 0,
                "status": "completed",
                "model": "gpt-4.1",
                "output": [],
                "usage": to_value(&usage).unwrap()
            }
        }))
        .unwrap();

        let final_chunk = expect_emit(
            &mut state,
            ResponseStreamEvent::ResponseCompleted(completed),
            false,
        );
        assert!(final_chunk.finished);

        assert_eq!(final_chunk.response.message(), Some("Hello"));

        let tool_calls = final_chunk
            .response
            .tool_calls()
            .expect("tool calls present");
        assert_eq!(tool_calls[0].name(), "lookup");
        assert_eq!(tool_calls[0].args(), Some("{\"q\":\"rust\"}"));

        let usage = final_chunk.response.usage.expect("usage");
        assert_eq!(usage.total_tokens, 8);
    }

    #[test]
    fn test_stream_state_take_final_only_once() {
        let mut state = ResponsesStreamState::default();
        assert!(state.take_final(true).is_some());
        assert!(state.take_final(true).is_none());
    }

    #[test]
    fn test_stream_state_ignores_events_after_completion() {
        let mut state = ResponsesStreamState::default();

        let usage = sample_usage();
        let completed: ResponseCompletedEvent = serde_json::from_value(json!({
            "sequence_number": 0,
            "response": {
                "id": "resp",
                "object": "response",
                "created_at": 0,
                "status": "completed",
                "model": "gpt-4.1",
                "output": [],
                "usage": to_value(&usage).unwrap()
            }
        }))
        .unwrap();

        let finished = expect_emit(
            &mut state,
            ResponseStreamEvent::ResponseCompleted(completed),
            false,
        );
        assert!(finished.finished);

        let delta: ResponseTextDeltaEvent = serde_json::from_value(json!({
            "sequence_number": 1,
            "item_id": "msg_1",
            "output_index": 0,
            "content_index": 0,
            "delta": "ignored"
        }))
        .unwrap();

        expect_no_emit(
            &mut state,
            ResponseStreamEvent::ResponseOutputTextDelta(delta),
            false,
        );
    }

    #[test]
    fn test_stream_state_message_item_added_collects_text() {
        let mut state = ResponsesStreamState::default();

        let item_added: ResponseOutputItemAddedEvent = serde_json::from_value(json!({
            "sequence_number": 0,
            "output_index": 0,
            "item": {
                "type": "message",
                "id": "msg",
                "role": "assistant",
                "status": "completed",
                "content": [
                    {"type": "output_text", "text": "Hello", "annotations": []},
                    {"type": "output_text", "text": "World", "annotations": []}
                ]
            }
        }))
        .unwrap();

        let chunk = expect_emit(
            &mut state,
            ResponseStreamEvent::ResponseOutputItemAdded(item_added),
            true,
        );

        assert_eq!(chunk.response.message(), Some("Hello\nWorld"));
    }

    #[test]
    fn test_stream_state_output_item_done_emits_tool_call() {
        let mut state = ResponsesStreamState::default();

        let item_added: ResponseOutputItemAddedEvent = serde_json::from_value(json!({
            "sequence_number": 0,
            "output_index": 0,
            "item": {
                "type": "function_call",
                "id": "call",
                "call_id": "call",
                "name": "lookup",
                "arguments": "",
                "status": "in_progress"
            }
        }))
        .unwrap();

        expect_emit(
            &mut state,
            ResponseStreamEvent::ResponseOutputItemAdded(item_added),
            true,
        );

        let done: ResponseOutputItemDoneEvent = serde_json::from_value(json!({
            "sequence_number": 1,
            "output_index": 0,
            "item": {
                "type": "function_call",
                "id": "call-id",
                "call_id": "",
                "name": "lookup",
                "arguments": "",
                "status": "completed"
            }
        }))
        .unwrap();

        let chunk = expect_emit(
            &mut state,
            ResponseStreamEvent::ResponseOutputItemDone(done),
            true,
        );

        let calls = chunk.response.tool_calls().expect("tool calls present");
        assert_eq!(calls[0].id(), "call");
        assert_eq!(calls[0].name(), "lookup");
    }

    #[test]
    fn test_stream_state_duplicate_arguments_done_no_emit() {
        let mut state = ResponsesStreamState::default();

        let item_added: ResponseOutputItemAddedEvent = serde_json::from_value(json!({
            "sequence_number": 0,
            "output_index": 0,
            "item": {
                "type": "function_call",
                "id": "call",
                "call_id": "call",
                "name": "lookup",
                "arguments": "",
                "status": "in_progress"
            }
        }))
        .unwrap();
        expect_emit(
            &mut state,
            ResponseStreamEvent::ResponseOutputItemAdded(item_added),
            false,
        );

        let args_delta: ResponseFunctionCallArgumentsDeltaEvent = serde_json::from_value(json!({
            "sequence_number": 1,
            "item_id": "call",
            "output_index": 0,
            "delta": "{\"q\":1}"
        }))
        .unwrap();
        expect_emit(
            &mut state,
            ResponseStreamEvent::ResponseFunctionCallArgumentsDelta(args_delta),
            false,
        );

        let args_done: ResponseFunctionCallArgumentsDoneEvent = serde_json::from_value(json!({
            "sequence_number": 2,
            "item_id": "call",
            "output_index": 0,
            "arguments": "{\"q\":1}",
            "name": ""
        }))
        .unwrap();

        expect_no_emit(
            &mut state,
            ResponseStreamEvent::ResponseFunctionCallArgumentsDone(args_done),
            false,
        );
    }

    #[test]
    fn test_stream_state_response_failed_and_error() {
        let mut state = ResponsesStreamState::default();

        let failed: ResponseFailedEvent = serde_json::from_value(json!({
            "sequence_number": 0,
            "response": {
                "id": "resp",
                "object": "response",
                "created_at": 0,
                "status": "failed",
                "model": "gpt-4.1",
                "output": [],
                "error": {"code": "oops", "message": "boom"}
            }
        }))
        .unwrap();

        let err = state
            .apply_event(ResponseStreamEvent::ResponseFailed(failed), false)
            .unwrap_err();
        assert!(
            matches!(err, LanguageModelError::PermanentError(msg) if msg.to_string().contains("oops"))
        );

        let mut state = ResponsesStreamState::default();
        let err_event: ResponseErrorEvent = serde_json::from_value(json!({
            "sequence_number": 1,
            "message": "bad things"
        }))
        .unwrap();
        let err = state
            .apply_event(ResponseStreamEvent::ResponseError(err_event), false)
            .unwrap_err();
        assert!(
            matches!(err, LanguageModelError::PermanentError(msg) if msg.to_string().contains("bad things"))
        );
    }

    #[test]
    fn test_response_to_chat_completion_failed_status_errors() {
        let response: Response = serde_json::from_value(json!({
            "created_at": 0,
            "id": "resp",
            "model": "gpt-4.1",
            "object": "response",
            "status": "failed",
            "error": {"code": "oops", "message": "boom"},
            "output": []
        }))
        .unwrap();

        let err = response_to_chat_completion(&response).unwrap_err();
        assert!(
            matches!(err, LanguageModelError::PermanentError(msg) if msg.to_string().contains("oops"))
        );
    }

    #[test]
    fn test_convert_metadata_rejects_non_string_values() {
        let metadata = json!({"tag": 123});
        assert!(convert_metadata(&metadata).is_none());
    }

    #[test]
    fn test_base_request_args_runs_with_seed_and_presence_penalty() {
        let openai = OpenAI::builder()
            .default_prompt_model("gpt-4.1")
            .default_options(
                Options::builder()
                    .seed(7)
                    .presence_penalty(0.4)
                    .temperature(0.1),
            )
            .build()
            .unwrap();

        assert!(base_request_args(&openai, "gpt-4.1").is_ok());
    }

    #[test]
    fn test_normalize_responses_function_call_id() {
        assert_eq!(
            normalize_responses_function_call_id("call_12345"),
            "fc_12345"
        );
        assert_eq!(normalize_responses_function_call_id("fc_abc"), "fc_abc");
        assert_eq!(normalize_responses_function_call_id("custom"), "custom");
    }
}
