use std::{collections::BTreeMap, sync::Arc};

use async_openai::types::responses as r;
use futures_util::{StreamExt as _, stream};
use serde_json::Value;
use swiftide_core::{
    ChatCompletionStream,
    chat_completion::{
        ChatCompletionRequest, ChatCompletionResponse, ReasoningItem, errors::LanguageModelError,
    },
};
use tracing_futures::Instrument;

use super::{
    Mistral, Response, Result, decode, output_to_completion, response_error, response_usage,
};

#[derive(Default)]
struct State {
    items: BTreeMap<u32, r::OutputItem>,
    response: ChatCompletionResponse,
    reasoning_delta: Option<Vec<ReasoningItem>>,
}

impl Mistral {
    pub(in super::super) async fn stream_responses(
        &self,
        request: &ChatCompletionRequest<'_>,
    ) -> Result<ChatCompletionStream> {
        let body = self.responses_request(request, true, None)?;
        let model = self
            .default_options
            .prompt_model
            .clone()
            .expect("validated model");
        let source = self
            .client
            .responses()
            .create_stream_byot::<_, Value>(&body)
            .await
            .map_err(|error| {
                self.track_responses_error(&model, &body, &error, None);
                super::mistral_error(error)
            })?;
        let client = self.clone();
        let body = Arc::new(body);
        let span = if cfg!(feature = "langfuse") {
            tracing::info_span!("responses_stream", langfuse.type = "GENERATION")
        } else {
            tracing::info_span!("responses_stream")
        };
        let stream = stream::unfold(
            (source, State::default(), false),
            move |(mut source, mut state, finished)| {
                let (client, body, model) = (client.clone(), body.clone(), model.clone());
                async move {
                    if finished {
                        return None;
                    }
                    let result = match source.next().await {
                        Some(Ok(raw)) => {
                            if let Some(usage) = response_usage(&raw) {
                                state.response.usage = Some(usage);
                            }
                            state.apply(&raw)
                        }
                        Some(Err(error)) => {
                            client.track_responses_error(
                                &model,
                                &body,
                                &error,
                                state.response.usage.as_ref(),
                            );
                            return Some((Err(super::mistral_error(error)), (source, state, true)));
                        }
                        None => Err(LanguageModelError::transient(
                            "Mistral Responses stream ended without a terminal response",
                        )),
                    };
                    match result {
                        Ok(terminal) => {
                            let snapshot = if terminal || client.stream_full {
                                state.response.clone()
                            } else {
                                ChatCompletionResponse {
                                    id: state.response.id,
                                    delta: state.response.delta.clone(),
                                    reasoning: state.reasoning_delta.take(),
                                    ..Default::default()
                                }
                            };
                            if terminal {
                                client.track_completion(
                                    &model,
                                    snapshot.usage.as_ref(),
                                    Some(body.as_ref()),
                                    Some(&snapshot),
                                );
                            }
                            Some((Ok(snapshot), (source, state, terminal)))
                        }
                        Err(error) => {
                            client.track_completion(
                                &model,
                                state.response.usage.as_ref(),
                                Some(body.as_ref()),
                                None::<&()>,
                            );
                            Some((Err(error), (source, state, true)))
                        }
                    }
                }
            },
        );
        Ok(Box::pin(stream.instrument(span)))
    }
}

impl State {
    fn apply(&mut self, raw: &Value) -> Result<bool> {
        self.response.delta = None;
        self.reasoning_delta = None;
        match raw["type"].as_str() {
            Some("response.completed" | "response.failed" | "response.incomplete") => {
                let response: Response = decode(&raw["response"])?;
                let id = self.response.id;
                let usage = self.response.usage.clone();
                self.response = response.into_completion()?;
                self.response.id = id;
                self.response.usage = self.response.usage.take().or(usage);
                return Ok(true);
            }
            Some("response.created" | "response.in_progress") => {
                // Decode the Mistral envelope separately from SDK lifecycle events, whose
                // timestamp fields require integers. No provider data is rewritten.
                let response: Response = decode(&raw["response"])?;
                if let Some(error) = response.error {
                    return Err(response_error(Some(&error.code), &error.message));
                }
                return Ok(false);
            }
            _ => {}
        }
        self.apply_event(decode(raw)?)?;
        let mut snapshot = output_to_completion(self.items.values())?;
        snapshot.id = self.response.id;
        snapshot.usage = self.response.usage.take();
        snapshot.delta = self.response.delta.take();
        self.response = snapshot;
        Ok(false)
    }

