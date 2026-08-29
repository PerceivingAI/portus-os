use serde_json::Value;

pub(crate) fn secret_like_key(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    [
        "authorization",
        "password",
        "passwd",
        "secret",
        "token",
        "api_key",
        "apikey",
        "cookie",
        "private_key",
        "credential",
    ]
    .iter()
    .any(|marker| value.contains(marker))
}

pub(crate) fn secret_like_text(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    [
        "authorization:",
        "bearer ",
        "api_key=",
        "api_key =",
        "apikey=",
        "password=",
        "password =",
        "secret=",
        "secret =",
        "token=",
        "token =",
        "private_key",
        "-----begin private key-----",
        "-----begin openssh private key-----",
    ]
    .iter()
    .any(|marker| value.contains(marker))
}

pub(crate) fn json_contains_secret_like(value: &Value) -> bool {
    match value {
        Value::Object(values) => values
            .iter()
            .any(|(key, value)| secret_like_key(key) || json_contains_secret_like(value)),
        Value::Array(values) => values.iter().any(json_contains_secret_like),
        Value::String(value) => secret_like_text(value),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_secret_shaped_keys_and_nested_values_without_rejecting_normal_tokens() {
        assert!(secret_like_key("access_token"));
        assert!(secret_like_text("Authorization: Bearer do-not-store"));
        assert!(json_contains_secret_like(&json!({
            "nested": [{"authorization": "opaque"}]
        })));
        assert!(json_contains_secret_like(&json!({
            "note": "token=do-not-store"
        })));
        assert!(!secret_like_text("token count is 12"));
        assert!(!json_contains_secret_like(&json!({
            "phase": "ready",
            "count": 12
        })));
    }
}
