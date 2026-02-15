use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use crate::agent::concerns::{Concern, ConcernContext, ConcernType, Salience};
use crate::agent::journal::{JournalContext, JournalEntry, JournalEntryType, JournalMood};
use crate::memory::archive::{
    evaluate_promotion_policy, MemoryDesignArchiveEntry, MemoryEvalRunRecord,
    MemoryPromotionDecisionRecord, MemoryPromotionPolicy, PromotionMetricsSnapshot,
    PromotionOutcome,
};
use crate::memory::{
    KvMemoryBackend, MemoryBackend, MemoryDesignVersion, MemoryMigrationRegistry,
    WorkingMemoryEntry, MEMORY_DESIGN_STATE_KEY, MEMORY_SCHEMA_VERSION_STATE_KEY,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportantPost {
    pub id: String,
    pub post_id: String,
    pub thread_id: String,
    pub post_body: String,
    pub why_important: String,
    pub importance_score: f64, // 0.0-1.0, how formative this experience was
    pub marked_at: DateTime<Utc>,
}

/// A snapshot of the agent's persona at a point in time.
/// This is the core data structure for "Ludonarrative Assonantic Tracing" -
/// tracking how the agent's personality evolves and inferring its trajectory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaSnapshot {
    pub id: String,
    pub captured_at: DateTime<Utc>,

    /// Dynamic personality dimensions - keyed by principle/dimension name, value 0.0-1.0
    /// These are derived from the agent's guiding_principles config, but the LLM
    /// can also define additional dimensions during reflection.
    /// This allows researchers to study arbitrary personality axes.
    pub traits: PersonaTraits,

    // The system prompt at this point
    pub system_prompt: String,

    // What triggered this snapshot (reflection, significant_interaction, manual, etc.)
    pub trigger: String,

    // LLM-generated self-description at this moment
    pub self_description: String,

    // Inferred trajectory - where is this persona heading?
    // This is the key to Ludonarrative Assonantic Tracing
    pub inferred_trajectory: Option<String>,

    // Notable experiences that shaped this snapshot
    pub formative_experiences: Vec<String>,
}

/// Dynamic personality traits - a flexible map of dimension names to scores.
///
/// Unlike fixed personality models (Big Five, MBTI, etc.), this allows:
/// 1. Agents to define their own axes via guiding_principles config
/// 2. LLMs to introduce new dimensions during self-reflection
/// 3. Researchers to study arbitrary personality dimensions
///
/// Each value is a 0.0-1.0 score representing where the agent falls on that dimension.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersonaTraits {
    /// Map of dimension name -> score (0.0 to 1.0)
    /// e.g., {"helpful": 0.8, "curious": 0.9, "nietzschean": 0.6}
    pub dimensions: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionRecord {
    pub id: String,
    pub reflected_at: DateTime<Utc>,
    pub old_prompt: String,
    pub new_prompt: String,
    pub reasoning: String,
    pub guiding_principles: String, // JSON array
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterCard {
    pub id: String,
    pub imported_at: DateTime<Utc>,
    pub format: String,         // "tavernai_v2", "wpp", "boostyle"
    pub original_data: String,  // Raw JSON/text of original card
    pub derived_prompt: String, // System prompt derived from card
    pub name: Option<String>,
    pub description: Option<String>,
}

/// A private chat message between the operator and the agent.
/// These are separate from forum posts - truly private dialogue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub conversation_id: String,
    pub role: String, // "operator" or "agent"
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub processed: bool, // Has the agent seen/responded to this?
}

pub const DEFAULT_CHAT_SESSION_ID: &str = "default_session";
pub const DEFAULT_CHAT_CONVERSATION_ID: &str = "default";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatTurnPhase {
    Idle,
    Processing,
    Completed,
    AwaitingApproval,
    Failed,
}

