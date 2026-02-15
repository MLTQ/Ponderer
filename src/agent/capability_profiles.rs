use crate::config::{AgentConfig, CapabilityProfileConfig, CapabilityProfileOverride};
use crate::tools::ToolContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentCapabilityProfile {
    PrivateChat,
    SkillEvents,
    Heartbeat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCapabilityPolicy {
    pub autonomous: bool,
    pub allowed_tools: Option<Vec<String>>,
    pub disallowed_tools: Vec<String>,
}

impl ToolCapabilityPolicy {
    fn to_tool_context(self, working_directory: String, username: String) -> ToolContext {
        ToolContext {
            working_directory,
            username,
            autonomous: self.autonomous,
            allowed_tools: self.allowed_tools,
            disallowed_tools: self.disallowed_tools,
        }
    }
}

pub fn build_tool_context_for_profile(
    config: &AgentConfig,
    profile: AgentCapabilityProfile,
    working_directory: String,
    username: String,
) -> ToolContext {
    resolve_capability_policy(profile, &config.capability_profiles)
        .to_tool_context(working_directory, username)
}

pub fn resolve_capability_policy(
    profile: AgentCapabilityProfile,
    config: &CapabilityProfileConfig,
) -> ToolCapabilityPolicy {
    let default_policy = default_policy(profile);
    let override_cfg = match profile {
        AgentCapabilityProfile::PrivateChat => &config.private_chat,
        AgentCapabilityProfile::SkillEvents => &config.skill_events,
        AgentCapabilityProfile::Heartbeat => &config.heartbeat,
    };
    apply_override(default_policy, override_cfg)
}

fn default_policy(profile: AgentCapabilityProfile) -> ToolCapabilityPolicy {
    match profile {
        AgentCapabilityProfile::PrivateChat => ToolCapabilityPolicy {
            autonomous: false,
            allowed_tools: None,
            disallowed_tools: vec![
                "graphchan_skill".to_string(),
                "post_to_graphchan".to_string(),
            ],
        },
        AgentCapabilityProfile::SkillEvents => ToolCapabilityPolicy {
            autonomous: true,
            allowed_tools: None,
            disallowed_tools: Vec::new(),
        },
        AgentCapabilityProfile::Heartbeat => ToolCapabilityPolicy {
            autonomous: true,
            allowed_tools: None,
            disallowed_tools: vec![
                "graphchan_skill".to_string(),
                "post_to_graphchan".to_string(),
            ],
        },
    }
}

fn apply_override(
    mut policy: ToolCapabilityPolicy,
    override_cfg: &CapabilityProfileOverride,
) -> ToolCapabilityPolicy {
    if let Some(allowed) = &override_cfg.allowed_tools {
        policy.allowed_tools = Some(normalize_tool_names(allowed));
    }

    if let Some(disallowed) = &override_cfg.disallowed_tools {
        policy.disallowed_tools = normalize_tool_names(disallowed);
    }

    policy
}

fn normalize_tool_names(items: &[String]) -> Vec<String> {
    let mut output: Vec<String> = Vec::new();
    for item in items {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !output
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(trimmed))
        {
            output.push(trimmed.to_string());
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AgentConfig;

    #[test]
    fn private_chat_blocks_graphchan_tools_by_default() {
        let cfg = AgentConfig::default();
        let policy = resolve_capability_policy(
            AgentCapabilityProfile::PrivateChat,
            &cfg.capability_profiles,
        );
        assert!(!policy.autonomous);
        assert!(policy
            .disallowed_tools
            .iter()
            .any(|tool| tool.eq_ignore_ascii_case("graphchan_skill")));
        assert!(policy
            .disallowed_tools
            .iter()
            .any(|tool| tool.eq_ignore_ascii_case("post_to_graphchan")));
    }

    #[test]
    fn skill_events_allow_graphchan_tools_by_default() {
        let cfg = AgentConfig::default();
        let policy = resolve_capability_policy(
            AgentCapabilityProfile::SkillEvents,
            &cfg.capability_profiles,
        );
        assert!(policy.autonomous);
        assert!(!policy
            .disallowed_tools
            .iter()
            .any(|tool| tool.eq_ignore_ascii_case("graphchan_skill")));
        assert!(policy.allowed_tools.is_none());
    }

    #[test]
    fn heartbeat_blocks_graphchan_tools_by_default() {
        let cfg = AgentConfig::default();
        let policy =
            resolve_capability_policy(AgentCapabilityProfile::Heartbeat, &cfg.capability_profiles);
        assert!(policy.autonomous);
        assert!(policy
            .disallowed_tools
            .iter()
            .any(|tool| tool.eq_ignore_ascii_case("graphchan_skill")));
    }

    #[test]
    fn overrides_replace_default_policy_lists() {
        let mut cfg = AgentConfig::default();
        cfg.capability_profiles.private_chat.allowed_tools = Some(vec![
            "shell".to_string(),
            "shell".to_string(),
            "".to_string(),
        ]);
        cfg.capability_profiles.private_chat.disallowed_tools =
            Some(vec!["read_file".to_string(), "READ_FILE".to_string()]);

        let policy = resolve_capability_policy(
            AgentCapabilityProfile::PrivateChat,
            &cfg.capability_profiles,
        );

        assert_eq!(policy.allowed_tools, Some(vec!["shell".to_string()]));
        assert_eq!(policy.disallowed_tools, vec!["read_file".to_string()]);
    }

    #[test]
    fn tool_context_uses_resolved_policy() {
        let mut cfg = AgentConfig::default();
        cfg.capability_profiles.skill_events.allowed_tools =
            Some(vec!["graphchan_skill".to_string()]);

        let ctx = build_tool_context_for_profile(
            &cfg,
            AgentCapabilityProfile::SkillEvents,
            "/tmp".to_string(),
            "ponderer".to_string(),
        );

        assert!(ctx.autonomous);
        assert_eq!(ctx.allowed_tools, Some(vec!["graphchan_skill".to_string()]));
        assert_eq!(ctx.disallowed_tools, Vec::<String>::new());
    }
}
