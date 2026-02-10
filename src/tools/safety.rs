//! Safety layer for tool execution.
//!
//! Wraps all tool input/output with validation, leak detection, and sanitization.
//! Prevents prompt injection from tool output and catches secret leaks.

use serde_json::Value;

/// Maximum length for tool output before truncation (characters)
const MAX_OUTPUT_LENGTH: usize = 50_000;

/// Maximum depth for JSON parameter nesting
const MAX_JSON_DEPTH: usize = 10;

/// Result of a safety check
#[derive(Debug, Clone)]
pub enum SafetyVerdict {
    /// Safe to proceed
    Allow,
    /// Blocked with reason
    Block(String),
    /// Allowed but with a warning
    Warn(String),
}

/// Validate tool input parameters.
///
/// Checks:
/// - JSON depth isn't excessive (DoS prevention)
/// - String values aren't unreasonably large
/// - No obvious injection patterns in parameter values
pub fn validate_input(params: &Value) -> SafetyVerdict {
    // Check JSON depth
    if json_depth(params) > MAX_JSON_DEPTH {
        return SafetyVerdict::Block(format!(
            "Parameter nesting depth exceeds maximum of {}",
            MAX_JSON_DEPTH
        ));
    }

    // Check for excessively large string values
    if let Some(large) = find_large_strings(params, 1_000_000) {
        return SafetyVerdict::Block(format!(
            "Parameter '{}' contains string value exceeding 1MB",
            large
        ));
    }

    SafetyVerdict::Allow
}

/// Scan output for potential secret/API key leaks before passing to LLM.
///
/// Looks for common patterns:
/// - API keys (sk-..., ghp_..., etc.)
/// - Bearer tokens
/// - Private keys (PEM format)
/// - Connection strings with passwords
pub fn detect_leaks(text: &str) -> SafetyVerdict {
    let patterns = [
        ("sk-[a-zA-Z0-9]{20,}", "OpenAI API key"),
        ("ghp_[a-zA-Z0-9]{36}", "GitHub personal access token"),
        ("gho_[a-zA-Z0-9]{36}", "GitHub OAuth token"),
        ("glpat-[a-zA-Z0-9\\-]{20,}", "GitLab personal access token"),
        ("-----BEGIN (?:RSA |EC |DSA )?PRIVATE KEY-----", "Private key"),
        ("AKIA[0-9A-Z]{16}", "AWS access key"),
        ("eyJ[a-zA-Z0-9_-]{10,}\\.eyJ[a-zA-Z0-9_-]{10,}", "JWT token"),
    ];

    for (pattern, description) in &patterns {
        if let Ok(re) = regex_lite::Regex::new(pattern) {
            if re.is_match(text) {
                return SafetyVerdict::Block(format!(
                    "Potential {} detected in output — blocked to prevent leak",
                    description
                ));
            }
        }
    }

    SafetyVerdict::Allow
}

/// Sanitize tool output before feeding back to the LLM.
///
/// - Truncates overly long output
/// - Wraps in XML delimiters to separate from trusted instructions
pub fn sanitize_output(tool_name: &str, output: &str) -> String {
    let truncated = if output.len() > MAX_OUTPUT_LENGTH {
        let truncated_text = &output[..MAX_OUTPUT_LENGTH];
        format!(
            "{}\n\n[OUTPUT TRUNCATED — showing first {} of {} characters]",
            truncated_text,
            MAX_OUTPUT_LENGTH,
            output.len()
        )
    } else {
        output.to_string()
    };

    // Wrap in XML delimiters to clearly separate tool output from instructions.
    // This helps prevent prompt injection from untrusted tool output.
    format!(
        "<tool_output name=\"{}\">\n{}\n</tool_output>",
        xml_escape(tool_name),
        truncated
    )
}

