use anyhow::Context as _;
use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::field::{Field, Visit};
use tracing::{Event, Id, Level, Metadata, Subscriber, span};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use uuid::Uuid;

use crate::langfuse_batch_manager::LangfuseBatchManager;
use crate::models::{
    IngestionEvent, ObservationBody, ObservationLevel, ObservationType, TraceBody,
};

#[derive(Default, Debug, Clone)]
pub struct SpanData {
    pub observation_id: String, // Langfuse requires ids to be UUID v4 strings
    pub name: String,
    pub start_time: String,
    pub level: ObservationLevel,
    pub metadata: serde_json::Map<String, Value>,
    pub parent_span_id: Option<u64>,
}

impl SpanData {
    pub fn get<T>(&self, key: &str) -> Option<T>
    where
        T: serde::de::DeserializeOwned,
    {
        self.metadata
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Returns metadata with all keys that do not start with "langfuse."
    #[must_use]
    pub fn remaining_metadata(&self) -> Option<serde_json::Map<String, Value>> {
        let mut metadata = self.metadata.clone();
        metadata.retain(|k, _| !k.starts_with("langfuse."));

        if metadata.is_empty() {
            None
        } else {
            Some(metadata)
        }
    }
}

impl From<serde_json::Map<String, Value>> for SpanData {
    fn from(metadata: serde_json::Map<String, Value>) -> Self {
        SpanData {
            metadata,
            ..Default::default()
        }
    }
}

pub fn map_level(level: &Level) -> ObservationLevel {
    use ObservationLevel::{Debug, Default, Error, Warning};
    match *level {
        Level::ERROR => Error,
        Level::WARN => Warning,
        Level::INFO => Default,
        Level::DEBUG => Debug,
        Level::TRACE => Debug,
    }
}

#[derive(Debug)]
pub struct SpanTracker {
    active_spans: HashMap<u64, (String, ObservationType)>,
    current_trace_id: Option<String>,
}

impl Default for SpanTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl SpanTracker {
    pub fn new() -> Self {
        Self {
            active_spans: HashMap::new(),
            current_trace_id: None,
        }
    }

    pub fn add_span(&mut self, span_id: u64, observation_id: String, ty: ObservationType) {
        self.active_spans.insert(span_id, (observation_id, ty));
    }

    pub fn get_span(&self, span_id: u64) -> Option<&(String, ObservationType)> {
        self.active_spans.get(&span_id)
    }

    pub fn remove_span(&mut self, span_id: u64) -> Option<(String, ObservationType)> {
        self.active_spans.remove(&span_id)
    }
}

#[derive(Clone)]
pub struct LangfuseLayer {
    pub batch_manager: LangfuseBatchManager,
    pub span_tracker: Arc<Mutex<SpanTracker>>,
}

fn observation_create_from(
    trace_id: &str,
    observation_id: &str,
    span_data: &SpanData,
    parent_observation_id: Option<String>,
) -> IngestionEvent {
    // Expect all langfuse values to be prefixed by "langfuse."
    // Extract the fields from the metadata

    // Metadata is all values without a langfuse prefix
    let metadata = span_data.remaining_metadata().map(Into::into);

    let start_time = span_data
        .get("langfuse.start_time")
        .unwrap_or(span_data.start_time.clone());

    let name = span_data.get("otel.name").unwrap_or(span_data.name.clone());

    IngestionEvent::new_observation_create(ObservationBody {
        id: Some(Some(observation_id.to_string())),
        trace_id: Some(Some(trace_id.to_string())),
        r#type: span_data
            .get("langfuse.type")
            .unwrap_or(ObservationType::Span),
        name: Some(Some(name)),
        start_time: Some(Some(start_time)),
        level: Some(span_data.level),
        parent_observation_id: Some(parent_observation_id),
        metadata: Some(metadata),
        model: Some(span_data.get("langfuse.model")),
        model_parameters: Some(span_data.get("langfuse.model_parameters")),
        input: Some(span_data.get("langfuse.input")),
        version: Some(span_data.get("langfuse.version")),
        output: Some(span_data.get("langfuse.output")),
        usage: span_data.get("langfuse.usage").map(Box::new),
        status_message: Some(span_data.get("langfuse.status_message")),
        environment: Some(span_data.get("langfuse.environment")),

        completion_start_time: None,
        end_time: None,
    })
}

impl LangfuseLayer {
    // Start the layer with a batch manager
    //
    // Note that the batch manager _must_ be started before using this layer.
    pub fn from_batch_manager(batch_manager: &LangfuseBatchManager) -> Self {
        let span_tracker = Arc::new(Mutex::new(SpanTracker::new()));

        Self {
            batch_manager: batch_manager.clone(),
            span_tracker,
        }
    }
    pub async fn flush(&self) -> anyhow::Result<()> {
        self.batch_manager
            .flush()
            .await
            .context("Failed to flush")?;

        Ok(())
    }

    pub async fn handle_span(&self, span_id: u64, span_data: SpanData) {
        let observation_id = span_data.observation_id.clone();

        let langfuse_ty = span_data
            .get("langfuse.type")
            .unwrap_or(ObservationType::Span);

        {
            let mut spans = self.span_tracker.lock().await;
            spans.add_span(span_id, observation_id.clone(), langfuse_ty);
        }

        // Get parent ID if it exists
        let parent_id = if let Some(parent_span_id) = span_data.parent_span_id {
            let spans = self.span_tracker.lock().await;
            spans.get_span(parent_span_id).cloned().map(|(id, _)| id)
        } else {
            None
        };

        let trace_id = self.ensure_trace_id().await;

        // Create the span observation
        let event = observation_create_from(&trace_id, &observation_id, &span_data, parent_id);

        self.batch_manager.add_event(event).await;
    }

