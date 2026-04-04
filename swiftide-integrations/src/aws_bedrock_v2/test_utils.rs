use aws_credential_types::Credentials;
use aws_sdk_bedrockruntime::{Client, Config, config::Region};
use aws_smithy_types::event_stream::{Header, HeaderValue, Message};
use serde_json::Value;

pub(crate) const TEST_MODEL_ID: &str = "bedrock-test-model";

pub(crate) fn bedrock_client_for_mock_server(endpoint_url: &str) -> Client {
    let config = Config::builder()
        .behavior_version_latest()
        .region(Region::new("us-east-1"))
        .credentials_provider(Credentials::for_tests())
        .endpoint_url(endpoint_url)
        .build();

    Client::from_conf(config)
}

pub(crate) fn converse_stream_event(event_type: &str, payload: &Value) -> Vec<u8> {
    let message = Message::new_from_parts(
        vec![
            Header::new(":message-type", HeaderValue::String("event".into())),
            Header::new(
                ":event-type",
                HeaderValue::String(event_type.to_owned().into()),
            ),
            Header::new(
                ":content-type",
                HeaderValue::String("application/json".into()),
            ),
        ],
        serde_json::to_vec(&payload).expect("serialize event payload"),
    );

    let mut bytes = Vec::new();
    aws_smithy_eventstream::frame::write_message_to(&message, &mut bytes)
        .expect("encode event stream frame");
    bytes
}

#[cfg(feature = "langfuse")]
pub(crate) type RecordedTracingEvent = std::collections::HashMap<String, String>;

#[cfg(feature = "langfuse")]
#[derive(Clone)]
struct EventCaptureLayer {
    events: std::sync::Arc<std::sync::Mutex<Vec<RecordedTracingEvent>>>,
}

#[cfg(feature = "langfuse")]
#[derive(Default)]
struct EventFieldVisitor {
    fields: RecordedTracingEvent,
}

#[cfg(feature = "langfuse")]
impl tracing::field::Visit for EventFieldVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.fields
            .insert(field.name().to_string(), format!("{value:?}"));
    }
}

#[cfg(feature = "langfuse")]
impl<S> tracing_subscriber::Layer<S> for EventCaptureLayer
where
    S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut visitor = EventFieldVisitor::default();
        event.record(&mut visitor);
        self.events.lock().unwrap().push(visitor.fields);
    }
}

#[cfg(feature = "langfuse")]
pub(crate) fn run_with_langfuse_event_capture<F, Fut, T>(
    future_factory: F,
) -> (T, Vec<RecordedTracingEvent>)
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    use tracing_subscriber::prelude::*;

    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(EventCaptureLayer {
        events: events.clone(),
    });
    let dispatch = tracing::Dispatch::new(subscriber);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");

    let result =
        tracing::dispatcher::with_default(&dispatch, || runtime.block_on(future_factory()));

    let recorded_events = events.lock().expect("event capture mutex").clone();

    (result, recorded_events)
}
