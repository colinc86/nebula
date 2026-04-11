/// Neural translation backend via Ollama HTTP API.
///
/// Provides neural code translation as an alternative to rule-based codegen.
/// Falls back gracefully when Ollama is not available.

use serde_json::{json, Value};

const OLLAMA_URL: &str = "http://localhost:11434";
const DEFAULT_MODEL: &str = "nebula-8lang-7b";

/// Check if Ollama is running and reachable.
pub fn is_available() -> bool {
    ureq::get(&format!("{}/api/tags", OLLAMA_URL))
        .timeout(std::time::Duration::from_secs(2))
        .call()
        .is_ok()
}

/// Generate target language code from Nebula source using Ollama.
pub fn generate(nebula_code: &str, target_lang: &str, model: Option<&str>) -> Result<String, String> {
    let model = model.unwrap_or(DEFAULT_MODEL);
    let system = format!(
        "You are a code translator. Given code in Nebula (a universal intermediate language), \
         produce the equivalent idiomatic {} code. Output only the {} code, no explanations.",
        target_lang, target_lang
    );

    let payload = json!({
        "model": model,
        "prompt": nebula_code,
        "system": system,
        "stream": false,
        "options": {
            "temperature": 0.1,
            "num_predict": 512
        }
    });

    let resp = ureq::post(&format!("{}/api/generate", OLLAMA_URL))
        .timeout(std::time::Duration::from_secs(60))
        .send_json(&payload)
        .map_err(|e| format!("Ollama request failed: {}", e))?;

    let body: Value = resp.into_json()
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    body["response"].as_str()
        .map(|s| s.trim().to_string())
        .ok_or_else(|| "No response field in Ollama output".to_string())
}
