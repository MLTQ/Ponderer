//! Tool system for local capabilities (shell, files, HTTP, etc.)
//!
//! Tools are distinct from Skills:
//! - **Skills** are external integrations (Graphchan, Telegram, etc.) that produce events via polling
//! - **Tools** are local capabilities the agent can invoke during reasoning (shell, file ops, HTTP)
//!
//! Each tool declares a JSON Schema for its parameters, enabling LLM function-calling.
//! Tools are registered in a thread-safe ToolRegistry that generates OpenAI-format
//! function definitions for the LLM.

pub mod agentic;
pub mod safety;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Category of tool — used for grouping in UI and applying approval policies
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolCategory {
    /// File system operations (read, write, list, patch)
    FileSystem,
    /// Shell/command execution
    Shell,
    /// HTTP/network requests
    Network,
    /// Memory and knowledge management
    Memory,
    /// General purpose / uncategorized
    General,
}

/// The result of executing a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolOutput {
    /// Successful text output
    Text(String),
    /// Successful structured output
    Json(serde_json::Value),
    /// Tool execution failed
    Error(String),
    /// Tool needs user approval before proceeding
    NeedsApproval {
        tool: String,
        params: serde_json::Value,
        reason: String,
    },
}

impl ToolOutput {
    /// Convert to a string representation suitable for feeding back to the LLM
    pub fn to_llm_string(&self) -> String {
        match self {
            ToolOutput::Text(s) => s.clone(),
            ToolOutput::Json(v) => serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string()),
            ToolOutput::Error(e) => format!("[ERROR] {}", e),
            ToolOutput::NeedsApproval { tool, reason, .. } => {
                format!("[NEEDS APPROVAL] Tool '{}': {}", tool, reason)
            }
        }
    }

    /// Returns true if this output represents success (Text or Json)
    pub fn is_success(&self) -> bool {
        matches!(self, ToolOutput::Text(_) | ToolOutput::Json(_))
    }
}

/// Context passed to tools during execution
pub struct ToolContext {
    /// Current working directory for file/shell operations
    pub working_directory: String,
    /// The agent's username (for attribution)
    pub username: String,
    /// Whether the tool is running in autonomous mode (vs interactive with user present)
    pub autonomous: bool,
}

/// A tool provides the agent with a local capability.
///
/// Unlike Skills (which poll external services), Tools are invoked on-demand
/// by the agent during its reasoning loop. Each tool declares its parameters
/// as a JSON Schema, enabling LLM function-calling.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique name used in function-calling (e.g., "shell", "read_file")
    fn name(&self) -> &str;

    /// Human-readable description shown to the LLM
    fn description(&self) -> &str;

    /// JSON Schema describing the tool's parameters.
    ///
    /// This is used directly in OpenAI-format function definitions.
    /// Example:
    /// ```json
    /// {
    ///   "type": "object",
    ///   "properties": {
    ///     "command": { "type": "string", "description": "Shell command to execute" }
    ///   },
    ///   "required": ["command"]
    /// }
    /// ```
    fn parameters_schema(&self) -> serde_json::Value;

    /// Execute the tool with the given parameters.
    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput>;

    /// Whether this tool requires user approval before execution.
    ///
    /// Tools that modify the filesystem, execute commands, or make network
    /// requests should return true. Read-only tools can return false.
    fn requires_approval(&self) -> bool {
        false
    }

    /// Category for grouping and policy application
    fn category(&self) -> ToolCategory {
        ToolCategory::General
    }
}

/// OpenAI-format function definition for LLM function-calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// OpenAI-format tool definition (wraps FunctionDef)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDef,
}

/// A tool call parsed from LLM output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Result of a tool call, ready to feed back to the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub name: String,
    pub output: ToolOutput,
}

