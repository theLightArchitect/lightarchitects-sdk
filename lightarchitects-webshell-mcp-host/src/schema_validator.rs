//! Common-subset JSON Schema validator.
//!
//! Handles: string (+ enum), boolean, integer, number, array (+ items), object
//! (+ required + properties). Unknown types pass through for forward compatibility.
//! Covers all 4 day-1 MCP server schemas without pulling in a full jsonschema crate.

use serde_json::Value;

use crate::McpHostError;

/// Validate `input` against the tool's declared input schema.
pub fn validate_input(
    schema: &Value,
    input: &Value,
    server: &str,
    tool: &str,
) -> Result<(), McpHostError> {
    check(schema, input).map_err(|reason| McpHostError::Scope {
        name: server.to_owned(),
        reason: format!("input schema violation for tool '{tool}': {reason}"),
    })
}

fn check(schema: &Value, value: &Value) -> Result<(), String> {
    match schema.get("type").and_then(Value::as_str) {
        Some("string") => {
            let s = value
                .as_str()
                .ok_or_else(|| format!("expected string, got {}", kind(value)))?;
            if let Some(enums) = schema.get("enum").and_then(Value::as_array) {
                if !enums.contains(value) {
                    return Err(format!("'{s}' not in enum {enums:?}"));
                }
            }
        }
        Some("boolean") => {
            if !value.is_boolean() {
                return Err(format!("expected boolean, got {}", kind(value)));
            }
        }
        Some("integer") => {
            if !value.is_i64() && !value.is_u64() {
                return Err(format!("expected integer, got {}", kind(value)));
            }
        }
        Some("number") => {
            if !value.is_number() {
                return Err(format!("expected number, got {}", kind(value)));
            }
        }
        Some("array") => {
            let arr = value
                .as_array()
                .ok_or_else(|| format!("expected array, got {}", kind(value)))?;
            if let Some(item_schema) = schema.get("items") {
                for (i, item) in arr.iter().enumerate() {
                    check(item_schema, item).map_err(|e| format!("[{i}]: {e}"))?;
                }
            }
        }
        Some("object") | None => check_object(schema, value)?,
        Some(_) => {} // Unknown type — forward-compatible pass
    }
    Ok(())
}

fn check_object(schema: &Value, value: &Value) -> Result<(), String> {
    let has_constraints = schema.get("required").is_some() || schema.get("properties").is_some();
    if !has_constraints {
        return Ok(());
    }
    let obj = value
        .as_object()
        .ok_or_else(|| format!("expected object, got {}", kind(value)))?;
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for r in required {
            let key = r.as_str().unwrap_or("");
            if !obj.contains_key(key) {
                return Err(format!("missing required field '{key}'"));
            }
        }
    }
    if let Some(props) = schema.get("properties").and_then(Value::as_object) {
        for (key, prop_schema) in props {
            if let Some(prop_val) = obj.get(key) {
                check(prop_schema, prop_val).map_err(|e| format!("'{key}': {e}"))?;
            }
        }
    }
    Ok(())
}

fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn required_field_missing_is_rejected() {
        let schema = json!({
            "type": "object",
            "required": ["content"],
            "properties": { "content": { "type": "string" } }
        });
        assert!(validate_input(&schema, &json!({}), "srv", "tool").is_err());
    }

    #[test]
    fn valid_object_passes() {
        let schema = json!({
            "type": "object",
            "required": ["content"],
            "properties": { "content": { "type": "string" } }
        });
        assert!(validate_input(&schema, &json!({"content": "hello"}), "srv", "tool").is_ok());
    }

    #[test]
    fn wrong_field_type_is_rejected() {
        let schema = json!({
            "type": "object",
            "properties": { "flag": { "type": "boolean" } }
        });
        let input = json!({ "flag": "not-a-bool" });
        assert!(validate_input(&schema, &input, "srv", "tool").is_err());
    }

    #[test]
    fn string_enum_accepts_valid_variant() {
        let schema = json!({ "type": "string", "enum": ["auto", "true", "false"] });
        assert!(validate_input(&schema, &json!("auto"), "srv", "tool").is_ok());
    }

    #[test]
    fn string_enum_rejects_unknown_variant() {
        let schema = json!({ "type": "string", "enum": ["auto", "true", "false"] });
        assert!(validate_input(&schema, &json!("yes"), "srv", "tool").is_err());
    }

    #[test]
    fn unknown_type_passes_through() {
        let schema = json!({ "type": "bytes" });
        assert!(validate_input(&schema, &json!("anything"), "srv", "tool").is_ok());
    }
}