impl ChatTurnPhase {
    fn as_db_str(self) -> &'static str {
        match self {
            ChatTurnPhase::Idle => "idle",
            ChatTurnPhase::Processing => "processing",
            ChatTurnPhase::Completed => "completed",
            ChatTurnPhase::AwaitingApproval => "awaiting_approval",
            ChatTurnPhase::Failed => "failed",
        }
    }

    fn from_db(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "processing" => ChatTurnPhase::Processing,
            "completed" => ChatTurnPhase::Completed,
            "awaiting_approval" => ChatTurnPhase::AwaitingApproval,
            "failed" => ChatTurnPhase::Failed,
            _ => ChatTurnPhase::Idle,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: String,
    pub label: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
pub struct ChatConversationSummary {
    pub conversation_id: String,
    pub summary_text: String,
    pub summarized_message_count: usize,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTurn {
    pub id: String,
    pub session_id: String,
    pub conversation_id: String,
    pub iteration: i64,
    pub phase_state: ChatTurnPhase,
    pub decision: Option<String>,
    pub status: Option<String>,
    pub trigger_message_ids: Vec<String>,
    pub operator_message: Option<String>,
    pub reason: Option<String>,
    pub error: Option<String>,
    pub tool_call_count: usize,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub agent_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTurnToolCall {
    pub id: String,
    pub turn_id: String,
    pub call_index: usize,
    pub tool_name: String,
    pub arguments_json: String,
    pub output_text: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrientationSnapshotRecord {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub user_state: serde_json::Value,
    pub disposition: String,
    pub synthesis: String,
    pub salience_map: serde_json::Value,
    pub anomalies: serde_json::Value,
    pub pending_thoughts: serde_json::Value,
    pub mood_valence: Option<f32>,
    pub mood_arousal: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingThoughtRecord {
    pub id: String,
    pub content: String,
    pub context: Option<String>,
    pub priority: f32,
    pub relates_to: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub surfaced_at: Option<DateTime<Utc>>,
    pub dismissed_at: Option<DateTime<Utc>>,
}

pub struct AgentDatabase {
    conn: Mutex<Connection>,
    memory_backend: Box<dyn MemoryBackend>,
    migration_registry: MemoryMigrationRegistry,
}

impl AgentDatabase {
    /// Helper to lock the connection
    fn lock_conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))
    }

    /// Create or open the database
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self {
            conn: Mutex::new(conn),
            memory_backend: Box::new(KvMemoryBackend::new()),
            migration_registry: MemoryMigrationRegistry::default(),
        };
        db.ensure_schema()?;
        db.ensure_memory_design_state()?;
        Ok(db)
    }

    fn ensure_memory_design_state(&self) -> Result<()> {
        if self.get_memory_design_version()?.is_none() {
            self.set_memory_design_version(&self.memory_backend.design_version())?;
        }
        Ok(())
    }

    fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(columns.iter().any(|name| name == column))
    }

    fn ensure_chat_messages_conversation_column(&self, conn: &Connection) -> Result<()> {
        if !Self::table_has_column(conn, "chat_messages", "conversation_id")? {
            conn.execute(
                "ALTER TABLE chat_messages ADD COLUMN conversation_id TEXT NOT NULL DEFAULT 'default'",
                [],
            )?;
        }

        if !Self::table_has_column(conn, "chat_messages", "turn_id")? {
            conn.execute("ALTER TABLE chat_messages ADD COLUMN turn_id TEXT", [])?;
        }

        conn.execute(
            "UPDATE chat_messages SET conversation_id = ?1 WHERE conversation_id IS NULL OR TRIM(conversation_id) = ''",
            [DEFAULT_CHAT_CONVERSATION_ID],
        )?;

        Ok(())
    }

    fn ensure_chat_conversations_runtime_columns(&self, conn: &Connection) -> Result<()> {
        if !Self::table_has_column(conn, "chat_conversations", "session_id")? {
            conn.execute(
                "ALTER TABLE chat_conversations ADD COLUMN session_id TEXT NOT NULL DEFAULT 'default_session'",
                [],
            )?;
        }
        if !Self::table_has_column(conn, "chat_conversations", "runtime_state")? {
            conn.execute(
                "ALTER TABLE chat_conversations ADD COLUMN runtime_state TEXT NOT NULL DEFAULT 'idle'",
                [],
            )?;
        }
        if !Self::table_has_column(conn, "chat_conversations", "active_turn_id")? {
            conn.execute(
                "ALTER TABLE chat_conversations ADD COLUMN active_turn_id TEXT",
                [],
            )?;
        }

        conn.execute(
            "UPDATE chat_conversations
             SET session_id = ?1
             WHERE session_id IS NULL OR TRIM(session_id) = ''",
            [DEFAULT_CHAT_SESSION_ID],
        )?;
        conn.execute(
            "UPDATE chat_conversations
             SET runtime_state = ?1
             WHERE runtime_state IS NULL OR TRIM(runtime_state) = ''",
            [ChatTurnPhase::Idle.as_db_str()],
        )?;

        Ok(())
    }

    fn ensure_default_chat_session(&self, conn: &Connection) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR IGNORE INTO chat_sessions (id, label, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![DEFAULT_CHAT_SESSION_ID, "Default session", now.clone(), now],
        )?;
        Ok(())
    }

    fn ensure_default_chat_conversation(&self, conn: &Connection) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR IGNORE INTO chat_conversations (id, session_id, title, created_at, updated_at, runtime_state, active_turn_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
            params![
                DEFAULT_CHAT_CONVERSATION_ID,
                DEFAULT_CHAT_SESSION_ID,
                "Default chat",
                now.clone(),
                now,
                ChatTurnPhase::Idle.as_db_str(),
            ],
        )?;
        Ok(())
    }

    /// Create the database schema
    fn ensure_schema(&self) -> Result<()> {
        let conn = self.lock_conn()?;

        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS important_posts (
                id TEXT PRIMARY KEY,
                post_id TEXT NOT NULL,
                thread_id TEXT NOT NULL,
                post_body TEXT NOT NULL,
                why_important TEXT NOT NULL,
                importance_score REAL NOT NULL,
                marked_at TEXT NOT NULL
            )"#,
            [],
        )?;

        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS reflection_history (
                id TEXT PRIMARY KEY,
                reflected_at TEXT NOT NULL,
                old_prompt TEXT NOT NULL,
                new_prompt TEXT NOT NULL,
                reasoning TEXT NOT NULL,
                guiding_principles TEXT NOT NULL
            )"#,
            [],
        )?;

        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS agent_state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )"#,
            [],
        )?;

        // Create index for faster lookups
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_important_posts_marked_at ON important_posts(marked_at DESC)",
            [],
        )?;

        // Character card storage
        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS character_cards (
                id TEXT PRIMARY KEY,
                imported_at TEXT NOT NULL,
                format TEXT NOT NULL,
                original_data TEXT NOT NULL,
                derived_prompt TEXT NOT NULL,
                name TEXT,
                description TEXT
            )"#,
            [],
        )?;

        // Persona history for Ludonarrative Assonantic Tracing
        // Tracks personality evolution over time
        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS persona_history (
                id TEXT PRIMARY KEY,
                captured_at TEXT NOT NULL,
                traits_json TEXT NOT NULL,
                system_prompt TEXT NOT NULL,
                trigger TEXT NOT NULL,
                self_description TEXT NOT NULL,
                inferred_trajectory TEXT,
                formative_experiences_json TEXT NOT NULL
            )"#,
            [],
        )?;

        // Index for chronological persona queries
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_persona_history_captured_at ON persona_history(captured_at DESC)",
            [],
        )?;

        // Working memory - agent's persistent scratchpad
        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS working_memory (
                key TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )"#,
            [],
        )?;

        // Memory design archive
        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS memory_design_archive (
                id TEXT PRIMARY KEY,
                design_id TEXT NOT NULL,
                schema_version INTEGER NOT NULL,
                description TEXT,
                metadata_json TEXT,
                created_at TEXT NOT NULL,
                UNIQUE(design_id, schema_version)
            )"#,
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memory_design_archive_created_at ON memory_design_archive(created_at DESC)",
            [],
        )?;

        // Stored memory eval runs (raw report JSON)
        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS memory_eval_runs (
                id TEXT PRIMARY KEY,
                trace_set_name TEXT NOT NULL,
                report_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            )"#,
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memory_eval_runs_created_at ON memory_eval_runs(created_at DESC)",
            [],
        )?;

        // Promotion decisions derived from eval runs and policies
        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS memory_promotion_decisions (
                id TEXT PRIMARY KEY,
                eval_run_id TEXT NOT NULL,
                candidate_design_id TEXT NOT NULL,
                candidate_schema_version INTEGER NOT NULL,
                outcome TEXT NOT NULL,
                rationale TEXT NOT NULL,
                policy_json TEXT NOT NULL,
                metrics_snapshot_json TEXT NOT NULL,
                rollback_design_id TEXT NOT NULL,
                rollback_schema_version INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY(eval_run_id) REFERENCES memory_eval_runs(id)
            )"#,
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memory_promotion_decisions_created_at ON memory_promotion_decisions(created_at DESC)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memory_promotion_decisions_eval_run_id ON memory_promotion_decisions(eval_run_id)",
            [],
        )?;

        // Session metadata for grouping chat threads over long-running use.
        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS chat_sessions (
                id TEXT PRIMARY KEY,
                label TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )"#,
            [],
        )?;

        // Conversation metadata for private operator chat.
        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS chat_conversations (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL DEFAULT 'default_session',
                title TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                runtime_state TEXT NOT NULL DEFAULT 'idle',
                active_turn_id TEXT
            )"#,
            [],
        )?;

        // Compacted long-context snapshot for one conversation.
        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS chat_conversation_summaries (
                conversation_id TEXT PRIMARY KEY,
                summary_text TEXT NOT NULL,
                summarized_message_count INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL
            )"#,
            [],
        )?;

        // Private chat between operator and agent.
        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS chat_messages (
                id TEXT PRIMARY KEY,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL,
                processed INTEGER NOT NULL DEFAULT 0,
                turn_id TEXT
            )"#,
            [],
        )?;

        // Conversation turns with explicit lifecycle.
        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS chat_turns (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                conversation_id TEXT NOT NULL,
                iteration INTEGER NOT NULL,
                phase_state TEXT NOT NULL,
                decision TEXT,
                status TEXT,
                trigger_message_ids_json TEXT NOT NULL,
                operator_message TEXT,
                reason TEXT,
                error TEXT,
                tool_call_count INTEGER NOT NULL DEFAULT 0,
                started_at TEXT NOT NULL,
                completed_at TEXT,
                agent_message_id TEXT
            )"#,
            [],
        )?;

        // Tool output lineage for each turn.
        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS chat_turn_tool_calls (
                id TEXT PRIMARY KEY,
                turn_id TEXT NOT NULL,
                call_index INTEGER NOT NULL,
                tool_name TEXT NOT NULL,
                arguments_json TEXT NOT NULL,
                output_text TEXT NOT NULL,
                created_at TEXT NOT NULL
            )"#,
            [],
        )?;

        // Living Loop foundation: private journal entries.
        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS journal_entries (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                entry_type TEXT NOT NULL,
                content TEXT NOT NULL,
                trigger TEXT,
                user_state_at_time TEXT,
                time_of_day TEXT,
                related_concerns TEXT,
                mood_valence REAL,
                mood_arousal REAL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )"#,
            [],
        )?;

        // Living Loop foundation: ongoing concerns.
        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS concerns (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                last_touched TEXT NOT NULL,
                summary TEXT NOT NULL,
                concern_type TEXT NOT NULL,
                salience TEXT NOT NULL,
                my_thoughts TEXT,
                related_memory_keys TEXT,
                context TEXT,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )"#,
            [],
        )?;

        // Living Loop foundation: orientation snapshots for debugging/analysis.
        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS orientation_snapshots (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                user_state TEXT NOT NULL,
                disposition TEXT NOT NULL,
                synthesis TEXT NOT NULL,
                salience_map TEXT,
                anomalies TEXT,
                pending_thoughts TEXT,
                mood_valence REAL,
                mood_arousal REAL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )"#,
            [],
        )?;

        // Living Loop foundation: queued thoughts to surface later.
        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS pending_thoughts_queue (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                context TEXT,
                priority REAL NOT NULL DEFAULT 0.5,
                relates_to TEXT,
                created_at TEXT NOT NULL,
                surfaced_at TEXT,
                dismissed_at TEXT
            )"#,
            [],
        )?;

        self.ensure_chat_messages_conversation_column(&conn)?;
        self.ensure_chat_conversations_runtime_columns(&conn)?;
        self.ensure_default_chat_session(&conn)?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chat_messages_created_at ON chat_messages(created_at ASC)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chat_messages_conversation_created_at ON chat_messages(conversation_id, created_at ASC)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chat_messages_turn_id ON chat_messages(turn_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chat_conversations_session_updated ON chat_conversations(session_id, updated_at DESC)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chat_conversations_runtime_state ON chat_conversations(runtime_state)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chat_conversation_summaries_updated_at ON chat_conversation_summaries(updated_at DESC)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chat_turns_conversation_started ON chat_turns(conversation_id, started_at DESC)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chat_turns_phase_state ON chat_turns(phase_state)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chat_turn_tool_calls_turn_idx ON chat_turn_tool_calls(turn_id, call_index ASC)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_journal_timestamp ON journal_entries(timestamp DESC)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_journal_type ON journal_entries(entry_type)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_concerns_salience ON concerns(salience)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_concerns_last_touched ON concerns(last_touched DESC)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_orientation_timestamp ON orientation_snapshots(timestamp DESC)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_pending_unsurfaced ON pending_thoughts_queue(surfaced_at) WHERE surfaced_at IS NULL",
            [],
        )?;

        self.ensure_default_chat_conversation(&conn)?;

        Ok(())
    }

    /// Save an important post
    pub fn save_important_post(&self, post: &ImportantPost) -> Result<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO important_posts (id, post_id, thread_id, post_body, why_important, importance_score, marked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                post.id,
                post.post_id,
                post.thread_id,
                post.post_body,
                post.why_important,
                post.importance_score,
                post.marked_at.to_rfc3339()
            ],
        )?;
        Ok(())
    }

    /// Get the N most recent important posts
    pub fn get_recent_important_posts(&self, limit: usize) -> Result<Vec<ImportantPost>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, post_id, thread_id, post_body, why_important, importance_score, marked_at
             FROM important_posts
             ORDER BY marked_at DESC
             LIMIT ?1",
        )?;

        let posts = stmt
            .query_map([limit], |row| {
                Ok(ImportantPost {
                    id: row.get(0)?,
                    post_id: row.get(1)?,
                    thread_id: row.get(2)?,
                    post_body: row.get(3)?,
                    why_important: row.get(4)?,
                    importance_score: row.get(5)?,
                    marked_at: row.get::<_, String>(6)?.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            6,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(posts)
    }

    /// Count total important posts
    pub fn count_important_posts(&self) -> Result<usize> {
        let conn = self.lock_conn()?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM important_posts", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Get all important posts ordered by score (lowest first)
    pub fn get_all_important_posts_by_score(&self) -> Result<Vec<ImportantPost>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, post_id, thread_id, post_body, why_important, importance_score, marked_at
             FROM important_posts
             ORDER BY importance_score ASC",
        )?;

        let posts = stmt
            .query_map([], |row| {
                Ok(ImportantPost {
                    id: row.get(0)?,
                    post_id: row.get(1)?,
                    thread_id: row.get(2)?,
                    post_body: row.get(3)?,
                    why_important: row.get(4)?,
                    importance_score: row.get(5)?,
                    marked_at: row.get::<_, String>(6)?.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            6,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(posts)
    }

    /// Delete an important post by ID
    pub fn delete_important_post(&self, id: &str) -> Result<()> {
        let conn = self.lock_conn()?;
        conn.execute("DELETE FROM important_posts WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Save a reflection record
    pub fn save_reflection(&self, reflection: &ReflectionRecord) -> Result<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO reflection_history (id, reflected_at, old_prompt, new_prompt, reasoning, guiding_principles)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                reflection.id,
                reflection.reflected_at.to_rfc3339(),
                reflection.old_prompt,
                reflection.new_prompt,
                reflection.reasoning,
                reflection.guiding_principles
            ],
        )?;
        Ok(())
    }

    /// Get reflection history
    pub fn get_reflection_history(&self, limit: usize) -> Result<Vec<ReflectionRecord>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, reflected_at, old_prompt, new_prompt, reasoning, guiding_principles
             FROM reflection_history
             ORDER BY reflected_at DESC
             LIMIT ?1",
        )?;

        let reflections = stmt
            .query_map([limit], |row| {
                Ok(ReflectionRecord {
                    id: row.get(0)?,
                    reflected_at: row.get::<_, String>(1)?.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    old_prompt: row.get(2)?,
                    new_prompt: row.get(3)?,
                    reasoning: row.get(4)?,
                    guiding_principles: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(reflections)
    }

    /// Get a state value
    pub fn get_state(&self, key: &str) -> Result<Option<String>> {
        let conn = self.lock_conn()?;
        let result = conn.query_row(
            "SELECT value FROM agent_state WHERE key = ?1",
            [key],
            |row| row.get(0),
        );

        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Set a state value
    pub fn set_state(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO agent_state (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    /// Get persisted memory design metadata.
    pub fn get_memory_design_version(&self) -> Result<Option<MemoryDesignVersion>> {
        let design = self.get_state(MEMORY_DESIGN_STATE_KEY)?;
        let version = self.get_state(MEMORY_SCHEMA_VERSION_STATE_KEY)?;

        match (design, version) {
            (None, None) => Ok(None),
            (Some(design_id), Some(version_str)) => {
                let schema_version = version_str.parse::<u32>().with_context(|| {
                    format!(
                        "Invalid {} value: {}",
                        MEMORY_SCHEMA_VERSION_STATE_KEY, version_str
                    )
                })?;
                Ok(Some(MemoryDesignVersion {
                    design_id,
                    schema_version,
                }))
            }
            _ => anyhow::bail!(
                "Incomplete memory design metadata in agent_state: expected both '{}' and '{}'",
                MEMORY_DESIGN_STATE_KEY,
                MEMORY_SCHEMA_VERSION_STATE_KEY
            ),
        }
    }

    /// Persist memory design metadata.
    pub fn set_memory_design_version(&self, design: &MemoryDesignVersion) -> Result<()> {
        self.set_state(MEMORY_DESIGN_STATE_KEY, &design.design_id)?;
        self.set_state(
            MEMORY_SCHEMA_VERSION_STATE_KEY,
            &design.schema_version.to_string(),
        )
    }

    /// Apply a direct memory migration and update persisted version metadata.
    pub fn apply_memory_migration(&self, target: &MemoryDesignVersion) -> Result<()> {
        let current = self
            .get_memory_design_version()?
            .unwrap_or_else(|| self.memory_backend.design_version());

        if current == *target {
            return Ok(());
        }

        let conn = self.lock_conn()?;
        self.migration_registry
            .apply_direct(&conn, &current, target)?;
        drop(conn);

        self.set_memory_design_version(target)
    }

    /// Save a versioned memory design into archive storage.
    pub fn save_memory_design_archive_entry(&self, entry: &MemoryDesignArchiveEntry) -> Result<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO memory_design_archive
             (id, design_id, schema_version, description, metadata_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                entry.id,
                entry.design_version.design_id,
                entry.design_version.schema_version as i64,
                entry.description,
                entry.metadata_json,
                entry.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// List recent archived memory designs.
    pub fn list_memory_design_archive_entries(
        &self,
        limit: usize,
    ) -> Result<Vec<MemoryDesignArchiveEntry>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, design_id, schema_version, description, metadata_json, created_at
             FROM memory_design_archive
             ORDER BY created_at DESC
             LIMIT ?1",
        )?;

        let rows = stmt
            .query_map([limit as i64], |row| {
                Ok(MemoryDesignArchiveEntry {
                    id: row.get(0)?,
                    design_version: MemoryDesignVersion {
                        design_id: row.get(1)?,
                        schema_version: row.get::<_, i64>(2)? as u32,
                    },
                    description: row.get(3)?,
                    metadata_json: row.get(4)?,
                    created_at: row.get::<_, String>(5)?.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    /// Persist a memory evaluation run report.
    pub fn save_memory_eval_run(&self, run: &MemoryEvalRunRecord) -> Result<()> {
        let conn = self.lock_conn()?;
        let report_json =
            serde_json::to_string(&run.report).context("Failed to serialize memory eval report")?;
        conn.execute(
            "INSERT OR REPLACE INTO memory_eval_runs (id, trace_set_name, report_json, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                run.id,
                run.trace_set_name,
                report_json,
                run.created_at.to_rfc3339()
            ],
        )?;
        Ok(())
    }

    /// Load a memory evaluation run report by ID.
    pub fn get_memory_eval_run(&self, id: &str) -> Result<Option<MemoryEvalRunRecord>> {
        let conn = self.lock_conn()?;
        let result = conn.query_row(
            "SELECT id, trace_set_name, report_json, created_at
             FROM memory_eval_runs
             WHERE id = ?1",
            [id],
            |row| {
                let report_json: String = row.get(2)?;
                let report = serde_json::from_str(&report_json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?;
                Ok(MemoryEvalRunRecord {
                    id: row.get(0)?,
                    trace_set_name: row.get(1)?,
                    report,
                    created_at: row.get::<_, String>(3)?.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            3,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                })
            },
        );

        match result {
            Ok(run) => Ok(Some(run)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Save a memory promotion decision record.
    ///
    /// Rollback fields are always persisted as NOT NULL columns, ensuring
    /// every decision includes an explicit fallback target.
    pub fn save_memory_promotion_decision(
        &self,
        decision: &MemoryPromotionDecisionRecord,
    ) -> Result<()> {
        let conn = self.lock_conn()?;
        let policy_json = serde_json::to_string(&decision.policy)
            .context("Failed to serialize promotion policy")?;
        let metrics_snapshot_json = serde_json::to_string(&decision.metrics_snapshot)
            .context("Failed to serialize promotion metrics snapshot")?;
        conn.execute(
            "INSERT OR REPLACE INTO memory_promotion_decisions
             (id, eval_run_id, candidate_design_id, candidate_schema_version, outcome, rationale,
              policy_json, metrics_snapshot_json, rollback_design_id, rollback_schema_version, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                decision.id,
                decision.eval_run_id,
                decision.candidate_design.design_id,
                decision.candidate_design.schema_version as i64,
                outcome_to_db(&decision.outcome),
                decision.rationale,
                policy_json,
                metrics_snapshot_json,
                decision.rollback_target.design_id,
                decision.rollback_target.schema_version as i64,
                decision.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Load a promotion decision by ID.
    pub fn get_memory_promotion_decision(
        &self,
        id: &str,
    ) -> Result<Option<MemoryPromotionDecisionRecord>> {
        let conn = self.lock_conn()?;
        let result = conn.query_row(
            "SELECT id, eval_run_id, candidate_design_id, candidate_schema_version, outcome, rationale,
                    policy_json, metrics_snapshot_json, rollback_design_id, rollback_schema_version, created_at
             FROM memory_promotion_decisions
             WHERE id = ?1",
            [id],
            |row| {
                let policy_json: String = row.get(6)?;
                let metrics_json: String = row.get(7)?;
                let policy: MemoryPromotionPolicy = serde_json::from_str(&policy_json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        6,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?;
                let metrics_snapshot: PromotionMetricsSnapshot =
                    serde_json::from_str(&metrics_json).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            7,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                let outcome_raw: String = row.get(4)?;
                let outcome = match outcome_raw.as_str() {
                    "promote" => PromotionOutcome::Promote,
                    "hold" => PromotionOutcome::Hold,
                    _ => {
                        return Err(rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!("Unknown promotion outcome '{}'", outcome_raw),
                            )),
                        ));
                    }
                };

                Ok(MemoryPromotionDecisionRecord {
                    id: row.get(0)?,
                    eval_run_id: row.get(1)?,
                    candidate_design: MemoryDesignVersion {
                        design_id: row.get(2)?,
                        schema_version: row.get::<_, i64>(3)? as u32,
                    },
                    outcome,
                    rationale: row.get(5)?,
                    policy,
                    metrics_snapshot,
                    rollback_target: MemoryDesignVersion {
                        design_id: row.get(8)?,
                        schema_version: row.get::<_, i64>(9)? as u32,
                    },
                    created_at: row.get::<_, String>(10)?.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            10,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                })
            },
        );

        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Evaluate promotion policy against a stored eval run, then persist the decision.
    pub fn evaluate_and_record_memory_promotion(
        &self,
        eval_run_id: &str,
        baseline_backend_id: &str,
        candidate_backend_id: &str,
        policy: &MemoryPromotionPolicy,
    ) -> Result<MemoryPromotionDecisionRecord> {
        let run = self
            .get_memory_eval_run(eval_run_id)?
            .with_context(|| format!("Missing memory eval run '{}'", eval_run_id))?;

        let current_design = self
            .get_memory_design_version()?
            .unwrap_or_else(|| self.memory_backend.design_version());

        let decision = evaluate_promotion_policy(
            &run.id,
            &run.report,
            baseline_backend_id,
            candidate_backend_id,
            &current_design,
            policy,
        )?;
        self.save_memory_promotion_decision(&decision)?;
        Ok(decision)
    }

    /// Recompute a stored promotion decision from archived metrics and policy.
    ///
    /// This is used to verify decision reproducibility from persisted artifacts.
    pub fn recompute_memory_promotion_decision(
        &self,
        decision_id: &str,
    ) -> Result<MemoryPromotionDecisionRecord> {
        let decision = self
            .get_memory_promotion_decision(decision_id)?
            .with_context(|| format!("Missing memory promotion decision '{}'", decision_id))?;
        let run = self
            .get_memory_eval_run(&decision.eval_run_id)?
            .with_context(|| format!("Missing memory eval run '{}'", decision.eval_run_id))?;

        evaluate_promotion_policy(
            &run.id,
            &run.report,
            &decision.metrics_snapshot.baseline_backend_id,
            &decision.metrics_snapshot.candidate_backend_id,
            &decision.rollback_target,
            &decision.policy,
        )
    }

    /// Get current system prompt
    pub fn get_current_system_prompt(&self) -> Result<Option<String>> {
        self.get_state("current_system_prompt")
    }

    /// Set current system prompt
    pub fn set_current_system_prompt(&self, prompt: &str) -> Result<()> {
        self.set_state("current_system_prompt", prompt)
    }

    /// Get last reflection time
    pub fn get_last_reflection_time(&self) -> Result<Option<DateTime<Utc>>> {
        if let Some(time_str) = self.get_state("last_reflection_time")? {
            let time: DateTime<Utc> = time_str
                .parse()
                .context("Failed to parse reflection time")?;
            Ok(Some(time))
        } else {
            Ok(None)
        }
    }

    /// Set last reflection time
    pub fn set_last_reflection_time(&self, time: DateTime<Utc>) -> Result<()> {
        self.set_state("last_reflection_time", &time.to_rfc3339())
    }

    /// Save a character card (only keeps one at a time)
    pub fn save_character_card(&self, card: &CharacterCard) -> Result<()> {
        let conn = self.lock_conn()?;
        // Delete any existing character card
        conn.execute("DELETE FROM character_cards", [])?;

        // Insert the new one
        conn.execute(
            "INSERT INTO character_cards (id, imported_at, format, original_data, derived_prompt, name, description)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                card.id,
                card.imported_at.to_rfc3339(),
                card.format,
                card.original_data,
                card.derived_prompt,
                card.name,
                card.description
            ],
        )?;
        Ok(())
    }

    /// Get the current character card (if any)
    pub fn get_character_card(&self) -> Result<Option<CharacterCard>> {
        let conn = self.lock_conn()?;
        let result = conn.query_row(
            "SELECT id, imported_at, format, original_data, derived_prompt, name, description
             FROM character_cards
             ORDER BY imported_at DESC
             LIMIT 1",
            [],
            |row| {
                Ok(CharacterCard {
                    id: row.get(0)?,
                    imported_at: row.get::<_, String>(1)?.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    format: row.get(2)?,
                    original_data: row.get(3)?,
                    derived_prompt: row.get(4)?,
                    name: row.get(5)?,
                    description: row.get(6)?,
                })
            },
        );

        match result {
            Ok(card) => Ok(Some(card)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Delete the current character card
    pub fn delete_character_card(&self) -> Result<()> {
        let conn = self.lock_conn()?;
        conn.execute("DELETE FROM character_cards", [])?;
        Ok(())
    }

    // ========================================================================
    // Persona History - Ludonarrative Assonantic Tracing
    // ========================================================================

    /// Save a persona snapshot
    pub fn save_persona_snapshot(&self, snapshot: &PersonaSnapshot) -> Result<()> {
        let traits_json = serde_json::to_string(&snapshot.traits)
            .context("Failed to serialize persona traits")?;
        let experiences_json = serde_json::to_string(&snapshot.formative_experiences)
            .context("Failed to serialize formative experiences")?;

        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO persona_history (id, captured_at, traits_json, system_prompt, trigger, self_description, inferred_trajectory, formative_experiences_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                snapshot.id,
                snapshot.captured_at.to_rfc3339(),
                traits_json,
                snapshot.system_prompt,
                snapshot.trigger,
                snapshot.self_description,
                snapshot.inferred_trajectory,
                experiences_json
            ],
        )?;
        Ok(())
    }

    /// Get the most recent persona snapshots
    pub fn get_persona_history(&self, limit: usize) -> Result<Vec<PersonaSnapshot>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, captured_at, traits_json, system_prompt, trigger, self_description, inferred_trajectory, formative_experiences_json
             FROM persona_history
             ORDER BY captured_at DESC
             LIMIT ?1",
        )?;

        let snapshots = stmt
            .query_map([limit], |row| {
                let traits_json: String = row.get(2)?;
                let experiences_json: String = row.get(7)?;

                Ok(PersonaSnapshot {
                    id: row.get(0)?,
                    captured_at: row.get::<_, String>(1)?.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    traits: serde_json::from_str(&traits_json).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            2,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    system_prompt: row.get(3)?,
                    trigger: row.get(4)?,
                    self_description: row.get(5)?,
                    inferred_trajectory: row.get(6)?,
                    formative_experiences: serde_json::from_str(&experiences_json).map_err(
                        |e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                7,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        },
                    )?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(snapshots)
    }

    /// Get persona snapshots within a date range
    pub fn get_persona_history_range(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<PersonaSnapshot>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, captured_at, traits_json, system_prompt, trigger, self_description, inferred_trajectory, formative_experiences_json
             FROM persona_history
             WHERE captured_at >= ?1 AND captured_at <= ?2
             ORDER BY captured_at ASC",
        )?;

        let snapshots = stmt
            .query_map([from.to_rfc3339(), to.to_rfc3339()], |row| {
                let traits_json: String = row.get(2)?;
                let experiences_json: String = row.get(7)?;

                Ok(PersonaSnapshot {
                    id: row.get(0)?,
                    captured_at: row.get::<_, String>(1)?.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    traits: serde_json::from_str(&traits_json).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            2,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    system_prompt: row.get(3)?,
                    trigger: row.get(4)?,
                    self_description: row.get(5)?,
                    inferred_trajectory: row.get(6)?,
                    formative_experiences: serde_json::from_str(&experiences_json).map_err(
                        |e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                7,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        },
                    )?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(snapshots)
    }

    /// Get the most recent persona snapshot
    pub fn get_latest_persona(&self) -> Result<Option<PersonaSnapshot>> {
        let snapshots = self.get_persona_history(1)?;
        Ok(snapshots.into_iter().next())
    }

    /// Count total persona snapshots
    pub fn count_persona_snapshots(&self) -> Result<usize> {
        let conn = self.lock_conn()?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM persona_history", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    // ========================================================================
    // Working Memory - Agent's Persistent Scratchpad
    // ========================================================================

    /// Set a working memory entry (creates or updates)
    pub fn set_working_memory(&self, key: &str, content: &str) -> Result<()> {
        let conn = self.lock_conn()?;
        self.memory_backend.set_entry(&conn, key, content)
    }

    /// Get a working memory entry by key
    pub fn get_working_memory(&self, key: &str) -> Result<Option<WorkingMemoryEntry>> {
        let conn = self.lock_conn()?;
        self.memory_backend.get_entry(&conn, key)
    }

    /// Get all working memory entries
    pub fn get_all_working_memory(&self) -> Result<Vec<WorkingMemoryEntry>> {
        let conn = self.lock_conn()?;
        self.memory_backend.list_entries(&conn)
    }

    /// Search working memory entries using a simple relevance rank over key/content text.
    pub fn search_working_memory(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<WorkingMemoryEntry>> {
        let max_results = limit.max(1);
        let trimmed_query = query.trim();
        let mut entries = self.get_all_working_memory()?;
        if trimmed_query.is_empty() {
            entries.truncate(max_results);
            return Ok(entries);
        }

        let query_lower = trimmed_query.to_ascii_lowercase();
        let terms: Vec<&str> = query_lower
            .split_whitespace()
            .filter(|t| !t.is_empty())
            .collect();

        let mut ranked = entries
            .into_iter()
            .filter_map(|entry| {
                let key_lower = entry.key.to_ascii_lowercase();
                let content_lower = entry.content.to_ascii_lowercase();

                let mut score: i32 = 0;
                if key_lower.contains(&query_lower) {
                    score += 6;
                }
                if content_lower.contains(&query_lower) {
                    score += 5;
                }
                for term in &terms {
                    if key_lower.contains(term) {
                        score += 3;
                    }
                    if content_lower.contains(term) {
                        score += 1;
                    }
                }

                if score > 0 {
                    Some((score, entry))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        ranked.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| b.1.updated_at.cmp(&a.1.updated_at))
        });
        ranked.truncate(max_results);
        Ok(ranked.into_iter().map(|(_, entry)| entry).collect())
    }

    /// Append one line to today's activity log in working memory.
    pub fn append_daily_activity_log(&self, entry: &str) -> Result<()> {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            return Ok(());
        }

        let now = Utc::now();
        let day_key = format!("activity-log-{}", now.format("%Y-%m-%d"));
        let line = format!("- [{} UTC] {}", now.format("%H:%M:%S"), trimmed);
        let existing = self
            .get_working_memory(&day_key)?
            .map(|item| item.content)
            .unwrap_or_default();

        let merged = if existing.trim().is_empty() {
            format!(
                "Daily activity log for {}\n\n{}",
                now.format("%Y-%m-%d"),
                line
            )
        } else {
            format!("{}\n{}", existing.trim_end(), line)
        };

        self.set_working_memory(&day_key, &merged)
    }

    /// Delete a working memory entry
    pub fn delete_working_memory(&self, key: &str) -> Result<()> {
        let conn = self.lock_conn()?;
        self.memory_backend.delete_entry(&conn, key)
    }

    /// Get working memory as a formatted string for inclusion in context
    pub fn get_working_memory_context(&self) -> Result<String> {
        let entries = self.get_all_working_memory()?;
        if entries.is_empty() {
            return Ok(String::new());
        }

        let mut context = String::from("## Your Working Memory (Notes to Self)\n\n");
        for entry in entries {
            context.push_str(&format!("### {}\n{}\n\n", entry.key, entry.content));
        }
        Ok(context)
    }

    // ========================================================================
    // Living Loop Foundation - Journal
    // ========================================================================

    pub fn add_journal_entry(&self, entry: &JournalEntry) -> Result<()> {
        let related_concerns_json = serde_json::to_string(&entry.related_concerns)
            .context("Failed to serialize journal related concerns")?;
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO journal_entries
             (id, timestamp, entry_type, content, trigger, user_state_at_time, time_of_day,
              related_concerns, mood_valence, mood_arousal, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                entry.id,
                entry.timestamp.to_rfc3339(),
                entry.entry_type.as_db_str(),
                entry.content,
                entry.context.trigger,
                entry.context.user_state_at_time,
                entry.context.time_of_day,
                related_concerns_json,
                entry.mood_at_time.as_ref().map(|m| m.valence),
                entry.mood_at_time.as_ref().map(|m| m.arousal),
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_recent_journal(&self, limit: usize) -> Result<Vec<JournalEntry>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, entry_type, content, trigger, user_state_at_time, time_of_day,
                    related_concerns, mood_valence, mood_arousal
             FROM journal_entries
             ORDER BY timestamp DESC
             LIMIT ?1",
        )?;

        let entries = stmt
            .query_map([limit.max(1)], |row| {
                let timestamp_raw: String = row.get(1)?;
                let related_raw: Option<String> = row.get(7)?;
                let related_concerns = related_raw
                    .as_deref()
                    .map(serde_json::from_str::<Vec<String>>)
                    .transpose()
                    .map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            7,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?
                    .unwrap_or_default();
                let mood_valence: Option<f32> = row.get(8)?;
                let mood_arousal: Option<f32> = row.get(9)?;

                Ok(JournalEntry {
                    id: row.get(0)?,
                    timestamp: timestamp_raw.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    entry_type: JournalEntryType::from_db(&row.get::<_, String>(2)?),
                    content: row.get(3)?,
                    context: JournalContext {
                        trigger: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                        user_state_at_time: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                        time_of_day: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
                    },
                    related_concerns,
                    mood_at_time: match (mood_valence, mood_arousal) {
                        (Some(valence), Some(arousal)) => Some(JournalMood { valence, arousal }),
                        _ => None,
                    },
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(entries)
    }

    pub fn get_journal_for_context(&self, max_tokens: usize) -> Result<String> {
        if max_tokens == 0 {
            return Ok(String::new());
        }

        let entries = self.get_recent_journal(64)?;
        if entries.is_empty() {
            return Ok(String::new());
        }

        let mut token_budget = 0usize;
        let mut out = String::from("## Recent Journal Notes\n\n");
        for entry in entries {
            let line = format!(
                "- [{}] ({}) {}\n",
                entry.timestamp.format("%Y-%m-%d %H:%M"),
                entry.entry_type.as_db_str(),
                entry.content.trim()
            );
            let est_tokens = line.split_whitespace().count();
            if token_budget + est_tokens > max_tokens {
                break;
            }
            token_budget += est_tokens;
            out.push_str(&line);
        }

        if token_budget == 0 {
            Ok(String::new())
        } else {
            Ok(out)
        }
    }

    pub fn search_journal(&self, query: &str, limit: usize) -> Result<Vec<JournalEntry>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return self.get_recent_journal(limit.max(1));
        }

        let like_pattern = format!("%{}%", trimmed.to_ascii_lowercase());
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, entry_type, content, trigger, user_state_at_time, time_of_day,
                    related_concerns, mood_valence, mood_arousal
             FROM journal_entries
             WHERE LOWER(content) LIKE ?1 OR LOWER(COALESCE(trigger, '')) LIKE ?1
             ORDER BY timestamp DESC
             LIMIT ?2",
        )?;

        let entries = stmt
            .query_map(params![like_pattern, limit.max(1)], |row| {
                let timestamp_raw: String = row.get(1)?;
                let related_raw: Option<String> = row.get(7)?;
                let related_concerns = related_raw
                    .as_deref()
                    .map(serde_json::from_str::<Vec<String>>)
                    .transpose()
                    .map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            7,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?
                    .unwrap_or_default();
                let mood_valence: Option<f32> = row.get(8)?;
                let mood_arousal: Option<f32> = row.get(9)?;

                Ok(JournalEntry {
                    id: row.get(0)?,
                    timestamp: timestamp_raw.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    entry_type: JournalEntryType::from_db(&row.get::<_, String>(2)?),
                    content: row.get(3)?,
                    context: JournalContext {
                        trigger: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                        user_state_at_time: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                        time_of_day: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
                    },
                    related_concerns,
                    mood_at_time: match (mood_valence, mood_arousal) {
                        (Some(valence), Some(arousal)) => Some(JournalMood { valence, arousal }),
                        _ => None,
                    },
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(entries)
    }

    // ========================================================================
    // Living Loop Foundation - Concerns
    // ========================================================================

    pub fn save_concern(&self, concern: &Concern) -> Result<()> {
        let concern_type_json = serde_json::to_string(&concern.concern_type)
            .context("Failed to serialize concern type")?;
        let related_keys_json = serde_json::to_string(&concern.related_memory_keys)
            .context("Failed to serialize concern related memory keys")?;
        let context_json = serde_json::to_string(&concern.context)
            .context("Failed to serialize concern context")?;

        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO concerns
             (id, created_at, last_touched, summary, concern_type, salience, my_thoughts,
              related_memory_keys, context, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                concern.id,
                concern.created_at.to_rfc3339(),
                concern.last_touched.to_rfc3339(),
                concern.summary,
                concern_type_json,
                concern.salience.as_db_str(),
                concern.my_thoughts,
                related_keys_json,
                context_json,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_concern(&self, id: &str) -> Result<Option<Concern>> {
        let conn = self.lock_conn()?;
        let result = conn.query_row(
            "SELECT id, created_at, last_touched, summary, concern_type, salience, my_thoughts,
                    related_memory_keys, context
             FROM concerns
             WHERE id = ?1",
            [id],
            |row| {
                let created_raw: String = row.get(1)?;
                let touched_raw: String = row.get(2)?;
                let concern_type_raw: String = row.get(4)?;
                let salience_raw: String = row.get(5)?;
                let related_raw: Option<String> = row.get(7)?;
                let context_raw: Option<String> = row.get(8)?;

                let concern_type: ConcernType =
                    serde_json::from_str(&concern_type_raw).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                let related_memory_keys = related_raw
                    .as_deref()
                    .map(serde_json::from_str::<Vec<String>>)
                    .transpose()
                    .map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            7,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?
                    .unwrap_or_default();
                let context: ConcernContext = context_raw
                    .as_deref()
                    .map(serde_json::from_str::<ConcernContext>)
                    .transpose()
                    .map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            8,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?
                    .unwrap_or_default();

                Ok(Concern {
                    id: row.get(0)?,
                    created_at: created_raw.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    last_touched: touched_raw.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            2,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    summary: row.get(3)?,
                    concern_type,
                    salience: Salience::from_db(&salience_raw),
                    my_thoughts: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
                    related_memory_keys,
                    context,
                })
            },
        );

        match result {
            Ok(concern) => Ok(Some(concern)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn get_active_concerns(&self) -> Result<Vec<Concern>> {
        let ids = {
            let conn = self.lock_conn()?;
            let mut stmt = conn.prepare(
                "SELECT id
                 FROM concerns
                 WHERE salience IN (?1, ?2)
                 ORDER BY last_touched DESC",
            )?;
            let rows = stmt.query_map(
                params![
                    Salience::Active.as_db_str(),
                    Salience::Monitoring.as_db_str()
                ],
                |row| row.get::<_, String>(0),
            )?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let mut concerns = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(concern) = self.get_concern(&id)? {
                concerns.push(concern);
            }
        }
        Ok(concerns)
    }

    pub fn get_all_concerns(&self) -> Result<Vec<Concern>> {
        let ids = {
            let conn = self.lock_conn()?;
            let mut stmt = conn.prepare("SELECT id FROM concerns ORDER BY last_touched DESC")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let mut concerns = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(concern) = self.get_concern(&id)? {
                concerns.push(concern);
            }
        }
        Ok(concerns)
    }

    pub fn update_concern_salience(&self, id: &str, salience: Salience) -> Result<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE concerns
             SET salience = ?2, updated_at = ?3
             WHERE id = ?1",
            params![id, salience.as_db_str(), Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn touch_concern(&self, id: &str, reason: &str) -> Result<()> {
        let Some(mut concern) = self.get_concern(id)? else {
            return Ok(());
        };
        concern.last_touched = Utc::now();
        concern.context.last_update_reason = reason.to_string();
        self.save_concern(&concern)
    }

    // ========================================================================
    // Living Loop Foundation - Orientation Snapshots
    // ========================================================================

    pub fn save_orientation_snapshot(&self, orientation: &OrientationSnapshotRecord) -> Result<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO orientation_snapshots
             (id, timestamp, user_state, disposition, synthesis, salience_map, anomalies,
              pending_thoughts, mood_valence, mood_arousal, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                orientation.id,
                orientation.timestamp.to_rfc3339(),
                serde_json::to_string(&orientation.user_state)
                    .context("Failed to serialize orientation user_state")?,
                orientation.disposition,
                orientation.synthesis,
                serde_json::to_string(&orientation.salience_map)
                    .context("Failed to serialize orientation salience_map")?,
                serde_json::to_string(&orientation.anomalies)
                    .context("Failed to serialize orientation anomalies")?,
                serde_json::to_string(&orientation.pending_thoughts)
                    .context("Failed to serialize orientation pending_thoughts")?,
                orientation.mood_valence,
                orientation.mood_arousal,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_recent_orientations(&self, limit: usize) -> Result<Vec<OrientationSnapshotRecord>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, user_state, disposition, synthesis, salience_map, anomalies,
                    pending_thoughts, mood_valence, mood_arousal
             FROM orientation_snapshots
             ORDER BY timestamp DESC
             LIMIT ?1",
        )?;
        let snapshots = stmt
            .query_map([limit.max(1)], |row| {
                let timestamp_raw: String = row.get(1)?;
                let user_state_raw: String = row.get(2)?;
                let salience_map_raw: Option<String> = row.get(5)?;
                let anomalies_raw: Option<String> = row.get(6)?;
                let pending_thoughts_raw: Option<String> = row.get(7)?;
                Ok(OrientationSnapshotRecord {
                    id: row.get(0)?,
                    timestamp: timestamp_raw.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    user_state: serde_json::from_str(&user_state_raw).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            2,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    disposition: row.get(3)?,
                    synthesis: row.get(4)?,
                    salience_map: match salience_map_raw {
                        Some(raw) => serde_json::from_str(&raw).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                5,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?,
                        None => serde_json::json!([]),
                    },
                    anomalies: match anomalies_raw {
                        Some(raw) => serde_json::from_str(&raw).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                6,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?,
                        None => serde_json::json!([]),
                    },
                    pending_thoughts: match pending_thoughts_raw {
                        Some(raw) => serde_json::from_str(&raw).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                7,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?,
                        None => serde_json::json!([]),
                    },
                    mood_valence: row.get(8)?,
                    mood_arousal: row.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(snapshots)
    }

    // ========================================================================
    // Living Loop Foundation - Pending Thought Queue
    // ========================================================================

    pub fn queue_pending_thought(&self, thought: &PendingThoughtRecord) -> Result<()> {
        let conn = self.lock_conn()?;
        let relates_to_json = serde_json::to_string(&thought.relates_to)
            .context("Failed to serialize pending_thought relates_to")?;
        conn.execute(
            "INSERT OR REPLACE INTO pending_thoughts_queue
             (id, content, context, priority, relates_to, created_at, surfaced_at, dismissed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                thought.id,
                thought.content,
                thought.context,
                thought.priority,
                relates_to_json,
                thought.created_at.to_rfc3339(),
                thought.surfaced_at.map(|v| v.to_rfc3339()),
                thought.dismissed_at.map(|v| v.to_rfc3339()),
            ],
        )?;
        Ok(())
    }

    pub fn get_unsurfaced_thoughts(&self) -> Result<Vec<PendingThoughtRecord>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, content, context, priority, relates_to, created_at, surfaced_at, dismissed_at
             FROM pending_thoughts_queue
             WHERE surfaced_at IS NULL AND dismissed_at IS NULL
             ORDER BY priority DESC, created_at ASC",
        )?;
        let thoughts = stmt
            .query_map([], |row| {
                let relates_to_raw: Option<String> = row.get(4)?;
                let created_at_raw: String = row.get(5)?;
                let surfaced_at_raw: Option<String> = row.get(6)?;
                let dismissed_at_raw: Option<String> = row.get(7)?;
                Ok(PendingThoughtRecord {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    context: row.get(2)?,
                    priority: row.get(3)?,
                    relates_to: relates_to_raw
                        .as_deref()
                        .map(serde_json::from_str::<Vec<String>>)
                        .transpose()
                        .map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                4,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?
                        .unwrap_or_default(),
                    created_at: created_at_raw.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    surfaced_at: match surfaced_at_raw {
                        Some(raw) => Some(raw.parse().map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                6,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?),
                        None => None,
                    },
                    dismissed_at: match dismissed_at_raw {
                        Some(raw) => Some(raw.parse().map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                7,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?),
                        None => None,
                    },
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(thoughts)
    }

    pub fn mark_thought_surfaced(&self, id: &str) -> Result<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE pending_thoughts_queue
             SET surfaced_at = ?2
             WHERE id = ?1",
            params![id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn dismiss_thought(&self, id: &str) -> Result<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE pending_thoughts_queue
             SET dismissed_at = ?2
             WHERE id = ?1",
            params![id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    // ========================================================================
    // Private Chat - Operator <-> Agent Communication
    // ========================================================================

    /// Add a chat message
    pub fn add_chat_message(&self, role: &str, content: &str) -> Result<String> {
        self.add_chat_message_in_conversation(DEFAULT_CHAT_CONVERSATION_ID, role, content)
    }

    fn add_chat_message_internal(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
        turn_id: Option<&str>,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let conversation_id = if conversation_id.trim().is_empty() {
            DEFAULT_CHAT_CONVERSATION_ID
        } else {
            conversation_id.trim()
        };
        let now = Utc::now().to_rfc3339();
        let processed = if role.eq_ignore_ascii_case("operator") {
            0
        } else {
            1
        };
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT OR IGNORE INTO chat_sessions (id, label, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                DEFAULT_CHAT_SESSION_ID,
                "Default session",
                now.clone(),
                now.clone()
            ],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO chat_conversations (id, session_id, title, created_at, updated_at, runtime_state, active_turn_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
            params![
                conversation_id,
                DEFAULT_CHAT_SESSION_ID,
                "Conversation",
                now.clone(),
                now.clone(),
                ChatTurnPhase::Idle.as_db_str(),
            ],
        )?;
        conn.execute(
            "INSERT INTO chat_messages (id, conversation_id, role, content, created_at, processed, turn_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, conversation_id, role, content, now.clone(), processed, turn_id],
        )?;
        if role.eq_ignore_ascii_case("operator") {
            conn.execute(
                "UPDATE chat_conversations
                 SET runtime_state = ?2, active_turn_id = NULL, updated_at = ?3
                 WHERE id = ?1",
                params![conversation_id, ChatTurnPhase::Idle.as_db_str(), now],
            )?;
        } else {
            conn.execute(
                "UPDATE chat_conversations
                 SET updated_at = ?2
                 WHERE id = ?1",
                params![conversation_id, now],
            )?;
        }
        Ok(id)
    }

    /// Add a chat message to a specific conversation.
    pub fn add_chat_message_in_conversation(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
    ) -> Result<String> {
        self.add_chat_message_internal(conversation_id, role, content, None)
    }

    /// Add a chat message and attach it to a specific turn.
    pub fn add_chat_message_in_turn(
        &self,
        conversation_id: &str,
        turn_id: &str,
        role: &str,
        content: &str,
    ) -> Result<String> {
        self.add_chat_message_internal(conversation_id, role, content, Some(turn_id))
    }

    /// Create a new conversation and return it.
    pub fn create_chat_conversation(&self, title: Option<&str>) -> Result<ChatConversation> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        let title = title
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("Chat {}", now.format("%Y-%m-%d %H:%M")));

        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT OR IGNORE INTO chat_sessions (id, label, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                DEFAULT_CHAT_SESSION_ID,
                "Default session",
                now_str.clone(),
                now_str.clone()
            ],
        )?;
        conn.execute(
            "INSERT INTO chat_conversations (id, session_id, title, created_at, updated_at, runtime_state, active_turn_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
            params![
                id,
                DEFAULT_CHAT_SESSION_ID,
                title,
                now_str.clone(),
                now_str,
                ChatTurnPhase::Idle.as_db_str(),
            ],
        )?;

        Ok(ChatConversation {
            id,
            session_id: DEFAULT_CHAT_SESSION_ID.to_string(),
            title,
            created_at: now,
            updated_at: now,
            runtime_state: ChatTurnPhase::Idle,
            active_turn_id: None,
            message_count: 0,
            last_message_at: None,
        })
    }

    /// List recent conversations, newest activity first.
    pub fn list_chat_conversations(&self, limit: usize) -> Result<Vec<ChatConversation>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            r#"SELECT
                   c.id,
                   c.session_id,
                   c.title,
                   c.created_at,
                   c.updated_at,
                   c.runtime_state,
                   c.active_turn_id,
                   COUNT(m.id) as message_count,
                   MAX(m.created_at) as last_message_at
               FROM chat_conversations c
               LEFT JOIN chat_messages m ON m.conversation_id = c.id
               GROUP BY c.id
               ORDER BY COALESCE(MAX(m.created_at), c.updated_at) DESC
               LIMIT ?1"#,
        )?;

        let conversations = stmt
            .query_map([limit], |row| {
                let created_at_str: String = row.get(3)?;
                let updated_at_str: String = row.get(4)?;
                let runtime_state_raw: String = row.get(5)?;
                let active_turn_id: Option<String> = row.get(6)?;
                let message_count = row.get::<_, i64>(7)? as usize;
                let last_message_at_str: Option<String> = row.get(8)?;

                Ok(ChatConversation {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    title: row.get(2)?,
                    created_at: created_at_str.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            3,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    updated_at: updated_at_str.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    runtime_state: ChatTurnPhase::from_db(&runtime_state_raw),
                    active_turn_id,
                    message_count,
                    last_message_at: match last_message_at_str {
                        Some(v) => Some(v.parse().map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                8,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?),
                        None => None,
                    },
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(conversations)
    }

    /// Start a new persisted turn for a conversation.
    pub fn begin_chat_turn(
        &self,
        conversation_id: &str,
        trigger_message_ids: &[String],
        iteration: i64,
    ) -> Result<String> {
        let turn_id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let trigger_json =
            serde_json::to_string(trigger_message_ids).unwrap_or_else(|_| "[]".to_string());
        let conn = self.lock_conn()?;

        conn.execute(
            "INSERT OR IGNORE INTO chat_sessions (id, label, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                DEFAULT_CHAT_SESSION_ID,
                "Default session",
                now.clone(),
                now.clone()
            ],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO chat_conversations (id, session_id, title, created_at, updated_at, runtime_state, active_turn_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
            params![
                conversation_id,
                DEFAULT_CHAT_SESSION_ID,
                "Conversation",
                now.clone(),
                now.clone(),
                ChatTurnPhase::Idle.as_db_str(),
            ],
        )?;

        let session_id: String = conn.query_row(
            "SELECT session_id FROM chat_conversations WHERE id = ?1",
            [conversation_id],
            |row| row.get(0),
        )?;

        conn.execute(
            "INSERT INTO chat_turns (
                id, session_id, conversation_id, iteration, phase_state, decision, status,
                trigger_message_ids_json, operator_message, reason, error, tool_call_count,
                started_at, completed_at, agent_message_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, ?6, NULL, NULL, NULL, 0, ?7, NULL, NULL)",
            params![
                turn_id,
                session_id,
                conversation_id,
                iteration,
                ChatTurnPhase::Processing.as_db_str(),
                trigger_json,
                now.clone(),
            ],
        )?;

        conn.execute(
            "UPDATE chat_conversations
             SET runtime_state = ?2, active_turn_id = ?3, updated_at = ?4
             WHERE id = ?1",
            params![
                conversation_id,
                ChatTurnPhase::Processing.as_db_str(),
                turn_id,
                now
            ],
        )?;

        Ok(turn_id)
    }

    /// Record a single tool call made during a turn.
    pub fn record_chat_turn_tool_call(
        &self,
        turn_id: &str,
        call_index: usize,
        tool_name: &str,
        arguments_json: &str,
        output_text: &str,
    ) -> Result<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO chat_turn_tool_calls (id, turn_id, call_index, tool_name, arguments_json, output_text, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                uuid::Uuid::new_v4().to_string(),
                turn_id,
                call_index as i64,
                tool_name,
                arguments_json,
                output_text,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Complete a turn and set its terminal state.
    pub fn complete_chat_turn(
        &self,
        turn_id: &str,
        phase_state: ChatTurnPhase,
        decision: &str,
        status: &str,
        operator_message: &str,
        reason: Option<&str>,
        tool_call_count: usize,
        agent_message_id: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.lock_conn()?;

        conn.execute(
            "UPDATE chat_turns
             SET phase_state = ?2,
                 decision = ?3,
                 status = ?4,
                 operator_message = ?5,
                 reason = ?6,
                 error = NULL,
                 tool_call_count = ?7,
                 completed_at = ?8,
                 agent_message_id = ?9
             WHERE id = ?1",
            params![
                turn_id,
                phase_state.as_db_str(),
                decision,
                status,
                operator_message,
                reason,
                tool_call_count as i64,
                now.clone(),
                agent_message_id,
            ],
        )?;

        conn.execute(
            "UPDATE chat_conversations
             SET runtime_state = ?2, active_turn_id = NULL, updated_at = ?3
             WHERE id = (SELECT conversation_id FROM chat_turns WHERE id = ?1)",
            params![turn_id, phase_state.as_db_str(), now],
        )?;

        Ok(())
    }

    /// Mark a turn as failed and retain the error text for post-mortem.
    pub fn fail_chat_turn(&self, turn_id: &str, error: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.lock_conn()?;

        conn.execute(
            "UPDATE chat_turns
             SET phase_state = ?2,
                 error = ?3,
                 completed_at = ?4
             WHERE id = ?1",
            params![
                turn_id,
                ChatTurnPhase::Failed.as_db_str(),
                error,
                now.clone()
            ],
        )?;

        conn.execute(
            "UPDATE chat_conversations
             SET runtime_state = ?2, active_turn_id = NULL, updated_at = ?3
             WHERE id = (SELECT conversation_id FROM chat_turns WHERE id = ?1)",
            params![turn_id, ChatTurnPhase::Failed.as_db_str(), now],
        )?;

        Ok(())
    }

    /// List recent turns for one conversation (newest first).
    pub fn list_chat_turns_for_conversation(
        &self,
        conversation_id: &str,
        limit: usize,
    ) -> Result<Vec<ChatTurn>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT
                id, session_id, conversation_id, iteration, phase_state, decision, status,
                trigger_message_ids_json, operator_message, reason, error, tool_call_count,
                started_at, completed_at, agent_message_id
             FROM chat_turns
             WHERE conversation_id = ?1
             ORDER BY started_at DESC
             LIMIT ?2",
        )?;

        let turns = stmt
            .query_map(params![conversation_id, limit], |row| {
                let started_at_str: String = row.get(12)?;
                let completed_at_str: Option<String> = row.get(13)?;
                let phase_state_raw: String = row.get(4)?;
                let trigger_ids_raw: String = row.get(7)?;
                let trigger_message_ids = serde_json::from_str::<Vec<String>>(&trigger_ids_raw)
                    .unwrap_or_else(|_| Vec::new());

                Ok(ChatTurn {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    conversation_id: row.get(2)?,
                    iteration: row.get(3)?,
                    phase_state: ChatTurnPhase::from_db(&phase_state_raw),
                    decision: row.get(5)?,
                    status: row.get(6)?,
                    trigger_message_ids,
                    operator_message: row.get(8)?,
                    reason: row.get(9)?,
                    error: row.get(10)?,
                    tool_call_count: row.get::<_, i64>(11)? as usize,
                    started_at: started_at_str.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            12,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    completed_at: match completed_at_str {
                        Some(v) => Some(v.parse().map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                13,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?),
                        None => None,
                    },
                    agent_message_id: row.get(14)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(turns)
    }

    /// List tool calls for a specific turn.
    pub fn list_chat_turn_tool_calls(&self, turn_id: &str) -> Result<Vec<ChatTurnToolCall>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, turn_id, call_index, tool_name, arguments_json, output_text, created_at
             FROM chat_turn_tool_calls
             WHERE turn_id = ?1
             ORDER BY call_index ASC",
        )?;

        let calls = stmt
            .query_map([turn_id], |row| {
                let created_at_str: String = row.get(6)?;
                Ok(ChatTurnToolCall {
                    id: row.get(0)?,
                    turn_id: row.get(1)?,
                    call_index: row.get::<_, i64>(2)? as usize,
                    tool_name: row.get(3)?,
                    arguments_json: row.get(4)?,
                    output_text: row.get(5)?,
                    created_at: created_at_str.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            6,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(calls)
    }

    /// Get unprocessed messages from the operator
    pub fn get_unprocessed_operator_messages(&self) -> Result<Vec<ChatMessage>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, role, content, created_at, processed FROM chat_messages
             WHERE role = 'operator' AND processed = 0
             ORDER BY created_at ASC",
        )?;

        let messages = stmt
            .query_map([], |row| {
                Ok(ChatMessage {
                    id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    role: row.get(2)?,
                    content: row.get(3)?,
                    created_at: row.get::<_, String>(4)?.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    processed: row.get::<_, i64>(5)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(messages)
    }

    /// Mark a message as processed
    pub fn mark_message_processed(&self, id: &str) -> Result<()> {
        let conn = self.lock_conn()?;
        conn.execute("UPDATE chat_messages SET processed = 1 WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Get recent chat history (for context)
    pub fn get_chat_history(&self, limit: usize) -> Result<Vec<ChatMessage>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, role, content, created_at, processed FROM chat_messages
             ORDER BY created_at DESC
             LIMIT ?1",
        )?;

        let messages = stmt
            .query_map([limit], |row| {
                Ok(ChatMessage {
                    id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    role: row.get(2)?,
                    content: row.get(3)?,
                    created_at: row.get::<_, String>(4)?.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    processed: row.get::<_, i64>(5)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Reverse to get chronological order
        Ok(messages.into_iter().rev().collect())
    }

    /// Get recent chat history for one conversation (chronological order).
    pub fn get_chat_history_for_conversation(
        &self,
        conversation_id: &str,
        limit: usize,
    ) -> Result<Vec<ChatMessage>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, role, content, created_at, processed FROM chat_messages
             WHERE conversation_id = ?1
             ORDER BY created_at DESC
             LIMIT ?2",
        )?;

        let messages = stmt
            .query_map(params![conversation_id, limit], |row| {
                Ok(ChatMessage {
                    id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    role: row.get(2)?,
                    content: row.get(3)?,
                    created_at: row.get::<_, String>(4)?.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    processed: row.get::<_, i64>(5)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Reverse to get chronological order
        Ok(messages.into_iter().rev().collect())
    }

    /// Get a chronological message window using offset from latest messages.
    ///
    /// `offset_from_latest = 0` means the newest `limit` messages.
    /// `offset_from_latest = 20` skips the most recent 20 messages, then returns
    /// the next `limit` older messages.
    pub fn get_chat_history_slice_for_conversation(
        &self,
        conversation_id: &str,
        offset_from_latest: usize,
        limit: usize,
    ) -> Result<Vec<ChatMessage>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, role, content, created_at, processed FROM chat_messages
             WHERE conversation_id = ?1
             ORDER BY created_at DESC
             LIMIT ?2 OFFSET ?3",
        )?;

        let messages = stmt
            .query_map(params![conversation_id, limit, offset_from_latest], |row| {
                Ok(ChatMessage {
                    id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    role: row.get(2)?,
                    content: row.get(3)?,
                    created_at: row.get::<_, String>(4)?.parse().map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    processed: row.get::<_, i64>(5)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(messages.into_iter().rev().collect())
    }

    /// Count messages in one conversation.
    pub fn count_chat_messages_for_conversation(&self, conversation_id: &str) -> Result<usize> {
        let conn = self.lock_conn()?;
        let count = conn.query_row(
            "SELECT COUNT(1) FROM chat_messages WHERE conversation_id = ?1",
            [conversation_id],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count.max(0) as usize)
    }

    /// Upsert a compacted summary snapshot for a conversation.
    pub fn upsert_chat_conversation_summary(
        &self,
        conversation_id: &str,
        summary_text: &str,
        summarized_message_count: usize,
    ) -> Result<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO chat_conversation_summaries (conversation_id, summary_text, summarized_message_count, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(conversation_id) DO UPDATE SET
                summary_text = excluded.summary_text,
                summarized_message_count = excluded.summarized_message_count,
                updated_at = excluded.updated_at",
            params![
                conversation_id,
                summary_text,
                summarized_message_count as i64,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Fetch compacted summary snapshot for one conversation.
    pub fn get_chat_conversation_summary(
        &self,
        conversation_id: &str,
    ) -> Result<Option<ChatConversationSummary>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT conversation_id, summary_text, summarized_message_count, updated_at
             FROM chat_conversation_summaries
             WHERE conversation_id = ?1",
        )?;
        let mut rows = stmt.query([conversation_id])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };

        let updated_at_raw: String = row.get(3)?;
        let updated_at = updated_at_raw.parse().map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
        })?;

        Ok(Some(ChatConversationSummary {
            conversation_id: row.get(0)?,
            summary_text: row.get(1)?,
            summarized_message_count: row.get::<_, i64>(2)?.max(0) as usize,
            updated_at,
        }))
    }

    /// Get chat history as formatted string for context
    pub fn get_chat_context(&self, limit: usize) -> Result<String> {
        let messages = self.get_chat_history(limit)?;
        Self::format_chat_context(messages)
    }

    /// Get chat history as formatted string for one conversation.
    pub fn get_chat_context_for_conversation(
        &self,
        conversation_id: &str,
        limit: usize,
    ) -> Result<String> {
        let messages = self.get_chat_history_for_conversation(conversation_id, limit)?;
        Self::format_chat_context(messages)
    }

    fn format_chat_context(messages: Vec<ChatMessage>) -> Result<String> {
        if messages.is_empty() {
            return Ok(String::new());
        }

        let mut context = String::from("## Recent Private Chat with Operator\n\n");
        for msg in messages {
            let role_display = if msg.role == "operator" {
                "Operator"
            } else {
                "You"
            };
            context.push_str(&format!("**{}**: {}\n\n", role_display, msg.content));
        }
        Ok(context)
    }
}

fn outcome_to_db(outcome: &PromotionOutcome) -> &'static str {
    match outcome {
        PromotionOutcome::Promote => "promote",
        PromotionOutcome::Hold => "hold",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_db_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("ponderer_{}_{}.db", name, uuid::Uuid::new_v4()));
        path
    }

    #[test]
    fn chat_turn_lifecycle_persists_state_and_tool_calls() {
        let path = temp_db_path("chat_turn_lifecycle");
        let db = AgentDatabase::new(&path).expect("db init");

        let conversation = db
            .create_chat_conversation(Some("Lifecycle test"))
            .expect("create conversation");
        let operator_message_id = db
            .add_chat_message_in_conversation(&conversation.id, "operator", "Please investigate.")
            .expect("insert operator message");

        let turn_id = db
            .begin_chat_turn(
                &conversation.id,
                std::slice::from_ref(&operator_message_id),
                1,
            )
            .expect("begin turn");

        db.record_chat_turn_tool_call(
            &turn_id,
            0,
            "list_directory",
            r#"{"path":"."}"#,
            "Found 3 entries",
        )
        .expect("record tool call");

        let agent_message_id = db
            .add_chat_message_in_turn(&conversation.id, &turn_id, "agent", "Done.")
            .expect("insert agent turn message");

        db.mark_message_processed(&operator_message_id)
            .expect("mark processed");

        db.complete_chat_turn(
            &turn_id,
            ChatTurnPhase::Completed,
            "yield",
            "done",
            "Done.",
            Some("Completed investigation"),
            1,
            Some(&agent_message_id),
        )
        .expect("complete turn");

        let turns = db
            .list_chat_turns_for_conversation(&conversation.id, 10)
            .expect("list turns");
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].id, turn_id);
        assert_eq!(turns[0].phase_state, ChatTurnPhase::Completed);
        assert_eq!(turns[0].tool_call_count, 1);
        assert_eq!(
            turns[0].agent_message_id.as_deref(),
            Some(agent_message_id.as_str())
        );
        assert_eq!(
            turns[0].trigger_message_ids,
            vec![operator_message_id.clone()]
        );

        let tool_calls = db
            .list_chat_turn_tool_calls(&turn_id)
            .expect("list tool calls");
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].tool_name, "list_directory");

        let conversations = db.list_chat_conversations(50).expect("list conversations");
        let convo_state = conversations
            .iter()
            .find(|c| c.id == conversation.id)
            .expect("find conversation");
        assert_eq!(convo_state.runtime_state, ChatTurnPhase::Completed);
        assert_eq!(convo_state.active_turn_id, None);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn failed_turn_keeps_operator_message_unprocessed_for_retry() {
        let path = temp_db_path("chat_turn_failure");
        let db = AgentDatabase::new(&path).expect("db init");

        let conversation = db
            .create_chat_conversation(Some("Failure test"))
            .expect("create conversation");
        let operator_message_id = db
            .add_chat_message_in_conversation(&conversation.id, "operator", "Try and fail")
            .expect("insert operator message");

        let turn_id = db
            .begin_chat_turn(
                &conversation.id,
                std::slice::from_ref(&operator_message_id),
                1,
            )
            .expect("begin turn");
        db.fail_chat_turn(&turn_id, "tool timeout")
            .expect("fail turn");

        let unprocessed = db
            .get_unprocessed_operator_messages()
            .expect("query unprocessed messages");
        assert!(unprocessed.iter().any(|m| m.id == operator_message_id));

        let conversations = db.list_chat_conversations(50).expect("list conversations");
        let convo_state = conversations
            .iter()
            .find(|c| c.id == conversation.id)
            .expect("find conversation");
        assert_eq!(convo_state.runtime_state, ChatTurnPhase::Failed);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn memory_search_returns_relevant_entries() {
        let path = temp_db_path("memory_search");
        let db = AgentDatabase::new(&path).expect("db init");

        db.set_working_memory("project-goal", "build desktop companion agent")
            .expect("seed memory");
        db.set_working_memory("music", "buy new synth")
            .expect("seed memory");

        let results = db
            .search_working_memory("desktop agent", 5)
            .expect("search memory");
        assert!(!results.is_empty());
        assert_eq!(results[0].key, "project-goal");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn append_daily_activity_log_accumulates_lines() {
        let path = temp_db_path("daily_activity_log");
        let db = AgentDatabase::new(&path).expect("db init");

        db.append_daily_activity_log("Ran memory search tool")
            .expect("append first");
        db.append_daily_activity_log("Answered operator request")
            .expect("append second");

        let today_key = format!("activity-log-{}", Utc::now().format("%Y-%m-%d"));
        let item = db
            .get_working_memory(&today_key)
            .expect("get memory")
            .expect("daily log exists");
        assert!(item.content.contains("Ran memory search tool"));
        assert!(item.content.contains("Answered operator request"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn chat_conversation_summary_roundtrip_and_history_slice() {
        let path = temp_db_path("chat_summary_roundtrip");
        let db = AgentDatabase::new(&path).expect("db init");

        let conversation = db
            .create_chat_conversation(Some("Summary test"))
            .expect("create conversation");

        for idx in 0..8 {
            let role = if idx % 2 == 0 { "operator" } else { "agent" };
            db.add_chat_message_in_conversation(
                &conversation.id,
                role,
                &format!("message {}", idx),
            )
            .expect("insert chat message");
        }

        let total = db
            .count_chat_messages_for_conversation(&conversation.id)
            .expect("count conversation messages");
        assert_eq!(total, 8);

        let slice = db
            .get_chat_history_slice_for_conversation(&conversation.id, 2, 3)
            .expect("slice chat messages");
        assert_eq!(slice.len(), 3);
        assert!(slice[0].content.contains("message 3"));
        assert!(slice[2].content.contains("message 5"));

        db.upsert_chat_conversation_summary(&conversation.id, "Older context summary", 5)
            .expect("upsert summary");
        let summary = db
            .get_chat_conversation_summary(&conversation.id)
            .expect("get summary")
            .expect("summary exists");
        assert_eq!(summary.summarized_message_count, 5);
        assert!(summary.summary_text.contains("Older context summary"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn journal_entry_roundtrip_search_and_context() {
        let path = temp_db_path("journal_roundtrip");
        let db = AgentDatabase::new(&path).expect("db init");

        let entry = JournalEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            entry_type: JournalEntryType::Reflection,
            content: "I noticed Max is iterating faster on calibration today.".to_string(),
            context: JournalContext {
                trigger: "ambient_orientation".to_string(),
                user_state_at_time: "deep_work".to_string(),
                time_of_day: "afternoon".to_string(),
            },
            related_concerns: vec!["thermal-array".to_string()],
            mood_at_time: Some(JournalMood {
                valence: 0.3,
                arousal: 0.6,
            }),
        };

        db.add_journal_entry(&entry).expect("save journal entry");

        let recent = db.get_recent_journal(5).expect("get recent journal");
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].id, entry.id);
        assert_eq!(recent[0].entry_type, JournalEntryType::Reflection);
        assert_eq!(
            recent[0].related_concerns,
            vec!["thermal-array".to_string()]
        );

        let found = db.search_journal("calibration", 5).expect("search journal");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, entry.id);

        let context = db
            .get_journal_for_context(64)
            .expect("journal context string");
        assert!(context.contains("Recent Journal Notes"));
        assert!(context.contains("calibration"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn concerns_roundtrip_touch_and_salience_update() {
        let path = temp_db_path("concerns_roundtrip");
        let db = AgentDatabase::new(&path).expect("db init");

        let now = Utc::now();
        let concern = Concern {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: now,
            last_touched: now,
            summary: "Thermal array calibration workflow".to_string(),
            concern_type: ConcernType::CollaborativeProject {
                project_name: "thermal-array".to_string(),
                my_role: "observer".to_string(),
            },
            salience: Salience::Active,
            my_thoughts: "Track iteration speed and toolchain friction.".to_string(),
            related_memory_keys: vec!["activity-log".to_string()],
            context: ConcernContext {
                how_it_started: "operator conversation".to_string(),
                key_events: vec!["initial planning".to_string()],
                last_update_reason: "created".to_string(),
            },
        };

        db.save_concern(&concern).expect("save concern");

        let loaded = db
            .get_concern(&concern.id)
            .expect("get concern")
            .expect("concern exists");
        assert_eq!(loaded.summary, concern.summary);
        assert_eq!(loaded.salience, Salience::Active);

        let active = db.get_active_concerns().expect("active concerns");
        assert!(active.iter().any(|c| c.id == concern.id));

        db.touch_concern(&concern.id, "checked during ll.1 test")
            .expect("touch concern");
        let touched = db
            .get_concern(&concern.id)
            .expect("get touched concern")
            .expect("touched concern exists");
        assert_eq!(
            touched.context.last_update_reason,
            "checked during ll.1 test"
        );

        db.update_concern_salience(&concern.id, Salience::Dormant)
            .expect("demote concern");
        let active_after = db.get_active_concerns().expect("active after demote");
        assert!(!active_after.iter().any(|c| c.id == concern.id));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn orientation_snapshot_and_pending_thought_queue_roundtrip() {
        let path = temp_db_path("orientation_pending");
        let db = AgentDatabase::new(&path).expect("db init");

        let snapshot = OrientationSnapshotRecord {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            user_state: serde_json::json!({"type":"deep_work","activity":"coding"}),
            disposition: "observe".to_string(),
            synthesis: "Max is focused and stable.".to_string(),
            salience_map: serde_json::json!([{"summary":"Code task","relevance":0.8}]),
            anomalies: serde_json::json!([]),
            pending_thoughts: serde_json::json!([{"content":"Mention GPU temp drift"}]),
            mood_valence: Some(0.2),
            mood_arousal: Some(0.5),
        };
        db.save_orientation_snapshot(&snapshot)
            .expect("save orientation snapshot");

        let recent = db.get_recent_orientations(3).expect("recent orientations");
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].id, snapshot.id);
        assert_eq!(recent[0].disposition, "observe");

        let thought_a = PendingThoughtRecord {
            id: uuid::Uuid::new_v4().to_string(),
            content: "Ask whether to run a thermal check.".to_string(),
            context: Some("orientation.surface".to_string()),
            priority: 0.9,
            relates_to: vec!["thermal-array".to_string()],
            created_at: Utc::now(),
            surfaced_at: None,
            dismissed_at: None,
        };
        let thought_b = PendingThoughtRecord {
            id: uuid::Uuid::new_v4().to_string(),
            content: "Share calibration trend if asked.".to_string(),
            context: Some("orientation.surface".to_string()),
            priority: 0.4,
            relates_to: vec![],
            created_at: Utc::now(),
            surfaced_at: None,
            dismissed_at: None,
        };
        db.queue_pending_thought(&thought_a)
            .expect("queue thought a");
        db.queue_pending_thought(&thought_b)
            .expect("queue thought b");

        let unsurfaced = db
            .get_unsurfaced_thoughts()
            .expect("get unsurfaced thoughts");
        assert_eq!(unsurfaced.len(), 2);
        assert_eq!(unsurfaced[0].id, thought_a.id);

        db.mark_thought_surfaced(&thought_a.id)
            .expect("mark surfaced");
        db.dismiss_thought(&thought_b.id).expect("dismiss thought");
        let remaining = db
            .get_unsurfaced_thoughts()
            .expect("remaining unsurfaced thoughts");
        assert!(remaining.is_empty());

        let _ = std::fs::remove_file(&path);
    }
}
