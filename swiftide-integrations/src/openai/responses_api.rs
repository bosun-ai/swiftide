use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::{Context as _, Result};
use async_openai::types::ResponseFormatJsonSchema;
use async_openai::types::responses::{
    Content, CreateResponse, CreateResponseArgs, FunctionArgs, Input, InputContent, InputItem,
    InputMessageArgs, OutputContent, OutputItem, OutputMessage, OutputStatus, ReasoningConfigArgs,
    Response, ResponseEvent, ResponseMetadata, ResponseStream, Role, Status, TextConfig,
    TextResponseFormat, ToolChoice, ToolChoiceMode, ToolDefinition, Usage as ResponsesUsage,
};
use futures_util::Stream;
use serde_json::json;
use swiftide_core::chat_completion::{
    ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ToolCall, ToolOutput, ToolSpec,
    Usage, UsageBuilder,
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
                        let call_id = tool_call.id();
                        let id = normalize_responses_function_call_id(call_id);
                        let arguments = tool_call.args().unwrap_or_default().to_owned();
                        let function_call = async_openai::types::responses::FunctionCall {
                            id: id.clone(),
                            call_id: call_id.to_owned(),
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
                    ToolOutput::Stop(message) => message.clone().unwrap_or(serde_json::Value::Null),
                    ToolOutput::AgentFailed(message) => {
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
        event: ResponseEvent,
        stream_full: bool,
    ) -> LmResult<Option<ResponsesStreamItem>> {
        if self.finished {
            return Ok(None);
        }

        let maybe_item = match event {
            ResponseEvent::ResponseOutputTextDelta(delta) => {
                self.response
                    .append_message_delta(Some(delta.delta.as_str()));
                Some(self.emit(stream_full, false))
            }
            ResponseEvent::ResponseContentPartAdded(part) => {
                part.part.text.as_deref().map(|text| {
                    self.response.append_message_delta(Some(text));
                    self.emit(stream_full, false)
                })
            }
            ResponseEvent::ResponseOutputItemAdded(event) => match event.item {
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
            ResponseEvent::ResponseOutputItemDone(event) => {
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
            ResponseEvent::ResponseFunctionCallArgumentsDelta(delta) => {
                let index = delta.output_index as usize;
                self.response
                    .append_tool_call_delta(index, None, None, Some(delta.delta.as_str()));
                Some(self.emit(stream_full, false))
            }
            ResponseEvent::ResponseFunctionCallArgumentsDone(done) => {
                let index = done.output_index as usize;

                let name = (!done.name.is_empty()).then_some(done.name.as_str());

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
            ResponseEvent::ResponseCompleted(completed) => {
                metadata_to_chat_completion(&completed.response, &mut self.response)?;
                self.response.delta = None;
                self.finished = true;
                Some(self.emit(stream_full, true))
            }
            ResponseEvent::ResponseIncomplete(incomplete) => {
                metadata_to_chat_completion(&incomplete.response, &mut self.response)?;
                self.response.delta = None;
                self.finished = true;
                Some(self.emit(stream_full, true))
            }
            ResponseEvent::ResponseFailed(failed) => {
                self.finished = true;
                let message = failed.response.error.as_ref().map_or_else(
                    || "Responses API stream failed".to_string(),
                    |err| format!("{}: {}", err.code, err.message),
                );
                return Err(LanguageModelError::permanent(message));
            }
            ResponseEvent::ResponseError(error) => {
                self.finished = true;
                return Err(LanguageModelError::permanent(error.message));
            }
            _ => None,
        };

        Ok(maybe_item)
    }

    fn emit(&self, stream_full: bool, finished: bool) -> ResponsesStreamItem {
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
            response.usage.clone_from(&self.response.usage);
        }

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

    if let Some(text) = response.output_text.as_ref().filter(|s| !s.is_empty()) {
        builder.message(text.clone());
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

    if accumulator.message.is_none()
        && let Some(output) = metadata.output.as_ref()
        && let Some(text) = collect_message_text_from_items(output)
    {
        accumulator.message = Some(text);
    }

    if accumulator.tool_calls.is_none()
        && let Some(output) = metadata.output.as_ref()
    {
        let tool_calls = collect_tool_calls_from_items(output)?;
        if !tool_calls.is_empty() {
            accumulator.tool_calls = Some(tool_calls);
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
        if let OutputItem::Message(message) = item
            && let Some(text) = collect_message_text_from_message(message)
        {
            if !buffer.is_empty() {
                buffer.push('\n');
            }
            buffer.push_str(&text);
        }
    }

    if buffer.is_empty() {
        None
    } else {
        Some(buffer)
    }
}

fn collect_tool_calls(output: &[OutputContent]) -> LmResult<Vec<ToolCall>> {
    let calls = output.iter().filter_map(|item| match item {
        OutputContent::FunctionCall(function_call) => Some(function_call),
        _ => None,
    });

    tool_calls_from_iter(calls)
}

fn collect_tool_calls_from_items(output: &[OutputItem]) -> LmResult<Vec<ToolCall>> {
    let calls = output.iter().filter_map(|item| match item {
        OutputItem::FunctionCall(function_call) => Some(function_call),
        _ => None,
    });

    tool_calls_from_iter(calls)
}

fn tool_call_from_function_call(
    function_call: &async_openai::types::responses::FunctionCall,
) -> LmResult<ToolCall> {
    let id = if function_call.call_id.is_empty() {
        function_call.id.clone()
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
    I: IntoIterator<Item = &'a async_openai::types::responses::FunctionCall>,
{
    calls
        .into_iter()
        .map(tool_call_from_function_call)
        .collect::<Result<Vec<_>, _>>()
}

fn function_call_identifier(function_call: &async_openai::types::responses::FunctionCall) -> &str {
    if function_call.call_id.is_empty() {
        function_call.id.as_str()
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
        verbosity: None,
    });

    args.build().map_err(openai_error_to_language_model_error)
}

#[allow(clippy::items_after_statements)]
#[cfg(test)]
mod tests {
    use super::*;
    use async_openai::types::responses::{
        CompletionTokensDetails, Content, FunctionCall as ResponsesFunctionCall, OutputContent,
        OutputMessage, OutputStatus, OutputText, PromptTokensDetails, ResponseCompleted,
        ResponseFunctionCallArgumentsDelta, ResponseFunctionCallArgumentsDone,
        ResponseOutputItemAdded, ResponseOutputTextDelta, ToolDefinition, Usage as ResponsesUsage,
    };
    use serde_json::{json, to_value};
    use std::collections::HashSet;
    use swiftide_core::chat_completion::{ChatCompletionRequest, ChatMessage, ToolSpec};

    use crate::openai::{OpenAI, Options};

    fn expect_emit(
        state: &mut ResponsesStreamState,
        event: ResponseEvent,
        stream_full: bool,
    ) -> ResponsesStreamItem {
        state
            .apply_event(event, stream_full)
            .unwrap()
            .expect("expected emission")
    }

    fn expect_no_emit(state: &mut ResponsesStreamState, event: ResponseEvent, stream_full: bool) {
        assert!(
            state.apply_event(event, stream_full).unwrap().is_none(),
            "expected no emission"
        );
    }

    fn sample_usage() -> ResponsesUsage {
        ResponsesUsage {
            input_tokens: 5,
            input_tokens_details: PromptTokensDetails {
                audio_tokens: Some(0),
                cached_tokens: Some(0),
            },
            output_tokens: 3,
            output_tokens_details: CompletionTokensDetails {
                accepted_prediction_tokens: Some(0),
                audio_tokens: Some(0),
                reasoning_tokens: Some(0),
                rejected_prediction_tokens: Some(0),
            },
            total_tokens: 8,
        }
    }

    #[allow(dead_code)]
    #[derive(schemars::JsonSchema)]
    struct WeatherArgs {
        city: String,
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

        assert_eq!(create.model, "gpt-4.1");
        assert_eq!(create.user.as_deref(), Some("tester"));
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

        let Input::Items(items) = &create.input else {
            panic!("expected items input");
        };
        assert_eq!(items.len(), 1);

        let tools = create.tools.expect("tools present");
        assert_eq!(tools.len(), 1);
        assert_eq!(
            create.tool_choice,
            Some(ToolChoice::Mode(ToolChoiceMode::Auto))
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

        let ToolDefinition::Function(function) = &tools[0] else {
            panic!("expected function tool");
        };

        let additional_properties = function.parameters.get("additionalProperties").cloned();

        #[allow(dead_code)]
        #[derive(schemars::JsonSchema)]
        #[serde(deny_unknown_fields)]
        #[schemars(title = "WeatherArgs")]
        struct WeatherArgsCorrect {
            city: String,
        }

        assert_eq!(
            additional_properties,
            Some(serde_json::Value::Bool(false)),
            "OpenAI requires additionalProperties to be set to false for tool parameters, got {}",
            serde_json::to_string_pretty(&function.parameters).unwrap()
        );

        assert_eq!(
            function.parameters,
            serde_json::to_value(schemars::schema_for!(WeatherArgsCorrect)).unwrap()
        );
    }

    #[test]
    fn test_response_to_chat_completion_maps_outputs() {
        let usage = sample_usage();
        let response = Response {
            created_at: 0,
            error: None,
            id: "resp".into(),
            incomplete_details: None,
            instructions: None,
            max_output_tokens: None,
            metadata: None,
            model: "gpt-4.1".into(),
            object: "response".into(),
            output: vec![
                OutputContent::Message(OutputMessage {
                    content: vec![Content::OutputText(OutputText {
                        annotations: Vec::new(),
                        text: "Assistant reply".into(),
                    })],
                    id: "msg".into(),
                    role: Role::Assistant,
                    status: OutputStatus::Completed,
                }),
                OutputContent::FunctionCall(ResponsesFunctionCall {
                    id: "tool".into(),
                    call_id: "tool".into(),
                    name: "get_weather".into(),
                    arguments: "{\"city\":\"Oslo\"}".into(),
                    status: OutputStatus::Completed,
                }),
            ],
            output_text: Some("Assistant reply".into()),
            parallel_tool_calls: None,
            previous_response_id: None,
            reasoning: None,
            store: None,
            service_tier: None,
            status: Status::Completed,
            temperature: None,
            text: None,
            tool_choice: None,
            tools: None,
            top_p: None,
            truncation: None,
            usage: Some(usage.clone()),
            user: None,
        };

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
    fn test_stream_accumulator_handles_text_and_tool_events() {
        let mut state = ResponsesStreamState::default();

        let delta: ResponseOutputTextDelta = serde_json::from_value(json!({
            "sequence_number": 0,
            "item_id": "msg_1",
            "output_index": 0,
            "content_index": 0,
            "delta": "Hello"
        }))
        .unwrap();

        let chunk = expect_emit(
            &mut state,
            ResponseEvent::ResponseOutputTextDelta(delta),
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

        let item_added: ResponseOutputItemAdded = serde_json::from_value(json!({
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
            ResponseEvent::ResponseOutputItemAdded(item_added),
            false,
        );

        let args_delta: ResponseFunctionCallArgumentsDelta = serde_json::from_value(json!({
            "sequence_number": 2,
            "item_id": "call",
            "output_index": 0,
            "delta": "{\"q\":\"rust\"}"
        }))
        .unwrap();

        expect_emit(
            &mut state,
            ResponseEvent::ResponseFunctionCallArgumentsDelta(args_delta),
            false,
        );

        let args_done: ResponseFunctionCallArgumentsDone = serde_json::from_value(json!({
            "sequence_number": 3,
            "item_id": "call",
            "output_index": 0,
            "name": "lookup",
            "arguments": "{\"q\":\"rust\"}"
        }))
        .unwrap();

        expect_emit(
            &mut state,
            ResponseEvent::ResponseFunctionCallArgumentsDone(args_done),
            false,
        );

        let usage = sample_usage();
        let completed: ResponseCompleted = serde_json::from_value(json!({
            "sequence_number": 4,
            "response": {
                "id": "resp",
                "object": "response",
                "created_at": 0,
                "status": "completed",
                "model": "gpt-4.1",
                "usage": to_value(&usage).unwrap()
            }
        }))
        .unwrap();

        let final_chunk = expect_emit(
            &mut state,
            ResponseEvent::ResponseCompleted(completed),
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
        let completed: ResponseCompleted = serde_json::from_value(json!({
            "sequence_number": 0,
            "response": {
                "id": "resp",
                "object": "response",
                "created_at": 0,
                "status": "completed",
                "model": "gpt-4.1",
                "usage": to_value(&usage).unwrap()
            }
        }))
        .unwrap();

        let finished = expect_emit(
            &mut state,
            ResponseEvent::ResponseCompleted(completed),
            false,
        );
        assert!(finished.finished);

        let delta: ResponseOutputTextDelta = serde_json::from_value(json!({
            "sequence_number": 1,
            "item_id": "msg_1",
            "output_index": 0,
            "content_index": 0,
            "delta": "ignored"
        }))
        .unwrap();

        expect_no_emit(
            &mut state,
            ResponseEvent::ResponseOutputTextDelta(delta),
            false,
        );
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
