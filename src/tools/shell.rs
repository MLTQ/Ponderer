//! Shell command execution tool.
//!
//! Allows the agent to run shell commands on the host system.
//! Always requires approval (configurable via ApprovalGate).

use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;

use super::{Tool, ToolCategory, ToolContext, ToolOutput};

/// Default command timeout in seconds
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Maximum command timeout in seconds
const MAX_TIMEOUT_SECS: u64 = 300;

/// Maximum output size before truncation (bytes)
const MAX_OUTPUT_BYTES: usize = 100_000;

pub struct ShellTool;

impl ShellTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Execute a shell command on the host system. Returns stdout, stderr, and exit code. \
         Use this for system operations, file manipulation, git commands, package management, etc."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute (passed to /bin/sh -c)"
                },
                "working_directory": {
                    "type": "string",
                    "description": "Working directory for the command (defaults to agent's working directory)"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 30, max: 300)"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let command = match params["command"].as_str() {
            Some(cmd) => cmd,
            None => return Ok(ToolOutput::Error("Missing 'command' parameter".to_string())),
        };

        let working_dir = params["working_directory"]
            .as_str()
            .unwrap_or(&ctx.working_directory);

        let timeout_secs = params["timeout_secs"]
            .as_u64()
            .unwrap_or(DEFAULT_TIMEOUT_SECS)
            .min(MAX_TIMEOUT_SECS);

        tracing::info!(
            "ShellTool executing: {} (cwd: {}, timeout: {}s)",
            command,
            working_dir,
            timeout_secs
        );

        // Execute command
        let result = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            tokio::process::Command::new("/bin/sh")
                .arg("-c")
                .arg(command)
                .current_dir(working_dir)
                .output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let mut stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let mut stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);

                // Truncate if too large
                if stdout.len() > MAX_OUTPUT_BYTES {
                    stdout.truncate(MAX_OUTPUT_BYTES);
                    stdout.push_str("\n[stdout truncated]");
                }
                if stderr.len() > MAX_OUTPUT_BYTES {
                    stderr.truncate(MAX_OUTPUT_BYTES);
                    stderr.push_str("\n[stderr truncated]");
                }

                let mut result_text = String::new();
                result_text.push_str(&format!("Exit code: {}\n", exit_code));

                if !stdout.is_empty() {
                    result_text.push_str(&format!("\n--- stdout ---\n{}", stdout));
                }
                if !stderr.is_empty() {
                    result_text.push_str(&format!("\n--- stderr ---\n{}", stderr));
                }

                if exit_code == 0 {
                    Ok(ToolOutput::Text(result_text))
                } else {
                    // Still return the output even on non-zero exit — the agent needs to see errors
                    Ok(ToolOutput::Text(result_text))
                }
            }
            Ok(Err(e)) => Ok(ToolOutput::Error(format!(
                "Failed to execute command: {}",
                e
            ))),
            Err(_) => Ok(ToolOutput::Error(format!(
                "Command timed out after {} seconds",
                timeout_secs
            ))),
        }
    }

    fn requires_approval(&self) -> bool {
        true
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Shell
    }
}

impl Default for ShellTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> ToolContext {
        ToolContext {
            working_directory: "/tmp".to_string(),
            username: "test".to_string(),
            autonomous: false,
        }
    }

    #[tokio::test]
    async fn test_echo_command() {
        let tool = ShellTool::new();
        let params = serde_json::json!({"command": "echo hello"});
        let result = tool.execute(params, &test_ctx()).await.unwrap();

        match result {
            ToolOutput::Text(text) => {
                assert!(text.contains("Exit code: 0"));
                assert!(text.contains("hello"));
            }
            other => panic!("Expected Text, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_failing_command() {
        let tool = ShellTool::new();
        let params = serde_json::json!({"command": "false"});
        let result = tool.execute(params, &test_ctx()).await.unwrap();

        match result {
            ToolOutput::Text(text) => {
                assert!(!text.contains("Exit code: 0"));
            }
            other => panic!("Expected Text, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_missing_command_param() {
        let tool = ShellTool::new();
        let params = serde_json::json!({});
        let result = tool.execute(params, &test_ctx()).await.unwrap();
        assert!(matches!(result, ToolOutput::Error(_)));
    }

    #[test]
    fn test_requires_approval() {
        let tool = ShellTool::new();
        assert!(tool.requires_approval());
    }

    #[test]
    fn test_schema_has_command() {
        let tool = ShellTool::new();
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["command"].is_object());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("command")));
    }
}
