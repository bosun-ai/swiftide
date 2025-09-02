use anyhow::Context as _;
use chrono::Utc;
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::str::FromStr as _;
use std::sync::Arc;
use std::{env, fmt};
use tokio::sync::Mutex;
use tracing::field::{Field, Visit};
use tracing::{Event, Id, Level, Metadata, Subscriber, span};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use uuid::Uuid;

use crate::langfuse_batch_manager::{BatchManagerTrait, LangfuseBatchManager};
use crate::models::{
    IngestionEvent, ObservationBody, ObservationLevel, ObservationType, TraceBody,
};
use crate::{Configuration, DEFAULT_LANGFUSE_URL};

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
        if let Some(value) = self.metadata.get(key) {
            let parsed = serde_json::from_value(value.clone());
            if let Err(e) = &parsed {
                tracing::warn!(
                    error.msg = %e,
                    error.type = %std::any::type_name_of_val(e),
                    key = %key,
                    value = %value,
                    "[Langfuse] Failed to parse metadata field"
                );
            }

            return parsed.ok();
        }
        None
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
    pub batch_manager: Box<dyn BatchManagerTrait>,
    pub span_tracker: Arc<Mutex<SpanTracker>>,
}

fn observation_create_from(
    trace_id: &str,
    observation_id: &str,
    span_data: &mut SpanData,
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
    let swiftide_usage = span_data.get::<swiftide_core::chat_completion::Usage>("langfuse.usage");

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
        usage: swiftide_usage.map(|u| Box::new(u.into())),
        status_message: Some(span_data.get("langfuse.status_message")),
        environment: Some(span_data.get("langfuse.environment")),

        completion_start_time: None,
        end_time: None,
    })
}

impl Default for LangfuseLayer {
    fn default() -> Self {
        let public_key = env::var("LANGFUSE_PUBLIC_KEY")
            .or_else(|_| env::var("LANGFUSE_INIT_PROJECT_PUBLIC_KEY"))
            .unwrap_or_default();

        let secret_key = env::var("LANGFUSE_SECRET_KEY")
            .or_else(|_| env::var("LANGFUSE_INIT_PROJECT_SECRET_KEY"))
            .unwrap_or_default();

        if public_key.is_empty() || secret_key.is_empty() {
            panic!(
                "Public key or secret key not set. Please set LANGFUSE_PUBLIC_KEY and LANGFUSE_SECRET_KEY environment variables."
            );
        }

        let base_url =
            env::var("LANGFUSE_URL").unwrap_or_else(|_| DEFAULT_LANGFUSE_URL.to_string());

        let config = Configuration {
            base_path: base_url.clone(),
            user_agent: Some("swiftide".to_string()),
            client: Client::new(),
            basic_auth: Some((public_key.clone(), Some(secret_key.clone()))),
            ..Default::default()
        };

        let batch_manager = LangfuseBatchManager::new(config);

        batch_manager.clone().spawn();

        LangfuseLayer {
            batch_manager: batch_manager.boxed(),
            span_tracker: Arc::new(Mutex::new(SpanTracker::new())),
        }
    }
}
impl LangfuseLayer {
    // Builds the layer from an existing configuration
    pub fn from_config(config: Configuration) -> Self {
        let batch_manager = LangfuseBatchManager::new(config);

        batch_manager.clone().spawn();

        let span_tracker = Arc::new(Mutex::new(SpanTracker::new()));

        Self {
            batch_manager: batch_manager.boxed(),
            span_tracker,
        }
    }
    // Start the layer with a batch manager
    //
    // Note that the batch manager _must_ be started before using this layer.
    pub fn from_batch_manager(batch_manager: &LangfuseBatchManager) -> Self {
        let span_tracker = Arc::new(Mutex::new(SpanTracker::new()));

        Self {
            batch_manager: batch_manager.boxed(),
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

    pub async fn handle_span(&self, span_id: u64, mut span_data: SpanData) {
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
        let event = observation_create_from(&trace_id, &observation_id, &mut span_data, parent_id);

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
        let swiftide_usage =
            metadata.get::<swiftide_core::chat_completion::Usage>("langfuse.usage");
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
            usage: swiftide_usage.map(|u| Box::new(u.into())),
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

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.insert_value(field, Value::String(format!("{value:?}")));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        let value = Value::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()));
        self.insert_value(field, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::Mutex;
    use tracing::{Level, subscriber::set_global_default};
    use tracing_subscriber::prelude::*;

    #[derive(Clone)]
    struct InMemoryBatchManager {
        pub events: Arc<Mutex<Vec<crate::models::ingestion_event::IngestionEvent>>>,
    }
    #[async_trait::async_trait]
    impl crate::langfuse_batch_manager::BatchManagerTrait for InMemoryBatchManager {
        async fn add_event(&self, event: crate::models::ingestion_event::IngestionEvent) {
            self.events.lock().await.push(event);
        }
        async fn flush(&self) -> anyhow::Result<()> {
            Ok(())
        }
        fn boxed(&self) -> Box<dyn crate::langfuse_batch_manager::BatchManagerTrait + Send + Sync> {
            Box::new(Self {
                events: Arc::clone(&self.events),
            })
        }
    }

    #[test_log::test(tokio::test)]
    async fn test_generation_span_fields_are_correct_and_single_observation_created() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let batch_mgr = InMemoryBatchManager {
            events: Arc::clone(&events),
        };
        let langfuse_layer = LangfuseLayer {
            batch_manager: batch_mgr.boxed(),
            span_tracker: Arc::new(Mutex::new(SpanTracker::new())),
        };

        let (non_blocking, _guard) = tracing_appender::non_blocking(std::io::sink());
        let subscriber = tracing_subscriber::Registry::default()
            .with(langfuse_layer)
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(non_blocking)
                    .with_test_writer(),
            );

        set_global_default(subscriber).unwrap();

        let usage = swiftide_core::chat_completion::Usage {
            prompt_tokens: 5,
            completion_tokens: 9,
            total_tokens: 14,
        };

        // Start a GENERATION span, record fields, and drop/end.
        {
            let span = tracing::span!(
                Level::INFO,
                "prompt",
                langfuse.type = "GENERATION",
                langfuse.input = "sample-in",
                langfuse.output = "sample-out",
                langfuse.usage = serde_json::to_string(&usage).unwrap()

            );
            let _enter = span.enter();
            // Span ends here (dropped)
        }

        // Allow async processing to complete
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let events = events.lock().await;
        // There should be one observation create (and likely one trace, but we check for GENERATION
        // only)
        let generation_events: Vec<_> = events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    crate::models::ingestion_event::IngestionEvent::ObservationCreate(_)
                )
            })
            .collect();

        assert_eq!(generation_events.len(), 1);

        if let crate::models::ingestion_event::IngestionEvent::ObservationCreate(obs) =
            &generation_events[0]
        {
            let body = &obs.body;
            assert_eq!(body.r#type, crate::models::ObservationType::Generation);
            assert_eq!(body.input, Some(Some("sample-in".into())));
            assert_eq!(body.output, Some(Some("sample-out".into())));
            assert_eq!(
                body.usage
                    .as_ref()
                    .map(|b| serde_json::to_value(&**b).unwrap()),
                Some(serde_json::json!({"input": 5, "output": 9, "total": 14, "unit": "TOKENS"}))
            );
        } else {
            panic!("Did not capture a GENERATION observation as expected");
        }
    }
}
