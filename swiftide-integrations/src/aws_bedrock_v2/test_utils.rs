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
