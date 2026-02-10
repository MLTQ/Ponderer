//! Agentic tool-calling loop.
//!
//! Replaces the single-shot "decide and act" pattern with a multi-step loop:
//! 1. Build context (system prompt + conversation + tool definitions)
//! 2. Call LLM with function-calling format
//! 3. If LLM returns tool calls, execute them
//! 4. Feed results back to LLM
//! 5. Loop until LLM returns final text or max iterations reached

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::safety;
use super::{ToolCall, ToolContext, ToolDef, ToolOutput, ToolRegistry};

/// Configuration for the agentic loop
#[derive(Debug, Clone)]
pub struct AgenticConfig {
    /// Maximum iterations before stopping
    pub max_iterations: usize,
    /// LLM API URL
    pub api_url: String,
    /// LLM model name
    pub model: String,
    /// Optional API key
    pub api_key: Option<String>,
    /// Temperature for LLM calls
    pub temperature: f32,
    /// Max tokens per LLM response
    pub max_tokens: u32,
}

impl Default for AgenticConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            api_url: "http://localhost:11434/v1".to_string(),
            model: "llama3.2".to_string(),
            api_key: None,
            temperature: 0.7,
            max_tokens: 4096,
        }
    }
}

/// A message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<LlmToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Tool call as returned by the LLM (OpenAI format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: LlmFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmFunctionCall {
    pub name: String,
    pub arguments: String, // JSON string
}

/// The outcome of running the agentic loop
#[derive(Debug, Clone)]
pub struct AgenticResult {
    /// Final text response from the LLM (if any)
    pub response: Option<String>,
    /// All tool calls that were made during the loop
    pub tool_calls_made: Vec<ToolCallRecord>,
    /// Number of iterations used
    pub iterations: usize,
    /// Whether the loop hit the iteration limit
    pub hit_limit: bool,
}

/// Record of a tool call made during the loop
#[derive(Debug, Clone)]
pub struct ToolCallRecord {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub output: ToolOutput,
}

/// The agentic loop executor
pub struct AgenticLoop {
    config: AgenticConfig,
    registry: Arc<ToolRegistry>,
    client: reqwest::Client,
}

impl AgenticLoop {
    pub fn new(config: AgenticConfig, registry: Arc<ToolRegistry>) -> Self {
        Self {
            config,
            registry,
            client: reqwest::Client::new(),
        }
    }

    /// Run the agentic loop with the given system prompt and user message.
    ///
    /// The loop will continue until the LLM produces a final text response
    /// (no tool calls) or the maximum iteration count is reached.
    pub async fn run(
        &self,
        system_prompt: &str,
        user_message: &str,
        tool_ctx: &ToolContext,
    ) -> Result<AgenticResult> {
        self.run_with_history(system_prompt, vec![], user_message, tool_ctx)
            .await
    }

    /// Run the agentic loop with existing conversation history.
    pub async fn run_with_history(
        &self,
        system_prompt: &str,
        history: Vec<Message>,
        user_message: &str,
        tool_ctx: &ToolContext,
    ) -> Result<AgenticResult> {
        // Build initial messages
        let mut messages = vec![Message {
            role: "system".to_string(),
            content: Some(system_prompt.to_string()),
            tool_calls: None,
            tool_call_id: None,
        }];

        // Add history
        messages.extend(history);

        // Add current user message
        messages.push(Message {
            role: "user".to_string(),
            content: Some(user_message.to_string()),
            tool_calls: None,
            tool_call_id: None,
        });

        // Get tool definitions
        let tool_defs = self.registry.tool_definitions().await;

        let mut tool_calls_made = Vec::new();
        let mut iterations = 0;

        loop {
            iterations += 1;

            if iterations > self.config.max_iterations {
                tracing::warn!(
                    "Agentic loop hit iteration limit ({})",
                    self.config.max_iterations
                );
                return Ok(AgenticResult {
                    response: Some(format!(
                        "[Reached maximum of {} tool-calling iterations]",
                        self.config.max_iterations
                    )),
                    tool_calls_made,
                    iterations: iterations - 1,
                    hit_limit: true,
                });
            }

            // Call LLM
            tracing::debug!("Agentic loop iteration {} — calling LLM", iterations);
            let llm_response = self
                .call_llm(&messages, &tool_defs)
                .await
                .context("LLM call failed in agentic loop")?;

            // Check if LLM returned tool calls
            if let Some(ref tool_calls) = llm_response.tool_calls {
                if !tool_calls.is_empty() {
                    tracing::debug!(
                        "LLM requested {} tool call(s)",
                        tool_calls.len()
                    );

                    // Add assistant message with tool calls to history
                    messages.push(llm_response.clone());

                    // Execute each tool call
                    for tc in tool_calls {
                        let arguments: serde_json::Value =
                            serde_json::from_str(&tc.function.arguments).unwrap_or_else(|e| {
                                tracing::warn!(
                                    "Failed to parse tool arguments as JSON: {}",
                                    e
                                );
                                serde_json::json!({})
                            });

                        // Validate input
                        match safety::validate_input(&arguments) {
                            safety::SafetyVerdict::Block(reason) => {
                                let output = ToolOutput::Error(format!(
                                    "Input validation failed: {}",
                                    reason
                                ));
                                tool_calls_made.push(ToolCallRecord {
                                    tool_name: tc.function.name.clone(),
                                    arguments: arguments.clone(),
                                    output: output.clone(),
                                });
                                messages.push(Message {
                                    role: "tool".to_string(),
                                    content: Some(output.to_llm_string()),
                                    tool_calls: None,
                                    tool_call_id: Some(tc.id.clone()),
                                });
                                continue;
                            }
                            safety::SafetyVerdict::Warn(reason) => {
                                tracing::warn!("Safety warning for {}: {}", tc.function.name, reason);
                            }
                            safety::SafetyVerdict::Allow => {}
                        }

                        // Execute tool
                        let call = ToolCall {
                            name: tc.function.name.clone(),
                            arguments: arguments.clone(),
                        };

                        let result = self.registry.execute_call(&call, tool_ctx).await;

                        // Run output through safety pipeline
                        let safe_output = match &result.output {
                            ToolOutput::Text(text) => {
                                match safety::check_output(&tc.function.name, text) {
                                    Ok(sanitized) => sanitized,
                                    Err(reason) => {
                                        format!("[BLOCKED] {}", reason)
                                    }
                                }
                            }
                            ToolOutput::Json(val) => {
                                let text = serde_json::to_string_pretty(val)
                                    .unwrap_or_else(|_| val.to_string());
                                match safety::check_output(&tc.function.name, &text) {
                                    Ok(sanitized) => sanitized,
                                    Err(reason) => format!("[BLOCKED] {}", reason),
                                }
                            }
                            other => other.to_llm_string(),
                        };

                        tool_calls_made.push(ToolCallRecord {
                            tool_name: tc.function.name.clone(),
                            arguments,
                            output: result.output,
                        });

                        // Add tool result message
                        messages.push(Message {
                            role: "tool".to_string(),
                            content: Some(safe_output),
                            tool_calls: None,
                            tool_call_id: Some(tc.id.clone()),
                        });
                    }

                    // Continue loop — LLM will see tool results
                    continue;
                }
            }

            // No tool calls — LLM produced final text response
            let response_text = llm_response.content.clone();
            tracing::debug!(
                "Agentic loop completed in {} iteration(s)",
                iterations
            );

            return Ok(AgenticResult {
                response: response_text,
                tool_calls_made,
                iterations,
                hit_limit: false,
            });
        }
    }

