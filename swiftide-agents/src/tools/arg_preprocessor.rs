use std::borrow::Cow;

use serde_json::{Map, Value};

/// Preprocesses arguments for tool calls and tries to fix common errors
/// This must be infallible and the result is always forwarded to the tool
pub struct ArgPreprocessor;

impl ArgPreprocessor {
    pub fn preprocess(value: Option<&str>) -> Option<Cow<'_, str>> {
        Some(take_first_occurrence_in_object(value?))
    }
}

/// Strips duplicate keys from JSON objects
fn take_first_occurrence_in_object(value: &str) -> Cow<'_, str> {
    let Ok(parsed) = &serde_json::from_str(value) else {
        return Cow::Borrowed(value);
    };
    if let Value::Object(obj) = parsed {
        let mut new_map = Map::with_capacity(obj.len());
        for (k, v) in obj {
            // Only insert if we haven't seen this key yet.
            new_map.entry(k).or_insert(v.clone());
        }
        Cow::Owned(Value::Object(new_map).to_string())
    } else {
        // If the top-level isn't even an object, just pass it as is,
        // or decide how you want to handle that situation.
        Cow::Borrowed(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_preprocess_regular_json() {
        let input = json!({
            "key1": "value1",
            "key2": "value2"
        })
        .to_string();
        let expected = json!({
            "key1": "value1",
            "key2": "value2"
        });
        let result = ArgPreprocessor::preprocess(Some(&input));
        assert_eq!(result.as_deref(), Some(expected.to_string().as_str()));
    }

    #[test]
    fn test_preprocess_json_with_duplicate_keys() {
        let input = json!({
            "key1": "value1",
            "key1": "value2"
        })
        .to_string();
        let expected = json!({
            "key1": "value2"
        });
        let result = ArgPreprocessor::preprocess(Some(&input));
        assert_eq!(result.as_deref(), Some(expected.to_string().as_str()));
    }

    #[test]
    fn test_no_preprocess_invalid_json() {
        let input = "invalid json";
        let result = ArgPreprocessor::preprocess(Some(input));
        assert_eq!(result.as_deref(), Some(input));
    }

    #[test]
    fn test_no_input() {
        let result = ArgPreprocessor::preprocess(None);
        assert_eq!(result, None);
    }
}
