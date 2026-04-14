/// Neural translation backend via Ollama HTTP API.
///
/// Provides neural code translation as an alternative to rule-based codegen.
/// Includes output validation to detect echoing, Nebula leakage, and hallucination.
/// Falls back gracefully when Ollama is not available.

use serde_json::{json, Value};

const FALLBACK_OLLAMA_URL: &str = "http://localhost:11434";
const FALLBACK_MODEL: &str = "nebula-8lang-14b-q4";

fn ollama_url() -> String {
    std::env::var("NEBULA_OLLAMA_URL").unwrap_or_else(|_| FALLBACK_OLLAMA_URL.to_string())
}

fn default_model() -> String {
    std::env::var("NEBULA_OLLAMA_MODEL").unwrap_or_else(|_| FALLBACK_MODEL.to_string())
}

/// Check if Ollama is running and reachable.
pub fn is_available() -> bool {
    ureq::get(&format!("{}/api/tags", ollama_url()))
        .timeout(std::time::Duration::from_secs(2))
        .call()
        .is_ok()
}

/// Generate target language code from Nebula source using Ollama.
pub fn generate(nebula_code: &str, target_lang: &str, model: Option<&str>) -> Result<String, String> {
    let default = default_model();
    let model = model.unwrap_or(&default);
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
            "num_predict": 1024
        }
    });

    let resp = ureq::post(&format!("{}/api/generate", ollama_url()))
        .timeout(std::time::Duration::from_secs(60))
        .send_json(&payload)
        .map_err(|e| format!("Ollama request failed: {}", e))?;

    let body: Value = resp.into_json()
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let raw = body["response"].as_str()
        .ok_or_else(|| "No response field in Ollama output".to_string())?;

    Ok(clean_output(raw))
}

/// Clean neural model output: strip markdown fences, trailing garbage, hallucination.
pub fn clean_output(output: &str) -> String {
    let mut s = output.trim().to_string();

    // Strip markdown code fences
    if s.starts_with("```") {
        if let Some(newline) = s.find('\n') {
            s = s[newline + 1..].to_string();
        }
    }
    if s.ends_with("```") {
        s = s[..s.len() - 3].trim_end().to_string();
    }

    // Strip "nebula\n" prefix if model echoed the language name
    if s.starts_with("nebula\n") {
        s = s[7..].to_string();
    }

    // Truncate at hallucination markers
    for marker in &["<|im_start|>", "<|im_end|>", "<|endoftext|>",
                     "BitFields", "Translate this", "\nuser\n", "\nassistant\n",
                     "\nYou are a code translator", "You are a code translator"] {
        if let Some(idx) = s.find(marker) {
            if idx > 0 {
                s = s[..idx].to_string();
            }
        }
    }

    // Truncate at first non-ASCII character (CJK hallucination)
    if let Some(idx) = s.find(|c: char| c as u32 > 127 && !"\u{201c}\u{201d}\u{2018}\u{2019}\u{2014}\u{2013}".contains(c)) {
        s = s[..idx].to_string();
    }

    s.trim_end().to_string()
}

/// Validate neural output — returns true if the output looks like valid target code.
pub fn validate_output(output: &str, target_lang: &str, input_nebula: &str) -> bool {
    let output = output.trim();

    // Reject empty or very short output
    if output.len() < 10 {
        return false;
    }

    // Echo detection: if output is too similar to the input Nebula, the model echoed
    let similarity = char_overlap(output, input_nebula);
    if similarity > 0.85 {
        return false;
    }

    // Nebula leak detection: check for Nebula-specific syntax in non-Nebula targets
    let target = target_lang.to_lowercase();
    if target != "nebula" {
        let nebula_markers = ["rt ", "fo ", "sf.", "lt ", "mt ", "wl "];
        let marker_count = nebula_markers.iter()
            .filter(|m| output.contains(**m))
            .count();
        // If multiple Nebula markers present, likely echoed
        if marker_count >= 3 {
            return false;
        }

        // Check for Nebula type names (only flag if NOT the target language's types)
        if target != "swift" && target != "kotlin" {
            // Float64/Bool/Int are valid in Swift/Kotlin but not in Python/Go/JS/C
            let type_leaks = ["Float64", "Bool "].iter()
                .filter(|t| output.contains(**t))
                .count();
            if type_leaks >= 2 {
                return false;
            }
        }
    }

    true
}

/// Character-level overlap ratio between two strings (0.0 - 1.0).
fn char_overlap(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let a_lines: std::collections::HashSet<&str> = a.lines().collect();
    let b_lines: std::collections::HashSet<&str> = b.lines().collect();
    let overlap = a_lines.intersection(&b_lines).count();
    let total = a_lines.len().max(b_lines.len());
    if total == 0 { 0.0 } else { overlap as f64 / total as f64 }
}
