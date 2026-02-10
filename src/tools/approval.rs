//! Approval gate system for dangerous tool operations.
//!
//! Provides configurable policies that determine which tool calls need
//! user approval before execution, and which are auto-approved.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tokio::sync::RwLock;

/// Policy for a specific tool or category
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalPolicy {
    /// Always allow without asking
    AlwaysAllow,
    /// Always require approval
    AlwaysAsk,
    /// Allow in interactive mode, ask in autonomous mode
    AskWhenAutonomous,
}

impl Default for ApprovalPolicy {
    fn default() -> Self {
        ApprovalPolicy::AskWhenAutonomous
    }
}

/// Decision from the approval system
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalDecision {
    /// Approved — proceed with execution
    Approved,
    /// Denied — do not execute
    Denied(String),
    /// Needs user input — pause and ask
    NeedsApproval {
        tool_name: String,
        description: String,
    },
}

/// Manages approval policies and pending approvals.
pub struct ApprovalGate {
    /// Per-tool policies (tool name → policy)
    tool_policies: RwLock<std::collections::HashMap<String, ApprovalPolicy>>,
    /// Tools that have been session-approved (user said "allow for this session")
    session_approved: RwLock<HashSet<String>>,
    /// Default policy for tools not in the map
    default_policy: ApprovalPolicy,
}

impl ApprovalGate {
    pub fn new() -> Self {
        Self {
            tool_policies: RwLock::new(std::collections::HashMap::new()),
            session_approved: RwLock::new(HashSet::new()),
            default_policy: ApprovalPolicy::AskWhenAutonomous,
        }
    }

    /// Create with specific default policy
    pub fn with_default_policy(policy: ApprovalPolicy) -> Self {
        Self {
            tool_policies: RwLock::new(std::collections::HashMap::new()),
            session_approved: RwLock::new(HashSet::new()),
            default_policy: policy,
        }
    }

    /// Set policy for a specific tool
    pub async fn set_tool_policy(&self, tool_name: &str, policy: ApprovalPolicy) {
        self.tool_policies
            .write()
            .await
            .insert(tool_name.to_string(), policy);
    }

    /// Grant session-level approval for a tool (user approved once, allow for rest of session)
    pub async fn grant_session_approval(&self, tool_name: &str) {
        self.session_approved
            .write()
            .await
            .insert(tool_name.to_string());
        tracing::info!("Granted session approval for tool: {}", tool_name);
    }

    /// Revoke session-level approval
    pub async fn revoke_session_approval(&self, tool_name: &str) {
        self.session_approved.write().await.remove(tool_name);
    }

    /// Check whether a tool call should be approved.
    ///
    /// Takes into account:
    /// 1. Whether the tool itself says it requires approval
    /// 2. Per-tool policy overrides
    /// 3. Session-level approvals
    /// 4. Whether we're in autonomous mode
    pub async fn check(
        &self,
        tool_name: &str,
        tool_requires_approval: bool,
        autonomous: bool,
        description: &str,
    ) -> ApprovalDecision {
        // If the tool doesn't require approval at all, always allow
        if !tool_requires_approval {
            return ApprovalDecision::Approved;
        }

        // Check session-level approval
        if self.session_approved.read().await.contains(tool_name) {
            return ApprovalDecision::Approved;
        }

        // Check per-tool policy
        let policy = self
            .tool_policies
            .read()
            .await
            .get(tool_name)
            .cloned()
            .unwrap_or_else(|| self.default_policy.clone());

        match policy {
            ApprovalPolicy::AlwaysAllow => ApprovalDecision::Approved,
            ApprovalPolicy::AlwaysAsk => ApprovalDecision::NeedsApproval {
                tool_name: tool_name.to_string(),
                description: description.to_string(),
            },
            ApprovalPolicy::AskWhenAutonomous => {
                if autonomous {
                    ApprovalDecision::NeedsApproval {
                        tool_name: tool_name.to_string(),
                        description: description.to_string(),
                    }
                } else {
                    ApprovalDecision::Approved
                }
            }
        }
    }

    /// Clear all session approvals (e.g., on session reset)
    pub async fn clear_session_approvals(&self) {
        self.session_approved.write().await.clear();
    }
}

impl Default for ApprovalGate {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_non_dangerous_tool_always_approved() {
        let gate = ApprovalGate::new();
        let result = gate.check("echo", false, true, "echo a message").await;
        assert_eq!(result, ApprovalDecision::Approved);
    }

    #[tokio::test]
    async fn test_dangerous_tool_needs_approval_when_autonomous() {
        let gate = ApprovalGate::new();
        let result = gate.check("shell", true, true, "run ls").await;
        assert!(matches!(result, ApprovalDecision::NeedsApproval { .. }));
    }

    #[tokio::test]
    async fn test_dangerous_tool_approved_when_interactive() {
        let gate = ApprovalGate::new();
        let result = gate.check("shell", true, false, "run ls").await;
        assert_eq!(result, ApprovalDecision::Approved);
    }

    #[tokio::test]
    async fn test_always_allow_policy() {
        let gate = ApprovalGate::new();
        gate.set_tool_policy("shell", ApprovalPolicy::AlwaysAllow)
            .await;
        let result = gate.check("shell", true, true, "run ls").await;
        assert_eq!(result, ApprovalDecision::Approved);
    }

    #[tokio::test]
    async fn test_always_ask_policy() {
        let gate = ApprovalGate::new();
        gate.set_tool_policy("shell", ApprovalPolicy::AlwaysAsk)
            .await;
        let result = gate.check("shell", true, false, "run ls").await;
        assert!(matches!(result, ApprovalDecision::NeedsApproval { .. }));
    }

    #[tokio::test]
    async fn test_session_approval() {
        let gate = ApprovalGate::new();
        // Should need approval initially
        let result = gate.check("shell", true, true, "run ls").await;
        assert!(matches!(result, ApprovalDecision::NeedsApproval { .. }));

        // Grant session approval
        gate.grant_session_approval("shell").await;

        // Should now be approved
        let result = gate.check("shell", true, true, "run ls").await;
        assert_eq!(result, ApprovalDecision::Approved);

        // Revoke
        gate.revoke_session_approval("shell").await;
        let result = gate.check("shell", true, true, "run ls").await;
        assert!(matches!(result, ApprovalDecision::NeedsApproval { .. }));
    }

    #[tokio::test]
    async fn test_clear_session_approvals() {
        let gate = ApprovalGate::new();
        gate.grant_session_approval("shell").await;
        gate.grant_session_approval("write_file").await;

        gate.clear_session_approvals().await;

        let result = gate.check("shell", true, true, "run ls").await;
        assert!(matches!(result, ApprovalDecision::NeedsApproval { .. }));
    }
}