    pub async fn handle_span_close(&self, span_id: u64) {
        let Some((observation_id, langfuse_type)) =
            self.span_tracker.lock().await.remove_span(span_id)
        else {
            return;
        };

        let trace_id = self.ensure_trace_id().await;

        let event = IngestionEvent::new_observation_update(ObservationBody {
            id: Some(Some(observation_id.clone())),
            r#type: langfuse_type,
            trace_id: Some(Some(trace_id.clone())),
            end_time: Some(Some(Utc::now().to_rfc3339())),
            ..Default::default()
        });
        self.batch_manager.add_event(event).await;
    }

    pub async fn ensure_trace_id(&self) -> String {
        let mut spans = self.span_tracker.lock().await;
        if let Some(id) = spans.current_trace_id.clone() {
            return id;
        }

        let trace_id = Uuid::new_v4().to_string();
        spans.current_trace_id = Some(trace_id.clone());

        let event = IngestionEvent::new_trace_create(TraceBody {
            id: Some(Some(trace_id.clone())),
            name: Some(Some(Utc::now().timestamp().to_string())),
            timestamp: Some(Some(Utc::now().to_rfc3339())),
            public: Some(Some(false)),
            ..Default::default()
        });
        self.batch_manager.add_event(event).await;

        trace_id
    }

    pub async fn handle_record(&self, span_id: u64, metadata: serde_json::Map<String, Value>) {
        let Some((observation_id, langfuse_type)) =
            self.span_tracker.lock().await.get_span(span_id).cloned()
        else {
            return;
        };

        let trace_id = self.ensure_trace_id().await;
        let metadata = SpanData::from(metadata);
        let remaining = metadata.remaining_metadata().map(Into::into);
        let event = IngestionEvent::new_observation_update(ObservationBody {
            id: Some(Some(observation_id.clone())),
            trace_id: Some(Some(trace_id.clone())),
            r#type: langfuse_type,
            metadata: Some(remaining),
            input: Some(metadata.get("langfuse.input")),
            output: Some(metadata.get("langfuse.output")),
            model: Some(metadata.get("langfuse.model")),
            model_parameters: Some(metadata.get("langfuse.model_parameters")),
            version: Some(metadata.get("langfuse.version")),
            usage: metadata.get("langfuse.usage").map(Box::new),
            status_message: Some(metadata.get("langfuse.status_message")),
            environment: Some(metadata.get("langfuse.environment")),
            ..Default::default()
        });

        self.batch_manager.add_event(event).await;
    }
}

impl<S> Layer<S> for LangfuseLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn enabled(&self, _metadata: &Metadata<'_>, _ctx: Context<'_, S>) -> bool {
        // Enable this layer for all spans and events
        true
    }

    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        let span_id = id.into_u64();

        let parent_span_id = ctx
            .span_scope(id)
            .and_then(|mut scope| scope.nth(1))
            .map(|parent| parent.id().into_u64());

        let mut visitor = JsonVisitor::new();
        attrs.record(&mut visitor);

        let span_data = SpanData {
            observation_id: Uuid::new_v4().to_string(),
            name: attrs.metadata().name().to_string(),
            start_time: Utc::now().to_rfc3339(),
            level: map_level(attrs.metadata().level()),
            metadata: visitor.recorded_fields,
            parent_span_id,
        };

        let layer = self.clone();
        tokio::spawn(async move { layer.handle_span(span_id, span_data).await });
    }

    fn on_close(&self, id: Id, _ctx: Context<'_, S>) {
        let span_id = id.into_u64();
        let layer = self.clone();

        tokio::spawn(async move { layer.handle_span_close(span_id).await });
    }

    fn on_record(&self, span: &Id, values: &span::Record<'_>, _ctx: Context<'_, S>) {
        let span_id = span.into_u64();
        let mut visitor = JsonVisitor::new();
        values.record(&mut visitor);
        let metadata = visitor.recorded_fields;

        if !metadata.is_empty() {
            let layer = self.clone();
            tokio::spawn(async move { layer.handle_record(span_id, metadata).await });
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let mut visitor = JsonVisitor::new();
        event.record(&mut visitor);
        let metadata = visitor.recorded_fields;

        if let Some(span_id) = ctx.lookup_current().map(|span| span.id().into_u64()) {
            let layer = self.clone();
            tokio::spawn(async move { layer.handle_record(span_id, metadata).await });
        }
    }
}

#[derive(Debug)]
struct JsonVisitor {
    recorded_fields: serde_json::Map<String, Value>,
}

impl JsonVisitor {
    fn new() -> Self {
        Self {
            recorded_fields: serde_json::Map::new(),
        }
    }

    fn insert_value(&mut self, field: &Field, value: Value) {
        self.recorded_fields.insert(field.name().to_string(), value);
    }
}

macro_rules! record_field {
    ($fn_name:ident, $type:ty) => {
        fn $fn_name(&mut self, field: &Field, value: $type) {
            self.insert_value(field, Value::from(value));
        }
    };
}

impl Visit for JsonVisitor {
    record_field!(record_i64, i64);
    record_field!(record_u64, u64);
    record_field!(record_bool, bool);
    record_field!(record_str, &str);

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.insert_value(field, Value::String(format!("{value:?}")));
    }
}
