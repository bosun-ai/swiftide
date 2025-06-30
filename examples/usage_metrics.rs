//! Swiftide can emit usage metrics using `metrics-rs`.
//!
//! For metrics to be emitted, the `metrics` feature must be enabled.
//!
//! `metrics-rs` is a flexibly rust library that allows you to collect and publish metrics
//! anywhere. From the user side, you need to provide a recorder and handles. The library itself
//! provides several built-in for these, i.e. prometheus.
//!
//! In this example, we're indexing markdown and logging the usage metrics to stdout. For the
//! recording we're using the examples from metric-rs.
//!
//! Usage metrics are emitted embedding, prompt requests, and chat completions. They always include
//! the model used as metadata

use swiftide::{
    indexing::{
        self,
        loaders::FileLoader,
        transformers::{ChunkMarkdown, Embed},
    },
    integrations::{self, qdrant::Qdrant},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // tracing_subscriber::fmt::init();
    init_print_logger();

    let metric_metadata = HashMap::from([("example".to_string(), "metadata".to_string())]);
    let openai_client = integrations::openai::OpenAI::builder()
        .default_embed_model("text-embedding-3-small")
        .default_prompt_model("gpt-4.1-nano")
        // Metadata will be added to every metric
        .metric_metadata(metric_metadata)
        .build()?;

    indexing::Pipeline::from_loader(FileLoader::new(".").with_extensions(&["md"]))
        .then_chunk(ChunkMarkdown::from_chunk_range(10..512))
        .then_in_batch(Embed::new(openai_client.clone()).with_batch_size(10))
        .then_store_with(
            Qdrant::builder()
                .batch_size(50)
                .vector_size(1536)
                .collection_name("swiftide-examples-metrics")
                .build()?,
        )
        .run()
        .await?;
    Ok(())

    // (counter) registered key swiftide.usage.prompt_tokens with unit None and description "token
    // usage for the prompt" (counter) registered key swiftide.usage.completion_tokens with unit
    // None and description "token usage for the completion" (counter) registered key
    // swiftide.usage.total_tokens with unit None and description "total token usage"
    // counter increment for 'Key(swiftide.usage.prompt_tokens, [example = metadata, model =
    // text-embedding-3-small])': 356 counter increment for
    // 'Key(swiftide.usage.completion_tokens, [example = metadata, model =
    // text-embedding-3-small])': 0 counter increment for 'Key(swiftide.usage.total_tokens,
    // [example = metadata, model = text-embedding-3-small])': 356 counter increment for
    // 'Key(swiftide.usage.prompt_tokens, [example = metadata, model = text-embedding-3-small])':
    // 336 counter increment for 'Key(swiftide.usage.completion_tokens, [example = metadata,
    // model = text-embedding-3-small])': 0 counter increment for
    // 'Key(swiftide.usage.total_tokens, [example = metadata, model = text-embedding-3-small])': 336
    // counter increment for 'Key(swiftide.usage.prompt_tokens, [example = metadata, model =
    // text-embedding-3-small])': 251 counter increment for
    // 'Key(swiftide.usage.completion_tokens, [example = metadata, model =
    // text-embedding-3-small])': 0 counter increment for 'Key(swiftide.usage.total_tokens,
    // [example = metadata, model = text-embedding-3-small])': 251 counter increment for
    // 'Key(swiftide.usage.prompt_tokens, [example = metadata, model = text-embedding-3-small])':
    // 404 counter increment for 'Key(swiftide.usage.completion_tokens, [example = metadata,
    // model = text-embedding-3-small])': 0 counter increment for
    // 'Key(swiftide.usage.total_tokens, [example = metadata, model = text-embedding-3-small])': 404
    // counter increment for 'Key(swiftide.usage.prompt_tokens, [example = metadata, model =
    // text-embedding-3-small])': 329 counter increment for
    // 'Key(swiftide.usage.completion_tokens, [example = metadata, model =
    // text-embedding-3-small])': 0 counter increment for 'Key(swiftide.usage.total_tokens,
    // [example = metadata, model = text-embedding-3-small])': 329
}

// --- Copied from https://github.com/metrics-rs/metrics/blob/main/metrics/examples/basic.rs
use std::{collections::HashMap, sync::Arc};

use metrics::{Counter, CounterFn, Gauge, GaugeFn, Histogram, HistogramFn, Key, Recorder, Unit};
use metrics::{KeyName, Metadata, SharedString};

#[derive(Clone, Debug)]
struct PrintHandle(Key);

impl CounterFn for PrintHandle {
    fn increment(&self, value: u64) {
        println!("counter increment for '{}': {}", self.0, value);
    }

    fn absolute(&self, value: u64) {
        println!("counter absolute for '{}': {}", self.0, value);
    }
}

impl GaugeFn for PrintHandle {
    fn increment(&self, value: f64) {
        println!("gauge increment for '{}': {}", self.0, value);
    }

    fn decrement(&self, value: f64) {
        println!("gauge decrement for '{}': {}", self.0, value);
    }

    fn set(&self, value: f64) {
        println!("gauge set for '{}': {}", self.0, value);
    }
}

impl HistogramFn for PrintHandle {
    fn record(&self, value: f64) {
        println!("histogram record for '{}': {}", self.0, value);
    }
}

#[derive(Debug)]
struct PrintRecorder;

impl Recorder for PrintRecorder {
    fn describe_counter(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        println!(
            "(counter) registered key {} with unit {:?} and description {:?}",
            key_name.as_str(),
            unit,
            description
        );
    }

    fn describe_gauge(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        println!(
            "(gauge) registered key {} with unit {:?} and description {:?}",
            key_name.as_str(),
            unit,
            description
        );
    }

    fn describe_histogram(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        println!(
            "(histogram) registered key {} with unit {:?} and description {:?}",
            key_name.as_str(),
            unit,
            description
        );
    }

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        Counter::from_arc(Arc::new(PrintHandle(key.clone())))
    }

    fn register_gauge(&self, key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        Gauge::from_arc(Arc::new(PrintHandle(key.clone())))
    }

    fn register_histogram(&self, key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        Histogram::from_arc(Arc::new(PrintHandle(key.clone())))
    }
}

fn init_print_logger() {
    metrics::set_global_recorder(PrintRecorder).unwrap()
}