/// Thread-safe registry of tools available to the agent.
///
/// The registry provides:
/// - Tool lookup by name
/// - Generation of OpenAI-format function definitions for LLM prompts
/// - Dynamic registration/deregistration of tools
pub struct ToolRegistry {
    tools: RwLock<HashMap<String, Arc<dyn Tool>>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
        }
    }

    /// Register a tool. Overwrites any existing tool with the same name.
    pub async fn register(&self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        tracing::info!("Registered tool: {} (category: {:?})", name, tool.category());
        self.tools.write().await.insert(name, tool);
    }

    /// Remove a tool by name.
    pub async fn deregister(&self, name: &str) -> bool {
        self.tools.write().await.remove(name).is_some()
    }

    /// Get a tool by name.
    pub async fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.read().await.get(name).cloned()
    }

    /// List all registered tool names.
    pub async fn list_names(&self) -> Vec<String> {
        self.tools.read().await.keys().cloned().collect()
    }

    /// Generate OpenAI-format tool definitions for all registered tools.
    ///
    /// This output can be passed directly to the `tools` parameter
    /// of an OpenAI-compatible chat completions request.
    pub async fn tool_definitions(&self) -> Vec<ToolDef> {
        let tools = self.tools.read().await;
        tools
            .values()
            .map(|tool| ToolDef {
                tool_type: "function".to_string(),
                function: FunctionDef {
                    name: tool.name().to_string(),
                    description: tool.description().to_string(),
                    parameters: tool.parameters_schema(),
                },
            })
            .collect()
    }

    /// Execute a tool call, handling approval checks.
    ///
    /// Returns `ToolOutput::NeedsApproval` if the tool requires approval
    /// and the context indicates autonomous mode.
    pub async fn execute_call(
        &self,
        call: &ToolCall,
        ctx: &ToolContext,
    ) -> ToolCallResult {
        let tool = match self.get(&call.name).await {
            Some(t) => t,
            None => {
                return ToolCallResult {
                    name: call.name.clone(),
                    output: ToolOutput::Error(format!("Unknown tool: {}", call.name)),
                };
            }
        };

        // Check if approval is needed
        if tool.requires_approval() && ctx.autonomous {
            return ToolCallResult {
                name: call.name.clone(),
                output: ToolOutput::NeedsApproval {
                    tool: call.name.clone(),
                    params: call.arguments.clone(),
                    reason: format!(
                        "Tool '{}' requires approval in autonomous mode",
                        call.name
                    ),
                },
            };
        }

        // Execute the tool
        match tool.execute(call.arguments.clone(), ctx).await {
            Ok(output) => ToolCallResult {
                name: call.name.clone(),
                output,
            },
            Err(e) => ToolCallResult {
                name: call.name.clone(),
                output: ToolOutput::Error(format!("Tool execution failed: {}", e)),
            },
        }
    }

    /// Execute multiple tool calls sequentially.
    pub async fn execute_calls(
        &self,
        calls: &[ToolCall],
        ctx: &ToolContext,
    ) -> Vec<ToolCallResult> {
        let mut results = Vec::with_capacity(calls.len());
        for call in calls {
            results.push(self.execute_call(call, ctx).await);
        }
        results
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }

        fn description(&self) -> &str {
            "Echoes back the input message"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "The message to echo"
                    }
                },
                "required": ["message"]
            })
        }

        async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
            let message = params["message"]
                .as_str()
                .unwrap_or("(no message)");
            Ok(ToolOutput::Text(message.to_string()))
        }
    }

    struct DangerousTool;

    #[async_trait]
    impl Tool for DangerousTool {
        fn name(&self) -> &str {
            "dangerous"
        }

        fn description(&self) -> &str {
            "A tool that requires approval"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {},
            })
        }

        async fn execute(&self, _params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
            Ok(ToolOutput::Text("executed".to_string()))
        }

        fn requires_approval(&self) -> bool {
            true
        }

        fn category(&self) -> ToolCategory {
            ToolCategory::Shell
        }
    }

    fn test_ctx() -> ToolContext {
        ToolContext {
            working_directory: "/tmp".to_string(),
            username: "test".to_string(),
            autonomous: false,
        }
    }

    #[tokio::test]
    async fn test_registry_register_and_get() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool)).await;

        assert!(registry.get("echo").await.is_some());
        assert!(registry.get("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_tool_definitions_format() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool)).await;

        let defs = registry.tool_definitions().await;
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].tool_type, "function");
        assert_eq!(defs[0].function.name, "echo");

        // Should be valid JSON that can be serialized
        let json = serde_json::to_string(&defs).unwrap();
        assert!(json.contains("echo"));
    }

    #[tokio::test]
    async fn test_execute_echo_tool() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool)).await;

        let call = ToolCall {
            name: "echo".to_string(),
            arguments: serde_json::json!({"message": "hello"}),
        };

        let result = registry.execute_call(&call, &test_ctx()).await;
        assert_eq!(result.name, "echo");
        assert!(result.output.is_success());
        assert_eq!(result.output.to_llm_string(), "hello");
    }

    #[tokio::test]
    async fn test_unknown_tool_returns_error() {
        let registry = ToolRegistry::new();

        let call = ToolCall {
            name: "nonexistent".to_string(),
            arguments: serde_json::json!({}),
        };

        let result = registry.execute_call(&call, &test_ctx()).await;
        assert!(!result.output.is_success());
        assert!(result.output.to_llm_string().contains("Unknown tool"));
    }

    #[tokio::test]
    async fn test_approval_required_in_autonomous_mode() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(DangerousTool)).await;

        let call = ToolCall {
            name: "dangerous".to_string(),
            arguments: serde_json::json!({}),
        };

        // In autonomous mode, should need approval
        let mut ctx = test_ctx();
        ctx.autonomous = true;
        let result = registry.execute_call(&call, &ctx).await;
        assert!(matches!(result.output, ToolOutput::NeedsApproval { .. }));

        // In interactive mode, should execute normally
        let mut ctx = test_ctx();
        ctx.autonomous = false;
        let result = registry.execute_call(&call, &ctx).await;
        assert!(result.output.is_success());
    }

    #[tokio::test]
    async fn test_deregister() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool)).await;
        assert!(registry.get("echo").await.is_some());

        registry.deregister("echo").await;
        assert!(registry.get("echo").await.is_none());
    }
}
