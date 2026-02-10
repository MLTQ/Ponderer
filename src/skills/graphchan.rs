use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::{Skill, SkillActionDef, SkillContext, SkillEvent, SkillResult};

// ========================================================================
// Graphchan API Types
// ========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadSummary {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub creator_peer_id: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadDetails {
    pub thread: ThreadSummary,
    #[serde(default)]
    pub posts: Vec<PostView>,
    #[serde(default)]
    pub peers: Vec<PeerView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostView {
    pub id: String,
    pub thread_id: String,
    #[serde(default)]
    pub author_peer_id: Option<String>,
    pub body: String,
    pub created_at: String,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub parent_post_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<PostMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerView {
    pub id: String,
    pub alias: Option<String>,
    pub username: Option<String>,
    pub bio: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreatePostInput {
    pub thread_id: String,
    #[serde(default)]
    pub author_peer_id: Option<String>,
    pub body: String,
    #[serde(default)]
    pub parent_post_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<PostMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentPostView {
    pub post: PostView,
    pub thread_title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentPostsResponse {
    pub posts: Vec<RecentPostView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostResponse {
    pub post: PostView,
}

// ========================================================================
// Graphchan Skill
// ========================================================================

pub struct GraphchanSkill {
    base_url: String,
    client: Client,
}

impl GraphchanSkill {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: Client::new(),
        }
    }

    pub async fn list_threads(&self) -> Result<Vec<ThreadSummary>> {
        let url = format!("{}/threads", self.base_url);
        let response = self.client.get(&url).send().await?;
        let threads = response.json().await?;
        Ok(threads)
    }

    pub async fn get_thread(&self, thread_id: &str) -> Result<ThreadDetails> {
        let url = format!("{}/threads/{}", self.base_url, thread_id);
        let response = self.client.get(&url).send().await?;
        let thread = response.json().await?;
        Ok(thread)
    }

    pub async fn get_recent_posts(&self, limit: usize) -> Result<RecentPostsResponse> {
        let url = format!("{}/posts/recent?limit={}", self.base_url, limit);
        let response = self.client.get(&url).send().await?;
        let posts = response.json().await?;
        Ok(posts)
    }

    pub async fn create_post(&self, input: CreatePostInput) -> Result<PostView> {
        let url = format!("{}/threads/{}/posts", self.base_url, input.thread_id);
        let response = self.client.post(&url).json(&input).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to create post: {} - {}", status, body);
        }

        let response_wrapper: PostResponse = response.json().await?;
        Ok(response_wrapper.post)
    }

    pub async fn health_check(&self) -> Result<()> {
        let url = format!("{}/threads", self.base_url);
        self.client
            .get(&url)
            .send()
            .await
            .context("Failed to connect to Graphchan API")?;
        Ok(())
    }

    pub async fn get_self_peer(&self) -> Result<PeerView> {
        let url = format!("{}/peers/self", self.base_url);
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to get self peer: {} - {}", status, body);
        }

        let peer = response.json().await?;
        Ok(peer)
    }
}

#[async_trait]
impl Skill for GraphchanSkill {
    fn name(&self) -> &str {
        "graphchan"
    }

    fn description(&self) -> &str {
        "Interact with a Graphchan forum: read posts, reply to threads"
    }

    async fn poll(&self, ctx: &SkillContext) -> Result<Vec<SkillEvent>> {
        let recent = self.get_recent_posts(20).await?;

        let events: Vec<SkillEvent> = recent
            .posts
            .into_iter()
            .filter(|post_view| {
                // Filter out agent's own posts
                let is_agent_post = post_view
                    .post
                    .metadata
                    .as_ref()
                    .and_then(|m| m.agent.as_ref())
                    .map(|a| a.name == ctx.username)
                    .unwrap_or(false);
                !is_agent_post
            })
            .map(|post_view| SkillEvent::NewContent {
                id: post_view.post.id.clone(),
                source: post_view.thread_title,
                author: post_view
                    .post
                    .author_peer_id
                    .unwrap_or_else(|| "Anonymous".to_string()),
                body: post_view.post.body,
                parent_ids: post_view.post.parent_post_ids,
            })
            .collect();

        Ok(events)
    }

    async fn execute(&self, action: &str, params: &serde_json::Value) -> Result<SkillResult> {
        match action {
            "reply" => {
                let thread_id = params["thread_id"].as_str().context("Missing thread_id")?;
                let post_id = params["post_id"].as_str().context("Missing post_id")?;
                let content = params["content"].as_str().context("Missing content")?;
                let username = params["username"].as_str().unwrap_or("Ponderer");

                let metadata = Some(PostMetadata {
                    agent: Some(AgentInfo {
                        name: username.to_string(),
                        version: None,
                    }),
                    client: Some("ponderer".to_string()),
                });

                let input = CreatePostInput {
                    thread_id: thread_id.to_string(),
                    author_peer_id: None,
                    body: content.to_string(),
                    parent_post_ids: vec![post_id.to_string()],
                    metadata,
                };

                let posted = self.create_post(input).await?;
                Ok(SkillResult::Success {
                    message: format!("Posted reply {}", posted.id),
                })
            }
            "list_threads" => {
                let threads = self.list_threads().await?;
                let summary: Vec<String> = threads
                    .iter()
                    .take(10)
                    .map(|t| format!("{}: {}", t.id, t.title))
                    .collect();
                Ok(SkillResult::Success {
                    message: summary.join("\n"),
                })
            }
            _ => Ok(SkillResult::Error {
                message: format!("Unknown action: {}", action),
            }),
        }
    }

    fn available_actions(&self) -> Vec<SkillActionDef> {
        vec![
            SkillActionDef {
                name: "reply".to_string(),
                description: "Reply to a forum post".to_string(),
                params_description:
                    "{\"thread_id\": \"...\", \"post_id\": \"...\", \"content\": \"...\"}"
                        .to_string(),
            },
            SkillActionDef {
                name: "list_threads".to_string(),
                description: "List recent forum threads".to_string(),
                params_description: "{}".to_string(),
            },
        ]
    }
}