    #[allow(clippy::too_many_lines)]
    fn apply_event(&mut self, event: r::ResponseStreamEvent) -> Result<()> {
        use r::ResponseStreamEvent as E;
        match event {
            E::ResponseOutputItemAdded(event) => {
                if self.items.insert(event.output_index, event.item).is_some() {
                    return Err(invalid_event());
                }
            }
            E::ResponseOutputItemDone(event) => {
                self.items.insert(event.output_index, event.item);
            }
            E::ResponseContentPartAdded(event) => self.set_part(
                event.output_index,
                &event.item_id,
                event.content_index,
                event.part,
            )?,
            E::ResponseContentPartDone(event) => self.set_part(
                event.output_index,
                &event.item_id,
                event.content_index,
                event.part,
            )?,
            E::ResponseOutputTextDelta(event) => {
                let part = self.text_part(
                    event.output_index,
                    &event.item_id,
                    event.content_index,
                    false,
                )?;
                part.push_str(&event.delta);
                self.response.append_message_delta(Some(&event.delta));
            }
            E::ResponseOutputTextDone(event) => {
                *self.text_part(
                    event.output_index,
                    &event.item_id,
                    event.content_index,
                    false,
                )? = event.text;
            }
            E::ResponseReasoningTextDelta(event) => {
                self.text_part(
                    event.output_index,
                    &event.item_id,
                    event.content_index,
                    true,
                )?
                .push_str(&event.delta);
                self.reasoning_delta = Some(vec![ReasoningItem {
                    id: Some(event.item_id),
                    content: Some(vec![event.delta]),
                    ..Default::default()
                }]);
            }
            E::ResponseReasoningTextDone(event) => {
                *self.text_part(
                    event.output_index,
                    &event.item_id,
                    event.content_index,
                    true,
                )? = event.text;
            }
            E::ResponseFunctionCallArgumentsDelta(event) => {
                let r::OutputItem::FunctionCall(call) =
                    self.item(event.output_index, &event.item_id)?
                else {
                    return Err(invalid_event());
                };
                call.arguments.push_str(&event.delta);
                let (id, name) = (call.call_id.clone(), call.name.clone());
                self.response.append_tool_call_delta(
                    event.output_index as usize,
                    Some(&id),
                    Some(&name),
                    Some(&event.delta),
                );
            }
            E::ResponseFunctionCallArgumentsDone(event) => {
                let r::OutputItem::FunctionCall(call) =
                    self.item(event.output_index, &event.item_id)?
                else {
                    return Err(invalid_event());
                };
                call.arguments = event.arguments;
                if let Some(name) = event.name {
                    call.name = name;
                }
            }
            E::ResponseError(event) => {
                return Err(response_error(event.code.as_deref(), &event.message));
            }
            // Mistral's preview has no server-executed tools or summary streams. Fail explicitly
            // on unsupported events instead of silently losing model output.
            _ => {
                return Err(LanguageModelError::permanent(
                    "Unsupported Mistral Responses stream event",
                ));
            }
        }
        Ok(())
    }

    fn item(&mut self, index: u32, id: &str) -> Result<&mut r::OutputItem> {
        let item = self.items.get_mut(&index).ok_or_else(invalid_event)?;
        let actual = match item {
            r::OutputItem::Message(item) => Some(item.id.as_str()),
            r::OutputItem::Reasoning(item) => item.id.as_deref(),
            r::OutputItem::FunctionCall(item) => item.id.as_deref(),
            _ => None,
        };
        if actual != Some(id) {
            return Err(invalid_event());
        }
        Ok(item)
    }

    fn set_part(
        &mut self,
        index: u32,
        id: &str,
        part_index: u32,
        part: r::OutputContent,
    ) -> Result<()> {
        match (self.item(index, id)?, part) {
            (r::OutputItem::Message(item), r::OutputContent::OutputText(part)) => set_slot(
                &mut item.content,
                part_index,
                r::OutputMessageContent::OutputText(part),
            ),
            (r::OutputItem::Message(item), r::OutputContent::Refusal(part)) => set_slot(
                &mut item.content,
                part_index,
                r::OutputMessageContent::Refusal(part),
            ),
            (r::OutputItem::Reasoning(item), r::OutputContent::ReasoningText(part)) => set_slot(
                item.content.get_or_insert_default(),
                part_index,
                r::ReasoningItemContent::ReasoningText(part),
            ),
            _ => Err(invalid_event()),
        }
    }

    fn text_part(
        &mut self,
        index: u32,
        id: &str,
        part_index: u32,
        reasoning: bool,
    ) -> Result<&mut String> {
        match self.item(index, id)? {
            r::OutputItem::Message(item) if !reasoning => {
                let r::OutputMessageContent::OutputText(part) = item
                    .content
                    .get_mut(part_index as usize)
                    .ok_or_else(invalid_event)?
                else {
                    return Err(invalid_event());
                };
                Ok(&mut part.text)
            }
            r::OutputItem::Reasoning(item) if reasoning => {
                let r::ReasoningItemContent::ReasoningText(part) = item
                    .content
                    .as_mut()
                    .and_then(|parts| parts.get_mut(part_index as usize))
                    .ok_or_else(invalid_event)?;
                Ok(&mut part.text)
            }
            _ => Err(invalid_event()),
        }
    }
}

fn set_slot<T>(parts: &mut Vec<T>, index: u32, part: T) -> Result<()> {
    let index = index as usize;
    if index == parts.len() {
        parts.push(part);
    } else if let Some(slot) = parts.get_mut(index) {
        *slot = part;
    } else {
        return Err(invalid_event());
    }
    Ok(())
}

fn invalid_event() -> LanguageModelError {
    LanguageModelError::permanent(
        "Mistral Responses event references an unexpected output item or content part",
    )
}