    /// Call the LLM with the current messages and tool definitions.
    async fn call_llm(
        &self,
        messages: &[Message],
        tool_defs: &[ToolDef],
    ) -> Result<Message> {
        let url = format!("{}/chat/completions", self.config.api_url);

        let mut body = serde_json::json!({
            "model": self.config.model,
            "messages": messages,
            "temperature": self.config.temperature,
            "max_tokens": self.config.max_tokens,
        });

        // Only include tools if we have any
        if !tool_defs.is_empty() {
            body["tools"] = serde_json::to_value(tool_defs)?;
        }

        let mut req = self.client.post(&url).json(&body);

        if let Some(ref key) = self.config.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let response = req.send().await.context("Failed to send LLM request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("LLM API error {}: {}", status, body);
        }

        let response_json: serde_json::Value =
            response.json().await.context("Failed to parse LLM response")?;

        // Extract the assistant message from the response
        let choice = response_json["choices"]
            .as_array()
            .and_then(|arr| arr.first())
            .context("Empty choices in LLM response")?;

        let message = &choice["message"];

        // Parse into our Message type
        let content = message["content"].as_str().map(String::from);

        let tool_calls: Option<Vec<LlmToolCall>> = message
            .get("tool_calls")
            .and_then(|tc| serde_json::from_value(tc.clone()).ok());

        Ok(Message {
            role: "assistant".to_string(),
            content,
            tool_calls,
            tool_call_id: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_serialization() {
        let msg = Message {
            role: "user".to_string(),
            content: Some("Hello".to_string()),
            tool_calls: None,
            tool_call_id: None,
        };

        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"], "Hello");
        // tool_calls should be absent (skip_serializing_if = None)
        assert!(json.get("tool_calls").is_none());
    }

    #[test]
    fn test_tool_call_message_serialization() {
        let msg = Message {
            role: "assistant".to_string(),
            content: None,
            tool_calls: Some(vec![LlmToolCall {
                id: "call_123".to_string(),
                call_type: "function".to_string(),
                function: LlmFunctionCall {
                    name: "shell".to_string(),
                    arguments: r#"{"command": "ls"}"#.to_string(),
                },
            }]),
            tool_call_id: None,
        };

        let json = serde_json::to_value(&msg).unwrap();
        assert!(json.get("tool_calls").is_some());
        assert_eq!(json["tool_calls"][0]["function"]["name"], "shell");
    }

    #[test]
    fn test_tool_result_message_serialization() {
        let msg = Message {
            role: "tool".to_string(),
            content: Some("file1.txt\nfile2.txt".to_string()),
            tool_calls: None,
            tool_call_id: Some("call_123".to_string()),
        };

        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "tool");
        assert_eq!(json["tool_call_id"], "call_123");
    }

    #[test]
    fn test_agentic_config_default() {
        let config = AgenticConfig::default();
        assert_eq!(config.max_iterations, 10);
        assert_eq!(config.temperature, 0.7);
    }
}