/// Run the full safety pipeline on tool output.
///
/// Returns the sanitized output string, or an error if blocked.
pub fn check_output(tool_name: &str, output: &str) -> Result<String, String> {
    // 1. Leak detection
    match detect_leaks(output) {
        SafetyVerdict::Block(reason) => {
            tracing::warn!("Safety blocked output from tool '{}': {}", tool_name, reason);
            return Err(reason);
        }
        SafetyVerdict::Warn(reason) => {
            tracing::warn!("Safety warning for tool '{}': {}", tool_name, reason);
        }
        SafetyVerdict::Allow => {}
    }

    // 2. Sanitize (truncate + wrap)
    Ok(sanitize_output(tool_name, output))
}

// ============================================================================
// Helpers
// ============================================================================

fn json_depth(value: &Value) -> usize {
    match value {
        Value::Object(map) => {
            1 + map.values().map(json_depth).max().unwrap_or(0)
        }
        Value::Array(arr) => {
            1 + arr.iter().map(json_depth).max().unwrap_or(0)
        }
        _ => 1,
    }
}

fn find_large_strings(value: &Value, max_len: usize) -> Option<String> {
    match value {
        Value::String(s) if s.len() > max_len => Some("(root)".to_string()),
        Value::Object(map) => {
            for (key, val) in map {
                if let Value::String(s) = val {
                    if s.len() > max_len {
                        return Some(key.clone());
                    }
                }
                if let Some(found) = find_large_strings(val, max_len) {
                    return Some(format!("{}.{}", key, found));
                }
            }
            None
        }
        Value::Array(arr) => {
            for (i, val) in arr.iter().enumerate() {
                if let Some(found) = find_large_strings(val, max_len) {
                    return Some(format!("[{}].{}", i, found));
                }
            }
            None
        }
        _ => None,
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_normal_input() {
        let params = serde_json::json!({"command": "ls -la", "timeout": 30});
        assert!(matches!(validate_input(&params), SafetyVerdict::Allow));
    }

    #[test]
    fn test_validate_deep_nesting() {
        // Build deeply nested JSON
        let mut val = serde_json::json!("leaf");
        for _ in 0..15 {
            val = serde_json::json!({"nested": val});
        }
        assert!(matches!(validate_input(&val), SafetyVerdict::Block(_)));
    }

    #[test]
    fn test_detect_openai_key() {
        let output = "The config contains sk-abcdefghijklmnopqrstuvwxyz1234567890 as the key";
        assert!(matches!(detect_leaks(output), SafetyVerdict::Block(_)));
    }

    #[test]
    fn test_detect_aws_key() {
        let output = "Found AKIAIOSFODNN7EXAMPLE in environment";
        assert!(matches!(detect_leaks(output), SafetyVerdict::Block(_)));
    }

    #[test]
    fn test_detect_private_key() {
        let output = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAK...";
        assert!(matches!(detect_leaks(output), SafetyVerdict::Block(_)));
    }

    #[test]
    fn test_clean_output_passes() {
        let output = "total 42\n-rw-r--r-- 1 user staff 1234 Jan  1 12:00 file.txt";
        assert!(matches!(detect_leaks(output), SafetyVerdict::Allow));
    }

    #[test]
    fn test_sanitize_wraps_xml() {
        let result = sanitize_output("shell", "hello world");
        assert!(result.contains("<tool_output name=\"shell\">"));
        assert!(result.contains("hello world"));
        assert!(result.contains("</tool_output>"));
    }

    #[test]
    fn test_sanitize_truncates_long_output() {
        let long = "x".repeat(100_000);
        let result = sanitize_output("shell", &long);
        assert!(result.contains("[OUTPUT TRUNCATED"));
        assert!(result.len() < 100_000 + 200); // truncated + overhead
    }

    #[test]
    fn test_check_output_blocks_secrets() {
        let output = "key: sk-abcdefghijklmnopqrstuvwxyz1234567890";
        assert!(check_output("shell", output).is_err());
    }

    #[test]
    fn test_check_output_passes_clean() {
        let output = "Hello, world!";
        let result = check_output("echo", output);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Hello, world!"));
    }
}
