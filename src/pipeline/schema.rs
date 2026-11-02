//! Checking a model's answer against the JSON schema its pipeline declares.
//! A small subset of JSON Schema, enough to describe an answer: `type`,
//! `properties`, `required`, `items`, `enum`, `minimum`, `maximum`,
//! `minLength` and `maxLength`. An answer that does not fit is rejected,
//! never repaired.

use serde_json::Value;

/// The keywords the check enforces, and those that only describe.
const ENFORCED: &[&str] = &[
    "type",
    "properties",
    "required",
    "items",
    "enum",
    "minimum",
    "maximum",
    "minLength",
    "maxLength",
];
const DESCRIBING: &[&str] = &["description", "title", "$schema"];

/// The first keyword of `schema` that the check would not enforce, with
/// where it is: a schema must not promise a rule nobody checks.
pub fn unsupported(schema: &Value) -> Option<String> {
    keyword_at(schema, "schema")
}

fn keyword_at(schema: &Value, path: &str) -> Option<String> {
    let object = schema.as_object()?;
    if let Some(key) = object
        .keys()
        .find(|k| !ENFORCED.contains(&k.as_str()) && !DESCRIBING.contains(&k.as_str()))
    {
        return Some(format!(
            "{path} uses \"{key}\", which Airth does not check (it checks {})",
            ENFORCED.join(", ")
        ));
    }
    let properties = object.get("properties").and_then(Value::as_object);
    for (name, sub) in properties.into_iter().flatten() {
        if let Some(why) = keyword_at(sub, &format!("{path}.{name}")) {
            return Some(why);
        }
    }
    object
        .get("items")
        .and_then(|items| keyword_at(items, &format!("{path}.items")))
}

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
    if let Some(n) = value.as_f64() {
        if let Some(min) = schema["minimum"].as_f64()
            && n < min
        {
            return Some(format!("{path} is {value}, below the minimum {min}"));
        }
        if let Some(max) = schema["maximum"].as_f64()
            && n > max
        {
            return Some(format!("{path} is {value}, above the maximum {max}"));
        }
    }
    if let Some(text) = value.as_str() {
        // Characters, as the founder counts them, not bytes.
        let length = text.chars().count() as u64;
        if let Some(min) = schema["minLength"].as_u64()
            && length < min
        {
            return Some(format!("{path} has {length} characters, fewer than {min}"));
        }
        if let Some(max) = schema["maxLength"].as_u64()
            && length > max
        {
            return Some(format!("{path} has {length} characters, more than {max}"));
        }
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
                "score": {"type": "integer", "minimum": 1, "maximum": 5},
                "story": {"type": "string", "minLength": 2, "maxLength": 5},
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

    /// A keyword the check does not enforce is named, wherever it is; one
    /// that only describes is fine.
    #[test]
    fn names_a_keyword_it_does_not_check() {
        assert_eq!(unsupported(&schema()), None);
        let described = json!({"type": "object", "description": "One item.", "properties": {"a": {"type": "string", "title": "A"}}});
        assert_eq!(unsupported(&described), None);
        for (schema, at) in [
            (
                json!({"type": "object", "additionalProperties": false}),
                "schema uses \"additionalProperties\"",
            ),
            (
                json!({"properties": {"draft": {"type": "string", "pattern": "^[A-Z]"}}}),
                "schema.draft uses \"pattern\"",
            ),
            (
                json!({"properties": {"tags": {"items": {"format": "uri"}}}}),
                "schema.tags.items uses \"format\"",
            ),
        ] {
            let why = unsupported(&schema).unwrap();
            assert!(why.contains(at), "{why}");
        }
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
        let flows = |field: &str, v: Value| {
            let mut answer = json!({"relevant": true, "pillar": "flows"});
            answer[field] = v;
            mismatch(&schema(), &answer)
        };
        assert!(
            flows("score", json!(0))
                .unwrap()
                .contains("below the minimum 1")
        );
        assert!(
            flows("score", json!(6))
                .unwrap()
                .contains("above the maximum 5")
        );
        assert_eq!(flows("score", json!(5)), None);
        assert!(flows("story", json!("a")).unwrap().contains("fewer than 2"));
        assert!(
            flows("story", json!("abcdef"))
                .unwrap()
                .contains("more than 5")
        );
        // Characters, not bytes: five accented letters fit.
        assert_eq!(flows("story", json!("ééééé")), None);
        assert!(
            why(json!({"relevant": true, "pillar": "flows", "tags": ["a", 2]}))
                .contains("answer.tags[1]")
        );
    }
}
