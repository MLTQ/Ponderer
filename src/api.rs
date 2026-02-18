use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use flume::Sender;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header as ws_header;
use tokio_tungstenite::tungstenite::http::HeaderValue as WsHeaderValue;
use tokio_tungstenite::tungstenite::Message;

pub const DEFAULT_CHAT_CONVERSATION_ID: &str = "default";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatTurnPhase {
    #[serde(alias = "Idle")]
    Idle,
    #[serde(alias = "Processing")]
    Processing,
    #[serde(alias = "Completed")]
    Completed,
    #[serde(alias = "AwaitingApproval")]
    AwaitingApproval,
    #[serde(alias = "Failed")]
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatConversation {
    pub id: String,
    pub session_id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub runtime_state: ChatTurnPhase,
    pub active_turn_id: Option<String>,
    pub message_count: usize,
    pub last_message_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub processed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentVisualState {
    #[serde(alias = "Idle")]
    Idle,
    #[serde(alias = "Reading")]
    Reading,
    #[serde(alias = "Thinking")]
    Thinking,
    #[serde(alias = "Writing")]
    Writing,
    #[serde(alias = "Happy")]
    Happy,
    #[serde(alias = "Confused")]
    Confused,
    #[serde(alias = "Paused")]
    Paused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRuntimeStatus {
    pub paused: bool,
    pub visual_state: AgentVisualState,
    pub actions_this_hour: u32,
    pub last_action_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct OrientationSummary {
    pub disposition: String,
    pub anomaly_count: usize,
    pub salience_count: usize,
}

#[derive(Debug, Clone)]
pub enum FrontendEvent {
    StateChanged(AgentVisualState),
    Observation(String),
    ReasoningTrace(Vec<String>),
    ToolCallProgress {
        conversation_id: String,
        tool_name: String,
        output_preview: String,
    },
    ChatStreaming {
        conversation_id: String,
        content: String,
        done: bool,
    },
    ActionTaken {
        action: String,
        result: String,
    },
    OrientationUpdate(OrientationSummary),
    JournalWritten(String),
    ConcernCreated {
        id: String,
        summary: String,
    },
    ConcernTouched {
        id: String,
        summary: String,
    },
    Error(String),
}

#[derive(Debug, Deserialize)]
struct ApiEventEnvelope {
    event_type: String,
    payload: Value,
}

#[derive(Debug, Deserialize)]
struct SendMessageResponse {
    message_id: String,
}

#[derive(Debug, Deserialize)]
struct PauseStateResponse {
    paused: bool,
}

#[derive(Debug, Deserialize)]
struct StopResponse {
    stopped: bool,
}

#[derive(Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    base_url: String,
    ws_url: String,
    token: Option<String>,
}

impl ApiClient {
    pub fn from_env() -> Self {
        let base = std::env::var("PONDERER_BACKEND_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8787".to_string());
        let token = std::env::var("PONDERER_BACKEND_TOKEN")
            .ok()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());

        Self::new(base, token)
    }

    pub fn new(base_url: String, token: Option<String>) -> Self {
        let normalized_base = normalize_base_url(&base_url);
        let ws_url = normalize_ws_url(&normalized_base);

        Self {
            http: reqwest::Client::new(),
            base_url: normalized_base,
            ws_url,
            token,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn get_config(&self) -> Result<crate::config::AgentConfig> {
        self.request(reqwest::Method::GET, "/v1/config")
            .send()
            .await?
            .error_for_status()
            .context("GET /v1/config failed")?
            .json::<crate::config::AgentConfig>()
            .await
            .context("Failed to decode config response")
    }

    pub async fn update_config(
        &self,
        config: &crate::config::AgentConfig,
    ) -> Result<crate::config::AgentConfig> {
        self.request(reqwest::Method::PUT, "/v1/config")
            .json(config)
            .send()
            .await?
            .error_for_status()
            .context("PUT /v1/config failed")?
            .json::<crate::config::AgentConfig>()
            .await
            .context("Failed to decode updated config")
    }

    pub async fn list_conversations(&self, limit: usize) -> Result<Vec<ChatConversation>> {
        let response = self
            .request(reqwest::Method::GET, "/v1/conversations")
            .query(&[("limit", limit)])
            .send()
            .await?
            .error_for_status()
            .context("GET /v1/conversations failed")?;

        let body = response
            .text()
            .await
            .context("Failed to read conversation list payload")?;
        serde_json::from_str::<Vec<ChatConversation>>(&body).context(format!(
            "Failed to decode conversation list. Payload preview: {}",
            body.chars().take(500).collect::<String>()
        ))
    }

    pub async fn create_conversation(&self, title: Option<&str>) -> Result<ChatConversation> {
        #[derive(Serialize)]
        struct CreateConversationRequest<'a> {
            title: Option<&'a str>,
        }

        self.request(reqwest::Method::POST, "/v1/conversations")
            .json(&CreateConversationRequest { title })
            .send()
            .await?
            .error_for_status()
            .context("POST /v1/conversations failed")?
            .json::<ChatConversation>()
            .await
            .context("Failed to decode created conversation")
    }

    pub async fn list_messages(
        &self,
        conversation_id: &str,
        limit: usize,
    ) -> Result<Vec<ChatMessage>> {
        self.request(
            reqwest::Method::GET,
            &format!("/v1/conversations/{}/messages", conversation_id),
        )
        .query(&[("limit", limit)])
        .send()
        .await?
        .error_for_status()
        .with_context(|| format!("GET /v1/conversations/{}/messages failed", conversation_id))?
        .json::<Vec<ChatMessage>>()
        .await
        .context("Failed to decode chat history")
    }

    pub async fn send_message(&self, conversation_id: &str, content: &str) -> Result<String> {
        #[derive(Serialize)]
        struct SendMessageRequest<'a> {
            content: &'a str,
        }

        let response = self
            .request(
                reqwest::Method::POST,
                &format!("/v1/conversations/{}/messages", conversation_id),
            )
            .json(&SendMessageRequest { content })
            .send()
            .await?
            .error_for_status()
            .with_context(|| format!("POST /v1/conversations/{}/messages failed", conversation_id))?
            .json::<SendMessageResponse>()
            .await
            .context("Failed to decode send message response")?;

        Ok(response.message_id)
    }

    pub async fn get_agent_status(&self) -> Result<AgentRuntimeStatus> {
        self.request(reqwest::Method::GET, "/v1/agent/status")
            .send()
            .await?
            .error_for_status()
            .context("GET /v1/agent/status failed")?
            .json::<AgentRuntimeStatus>()
            .await
            .context("Failed to decode agent status")
    }

    pub async fn toggle_pause(&self) -> Result<bool> {
        let response = self
            .request(reqwest::Method::POST, "/v1/agent/toggle-pause")
            .send()
            .await?
            .error_for_status()
            .context("POST /v1/agent/toggle-pause failed")?
            .json::<PauseStateResponse>()
            .await
            .context("Failed to decode toggle pause response")?;
        Ok(response.paused)
    }

    pub async fn stop_agent_turn(&self) -> Result<bool> {
        let response = self
            .request(reqwest::Method::POST, "/v1/agent/stop")
            .send()
            .await?
            .error_for_status()
            .context("POST /v1/agent/stop failed")?
            .json::<StopResponse>()
            .await
            .context("Failed to decode stop response")?;
        Ok(response.stopped)
    }

    pub async fn stream_events_forever(self, tx: Sender<FrontendEvent>) {
        loop {
            match self.stream_events_once(&tx).await {
                Ok(()) => {
                    tracing::info!("Event stream disconnected; reconnecting in 1s");
                }
                Err(error) => {
                    tracing::warn!("Event stream failed: {}; reconnecting in 2s", error);
                }
            }
            sleep(Duration::from_secs(2)).await;
        }
    }

    async fn stream_events_once(&self, tx: &Sender<FrontendEvent>) -> Result<()> {
        let ws_endpoint = format!("{}/v1/ws/events", self.ws_url);
        let mut request = ws_endpoint
            .into_client_request()
            .context("Invalid websocket endpoint URL")?;

        if let Some(token) = self.token.as_deref() {
            let value = WsHeaderValue::from_str(&format!("Bearer {}", token))
                .context("Invalid bearer token for websocket auth")?;
            request
                .headers_mut()
                .insert(ws_header::AUTHORIZATION, value);
        }

        let (stream, _) = connect_async(request)
            .await
            .context("Failed to connect websocket event stream")?;
        let (_write, mut read) = stream.split();

        while let Some(message) = read.next().await {
            match message.context("Websocket read error")? {
                Message::Text(text) => {
                    if let Some(event) = parse_event_envelope(&text)? {
                        let _ = tx.send(event);
                    }
                }
                Message::Binary(bytes) => {
                    if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                        if let Some(event) = parse_event_envelope(&text)? {
                            let _ = tx.send(event);
                        }
                    }
                }
                Message::Close(_) => {
                    return Ok(());
                }
                Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {}
            }
        }

        Ok(())
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut builder = self.http.request(method, url);
        if let Some(token) = self.token.as_deref() {
            builder = builder.bearer_auth(token);
        }
        builder
    }
}

fn parse_event_envelope(text: &str) -> Result<Option<FrontendEvent>> {
    let envelope: ApiEventEnvelope =
        serde_json::from_str(text).context("Failed to decode API event envelope")?;
    Ok(map_event(envelope))
}

fn map_event(envelope: ApiEventEnvelope) -> Option<FrontendEvent> {
    match envelope.event_type.as_str() {
        "state_changed" => {
            let state_raw = envelope.payload.get("state")?.as_str()?;
            parse_visual_state(state_raw).map(FrontendEvent::StateChanged)
        }
        "observation" => Some(FrontendEvent::Observation(
            envelope
                .payload
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        )),
        "reasoning_trace" => Some(FrontendEvent::ReasoningTrace(
            envelope
                .payload
                .get("steps")
                .and_then(Value::as_array)
                .map(|steps| {
                    steps
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        )),
        "tool_call_progress" => Some(FrontendEvent::ToolCallProgress {
            conversation_id: envelope
                .payload
                .get("conversation_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            tool_name: envelope
                .payload
                .get("tool_name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            output_preview: envelope
                .payload
                .get("output_preview")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        }),
        "chat_streaming" => Some(FrontendEvent::ChatStreaming {
            conversation_id: envelope
                .payload
                .get("conversation_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            content: envelope
                .payload
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            done: envelope
                .payload
                .get("done")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        "action_taken" => Some(FrontendEvent::ActionTaken {
            action: envelope
                .payload
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            result: envelope
                .payload
                .get("result")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        }),
        "orientation_update" => {
            let disposition = envelope
                .payload
                .get("disposition")
                .map(json_value_to_short_string)
                .unwrap_or_else(|| "unknown".to_string());
            let anomaly_count = envelope
                .payload
                .get("anomalies")
                .and_then(Value::as_array)
                .map(|a| a.len())
                .unwrap_or(0);
            let salience_count = envelope
                .payload
                .get("salience_map")
                .and_then(Value::as_object)
                .map(|m| m.len())
                .unwrap_or(0);

            Some(FrontendEvent::OrientationUpdate(OrientationSummary {
                disposition,
                anomaly_count,
                salience_count,
            }))
        }
        "journal_written" => Some(FrontendEvent::JournalWritten(
            envelope
                .payload
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        )),
        "concern_created" => Some(FrontendEvent::ConcernCreated {
            id: envelope
                .payload
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            summary: envelope
                .payload
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        }),
        "concern_touched" => Some(FrontendEvent::ConcernTouched {
            id: envelope
                .payload
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            summary: envelope
                .payload
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        }),
        "error" => Some(FrontendEvent::Error(
            envelope
                .payload
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Unknown backend error")
                .to_string(),
        )),
        _ => None,
    }
}

fn parse_visual_state(raw: &str) -> Option<AgentVisualState> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "idle" => Some(AgentVisualState::Idle),
        "reading" => Some(AgentVisualState::Reading),
        "thinking" => Some(AgentVisualState::Thinking),
        "writing" => Some(AgentVisualState::Writing),
        "happy" => Some(AgentVisualState::Happy),
        "confused" => Some(AgentVisualState::Confused),
        "paused" => Some(AgentVisualState::Paused),
        _ => None,
    }
}

fn json_value_to_short_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

fn normalize_base_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        "http://127.0.0.1:8787".to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_ws_url(base_http_url: &str) -> String {
    if let Some(rest) = base_http_url.strip_prefix("https://") {
        format!("wss://{}", rest)
    } else if let Some(rest) = base_http_url.strip_prefix("http://") {
        format!("ws://{}", rest)
    } else {
        format!("ws://{}", base_http_url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_visual_state_case_insensitive() {
        assert!(matches!(
            parse_visual_state("Thinking"),
            Some(AgentVisualState::Thinking)
        ));
        assert!(matches!(
            parse_visual_state("paused"),
            Some(AgentVisualState::Paused)
        ));
        assert!(parse_visual_state("unknown").is_none());
    }

    #[test]
    fn normalizes_base_url() {
        assert_eq!(normalize_base_url("http://x:1/"), "http://x:1");
        assert_eq!(normalize_base_url(""), "http://127.0.0.1:8787");
    }

    #[test]
    fn maps_http_to_ws_url() {
        assert_eq!(
            normalize_ws_url("http://127.0.0.1:8787"),
            "ws://127.0.0.1:8787"
        );
        assert_eq!(normalize_ws_url("https://example.com"), "wss://example.com");
    }

    #[test]
    fn parses_state_change_event() {
        let envelope = ApiEventEnvelope {
            event_type: "state_changed".to_string(),
            payload: serde_json::json!({"state": "Idle"}),
        };

        let mapped = map_event(envelope);
        assert!(matches!(mapped, Some(FrontendEvent::StateChanged(_))));
    }

    #[test]
    fn parses_orientation_summary_event() {
        let envelope = ApiEventEnvelope {
            event_type: "orientation_update".to_string(),
            payload: serde_json::json!({
                "disposition": "observe",
                "anomalies": [1, 2],
                "salience_map": {"a": 1, "b": 2, "c": 3}
            }),
        };

        let mapped = map_event(envelope).expect("mapped");
        match mapped {
            FrontendEvent::OrientationUpdate(summary) => {
                assert_eq!(summary.disposition, "observe");
                assert_eq!(summary.anomaly_count, 2);
                assert_eq!(summary.salience_count, 3);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn api_client_from_env_picks_defaults() {
        let client = ApiClient::new("http://127.0.0.1:8787/".to_string(), None);
        assert_eq!(client.base_url(), "http://127.0.0.1:8787");
    }

    #[test]
    fn chat_conversation_deserializes_snake_case_runtime_state() {
        let payload = serde_json::json!([{
            "id": "c1",
            "session_id": "s1",
            "title": "Chat",
            "created_at": "2026-02-18T06:17:38.096788Z",
            "updated_at": "2026-02-18T06:17:38.096788Z",
            "runtime_state": "awaiting_approval",
            "active_turn_id": null,
            "message_count": 0,
            "last_message_at": null
        }]);

        let parsed: Vec<ChatConversation> =
            serde_json::from_value(payload).expect("decode conversation list");
        assert_eq!(parsed.len(), 1);
        assert!(matches!(
            parsed[0].runtime_state,
            ChatTurnPhase::AwaitingApproval
        ));
    }

    #[test]
    fn runtime_status_deserializes_snake_case_visual_state() {
        let payload = serde_json::json!({
            "paused": false,
            "visual_state": "thinking",
            "actions_this_hour": 2,
            "last_action_time": "2026-02-18T06:17:38.096788Z"
        });

        let parsed: AgentRuntimeStatus =
            serde_json::from_value(payload).expect("decode status");
        assert!(matches!(parsed.visual_state, AgentVisualState::Thinking));
    }
}
