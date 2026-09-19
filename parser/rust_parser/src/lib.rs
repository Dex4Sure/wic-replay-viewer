/// World in Conflict Replay Parser v6.0 (Rust) — WASM entry point
pub mod editor;
pub mod parser;

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "wasm")]
#[derive(serde::Serialize)]
struct WasmError {
    error: String,
}

#[cfg(feature = "wasm")]
fn error_json(error: impl std::fmt::Display) -> String {
    serde_json::to_string(&WasmError {
        error: error.to_string(),
    })
    .unwrap_or_else(|_| "{\"error\":\"Serialization failed\"}".to_string())
}

#[cfg(feature = "wasm")]
fn serialize_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|error| error_json(format!("Serialization failed: {error}")))
}

/// Parse a .wicdemo replay file from raw bytes.
/// Returns a JSON string matching the TypeScript ReplayData interface.
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn parse_replay_wasm(data: &[u8]) -> String {
    match parser::WicReplayParser::from_bytes(data) {
        Ok(parser) => serialize_json(&parser.parse()),
        Err(error) => error_json(error),
    }
}

/// Parse a replay into the versioned lightweight timeline only.
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn parse_replay_timeline_wasm(data: &[u8]) -> String {
    match parser::WicReplayParser::from_bytes(data) {
        Ok(parser) => serialize_json(&parser.parse_timeline()),
        Err(error) => error_json(error),
    }
}

/// Parse both the existing match result and the versioned lightweight timeline
/// in one decompression pass.
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn parse_replay_with_timeline_wasm(data: &[u8]) -> String {
    match parser::WicReplayParser::from_bytes(data) {
        Ok(parser) => serialize_json(&parser.parse_with_timeline()),
        Err(error) => error_json(error),
    }
}

#[cfg(all(test, feature = "wasm"))]
mod tests {
    use super::*;

    #[test]
    fn wasm_errors_are_always_valid_json() {
        let output = error_json("bad \"quoted\" path \\ and newline\n");
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid error JSON");

        assert_eq!(
            parsed,
            serde_json::json!({"error": "bad \"quoted\" path \\ and newline\n"})
        );
    }
}
