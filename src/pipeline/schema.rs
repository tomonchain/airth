//! Checking a model's answer against the JSON schema its pipeline declares.
//! A small subset of JSON Schema, enough to describe an answer: `type`,
//! `properties`, `required`, `items` and `enum`. An answer that does not
//! fit is rejected, never repaired.

use serde_json::Value;

/// Why `value` does not fit `schema`, or None when it does.
pub fn mismatch(schema: &Value, value: &Value) -> Option<String> {
    at(schema, value, "answer")
}

fn at(schema: &Value, value: &Value, path: &str) -> Option<String> {
    if let Some(kind) = schema["type"].as_str() {
        let fits = match kind {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "boolean" => value.is_boolean(),
            "number" => value.is_number(),
            "integer" => value.is_i64() || value.is_u64(),
            "null" => value.is_null(),
            _ => true,
        };
        if !fits {
            return Some(format!("{path} should be {kind}, not {value}"));
        }
    }
    if let Some(allowed) = schema["enum"].as_array()
        && !allowed.contains(value)
    {
        return Some(format!("{path} is {value}, not one of {}", schema["enum"]));
    }
    if let Some(object) = value.as_object() {
        for key in schema["required"].as_array().into_iter().flatten() {
            if let Some(key) = key.as_str()
                && !object.contains_key(key)
            {
                return Some(format!("{path}.{key} is missing"));
            }
        }
        if let Some(properties) = schema["properties"].as_object() {
            for (key, sub) in properties {
                if let Some(v) = object.get(key)
                    && let Some(why) = at(sub, v, &format!("{path}.{key}"))
                {
                    return Some(why);
                }
            }
        }
    }
    if let Some(items) = value.as_array()
        && schema["items"].is_object()
    {
        for (i, item) in items.iter().enumerate() {
            if let Some(why) = at(&schema["items"], item, &format!("{path}[{i}]")) {
                return Some(why);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({
            "type": "object",
            "required": ["relevant", "pillar"],
            "properties": {
                "relevant": {"type": "boolean"},
                "pillar": {"type": "string", "enum": ["flows", "yields"]},
                "score": {"type": "integer"},
                "tags": {"type": "array", "items": {"type": "string"}}
            }
        })
    }

    #[test]
    fn accepts_an_answer_that_fits() {
        let ok =
            json!({"relevant": true, "pillar": "flows", "score": 3, "tags": ["eth"], "extra": 1});
        assert_eq!(mismatch(&schema(), &ok), None);
    }

    #[test]
    fn says_what_does_not_fit() {
        let why = |v: Value| mismatch(&schema(), &v).unwrap();
        assert!(why(json!([1])).contains("answer should be object"));
        assert!(why(json!({"relevant": true})).contains("answer.pillar is missing"));
        assert!(
            why(json!({"relevant": "yes", "pillar": "flows"}))
                .contains("answer.relevant should be boolean")
        );
        assert!(why(json!({"relevant": true, "pillar": "memes"})).contains("not one of"));
        assert!(
            why(json!({"relevant": true, "pillar": "flows", "score": 2.5})).contains("integer")
        );
        assert!(
            why(json!({"relevant": true, "pillar": "flows", "tags": ["a", 2]}))
                .contains("answer.tags[1]")
        );
    }
}
