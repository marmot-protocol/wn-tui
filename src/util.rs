use serde_json::Value;

/// Extract a named field from a JSON object and convert it to a hex string.
/// Handles plain strings, `{"value":{"vec":[u8,...]}}` objects, and direct `[u8,...]` arrays.
pub fn extract_hash_hex(obj: &Value, field: &str) -> Option<String> {
    value_to_hex(obj.get(field)?)
}

/// Convert a JSON value to a hex string.
/// Handles plain strings, `{"value":{"vec":[u8,...]}}` objects, and direct `[u8,...]` arrays.
pub fn value_to_hex(val: &Value) -> Option<String> {
    if let Some(s) = val.as_str() {
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }
    let arr = val
        .get("value")
        .and_then(|v| v.get("vec"))
        .and_then(|v| v.as_array())
        .or_else(|| val.as_array())?;
    let bytes: Vec<u8> = arr
        .iter()
        .map(|b| b.as_u64().and_then(|n| u8::try_from(n).ok()))
        .collect::<Option<Vec<_>>>()?;
    if bytes.is_empty() {
        None
    } else {
        Some(bytes.iter().map(|b| format!("{b:02x}")).collect())
    }
}
