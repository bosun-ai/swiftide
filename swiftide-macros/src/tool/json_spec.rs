use super::{ParamOptions, ToolArgs};

pub fn json_spec(tool_name: &str, args: &ToolArgs) -> String {
    if args.param.is_empty() {
        serde_json::json!(
            {
                "name": tool_name,
                "description": args.description,
        })
        .to_string()
    } else {
        serde_json::json!(
            {
                "name": tool_name,
                "description": args.description,
                "parameters": serialize_params(&args.param),
        })
        .to_string()
    }
}

fn serialize_params(params: &[ParamOptions]) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for param in params {
        map.insert(
            param.name.clone(),
            serde_json::json!({
                "type": "string",
                "description": param.description,
            }),
        );
    }
    serde_json::Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_json_spec_no_params() {
        let tool_name = "test_tool";
        let args = ToolArgs {
            description: "A tool with no parameters".to_string(),
            param: vec![],
        };

        let result = json_spec(tool_name, &args);
        let expected = json!({
            "name": tool_name,
            "description": args.description,
        })
        .to_string();

        assert_eq!(result, expected);
    }

    #[test]
    fn test_json_spec_single_param() {
        let tool_name = "test_tool";
        let args = ToolArgs {
            description: "A tool with a single parameter".to_string(),
            param: vec![ParamOptions {
                name: "param1".to_string(),
                description: "The first parameter".to_string(),
            }],
        };

        let result = json_spec(tool_name, &args);
        let expected = json!({
            "name": tool_name,
            "description": args.description,
            "parameters": {
                "param1": {
                    "type": "string",
                    "description": "The first parameter"
                }
            }
        })
        .to_string();

        assert_eq!(result, expected);
    }

    #[test]
    fn test_json_spec_multiple_params() {
        let tool_name = "test_tool";
        let args = ToolArgs {
            description: "A tool with multiple parameters".to_string(),
            param: vec![
                ParamOptions {
                    name: "param1".to_string(),
                    description: "The first parameter".to_string(),
                },
                ParamOptions {
                    name: "param2".to_string(),
                    description: "The second parameter".to_string(),
                },
            ],
        };

        let result = json_spec(tool_name, &args);
        let expected = json!({
            "name": tool_name,
            "description": args.description,
            "parameters": {
                "param1": {
                    "type": "string",
                    "description": "The first parameter"
                },
                "param2": {
                    "type": "string",
                    "description": "The second parameter"
                }
            }
        })
        .to_string();

        assert_eq!(result, expected);
    }
}
