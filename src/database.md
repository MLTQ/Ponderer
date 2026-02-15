# database.rs

## Purpose
Provides the agent's persistent memory layer via SQLite. Stores important posts, reflection history, persona snapshots (for personality evolution tracking), private chat sessions/conversations/turns/messages, per-turn tool call lineage, Living Loop foundation records (journal, concerns, orientation snapshots, pending thoughts), and key-value state. Working-memory CRUD routes through a versioned `MemoryBackend` abstraction while preserving the existing KV behavior (`kv_v1`).

## Components

### `AgentDatabase`
- **Does**: Wraps a `Mutex<rusqlite::Connection>`, delegates working-memory CRUD to `MemoryBackend`, and provides typed CRUD methods for all tables
- **Interacts with**: `main.rs` (instantiated), `agent::Agent` (reads/writes memories), `ui::app::AgentApp` (displays data), `memory::KvMemoryBackend` (default backend), `memory::MemoryMigrationRegistry` (upgrade scaffolding)

### `ImportantPost`
- **Does**: Records a forum post the agent found significant, with `importance_score` (0.0-1.0) and `why_important` explanation
- **Interacts with**: `agent::reasoning` (marks posts), UI persona/memory views

### `PersonaSnapshot`
- **Does**: Captures the agent's personality state at a point in time, including dynamic `PersonaTraits` dimensions, system prompt, trigger, self-description, and inferred trajectory
- **Rationale**: Core data structure for "Ludonarrative Assonantic Tracing" -- tracking personality evolution over time

### `PersonaTraits`
- **Does**: Flexible `HashMap<String, f64>` mapping dimension names to 0.0-1.0 scores
- **Rationale**: Avoids fixed personality models; agents define their own axes via `guiding_principles` or LLM self-reflection

### `ReflectionRecord`
- **Does**: Logs each self-reflection event with old/new system prompts, reasoning, and guiding principles

### `WorkingMemoryEntry`
- **Does**: Key-value scratchpad entries the agent can read/write between sessions
- **Interacts with**: `memory::MemoryBackend` contract, `get_working_memory_context()` formats all entries for LLM context injection

### Working-memory search/log methods (`search_working_memory`, `append_daily_activity_log`)
- **Does**: Performs ranked text search over persisted memory entries and appends timestamped daily activity lines into date-keyed memory notes
- **Interacts with**: `tools::memory::{search_memory, write_memory}`, `agent::process_chat_messages` automatic conversation logging

### `get_memory_design_version` / `set_memory_design_version`
- **Does**: Persists and reads active memory design metadata from `agent_state` (`memory_design_id`, `memory_schema_version`)
- **Interacts with**: `memory::MemoryDesignVersion`, startup initialization in `AgentDatabase::new`

### `apply_memory_migration`
- **Does**: Applies a direct migration via `MemoryMigrationRegistry` and updates persisted memory design metadata
- **Interacts with**: Future scheduled upgrade flow (ALMA-lite memory evolution loop)

### Memory Archive Methods (`save_memory_design_archive_entry`, `save_memory_eval_run`, `save_memory_promotion_decision`, `evaluate_and_record_memory_promotion`)
- **Does**: Persists memory design versions, eval reports, and policy decisions into dedicated archive tables
- **Interacts with**: `memory::archive` policy evaluator + record types, `memory::eval::MemoryEvalReport`
- **Rationale**: Makes promotion outcomes auditable/reproducible and ensures rollback target is always persisted

### `ChatMessage`
- **Does**: Private operator-agent chat messages with `conversation_id` routing, `processed` unread tracking, and optional `turn_id` linkage for agent replies
- **Interacts with**: UI chat panel, `agent::Agent` poll loop

### `ChatTurnPhase`
- **Does**: Encodes persisted lifecycle states shared across conversations and turns (`idle`, `processing`, `completed`, `awaiting_approval`, `failed`)
- **Interacts with**: `chat_conversations.runtime_state`, `chat_turns.phase_state`, `agent::process_chat_messages`

### `ChatSession`
- **Does**: Top-level container grouping conversation threads for long-running desktop usage
- **Interacts with**: `chat_conversations.session_id`, default bootstrap row (`default_session`)

### `ChatConversation`
- **Does**: Conversation-level metadata (`id`, `session_id`, `title`, timestamps, runtime state, active turn pointer, message stats) used by the UI for multi-chat selection and status display
- **Interacts with**: `ui::app` conversation picker/status label, `chat_messages` and `chat_turns` via `conversation_id`

### `ChatConversationSummary`
- **Does**: Stores compacted long-context snapshot text per conversation plus coverage count (`summarized_message_count`) and update timestamp
- **Interacts with**: `agent::process_chat_messages` summary refresh/compaction prompt injection

### `ChatTurn` / `ChatTurnToolCall`
- **Does**: Persist one agent turn with decision/status/error context and per-tool input/output records for replay/debug
- **Interacts with**: `agent::process_chat_messages`, future turn history/undo/resume UX

### `OrientationSnapshotRecord` / `PendingThoughtRecord`
- **Does**: Typed records for stored orientation snapshots and queued pending-thought items
- **Interacts with**: Future ambient/orientation loop surfaces and LL debugging views

### Chat lifecycle methods (`begin_chat_turn`, `record_chat_turn_tool_call`, `complete_chat_turn`, `fail_chat_turn`, `list_chat_turns_for_conversation`, `list_chat_turn_tool_calls`)
- **Does**: Implements persisted turn state transitions and tool-call lineage for each conversation thread
- **Interacts with**: `agent::process_chat_messages` autonomous turn loop, diagnostics/recovery tooling

### Chat conversation methods (`create_chat_conversation`, `list_chat_conversations`, `add_chat_message_in_conversation`, `add_chat_message_in_turn`, `get_chat_history_for_conversation`, `get_chat_context_for_conversation`)
- **Does**: Creates/lists conversation threads, writes messages (optionally bound to a turn), and returns thread-scoped history/context
- **Interacts with**: `ui::app::AgentApp` new-chat/switch-chat actions, `agent::process_chat_messages` per-conversation prompt building

### Chat compaction methods (`count_chat_messages_for_conversation`, `get_chat_history_slice_for_conversation`, `upsert_chat_conversation_summary`, `get_chat_conversation_summary`)
- **Does**: Supports long-session context compaction by counting messages, loading older windows, and persisting summary snapshots
- **Interacts with**: `agent::maybe_refresh_conversation_compaction_summary` before private-chat tool loop turns

### Living Loop foundation methods
- **Does**: Adds additive CRUD for:
  - Journal (`add_journal_entry`, `get_recent_journal`, `get_journal_for_context`, `search_journal`)
  - Concerns (`save_concern`, `get_concern`, `get_active_concerns`, `get_all_concerns`, `update_concern_salience`, `touch_concern`)
  - Orientation snapshots (`save_orientation_snapshot`, `get_recent_orientations`)
  - Pending thought queue (`queue_pending_thought`, `get_unsurfaced_thoughts`, `mark_thought_surfaced`, `dismiss_thought`)
- **Interacts with**: `agent::{journal, concerns}` type models and future Living Loop ambient/disposition logic

### `CharacterCard` (DB model)
- **Does**: Stores imported character card metadata (format, raw data, derived prompt)
- **Rationale**: Only one card kept at a time (previous deleted on import)

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `main.rs` | `AgentDatabase::new(path)` creates/opens DB, ensures schema, and initializes memory design metadata | Changing startup initialization or metadata keys |
| `agent::Agent` | Chat APIs include turn lifecycle methods plus conversation-scoped message/context methods (`begin_chat_turn`, `record_chat_turn_tool_call`, `complete_chat_turn`, `fail_chat_turn`, `add_chat_message_in_turn`) | Removing/renaming lifecycle methods or changing state semantics |
| `ui::app` | `ChatConversation` includes `runtime_state`; conversation APIs remain (`list_chat_conversations`, `create_chat_conversation`, `get_chat_history_for_conversation`, `add_chat_message_in_conversation`) | Removing runtime state fields or changing chat query/write signatures |
| `memory::mod` | `MemoryBackend` trait and `MemoryDesignVersion` metadata keys remain stable | Changing backend trait signatures or metadata semantics |
| `memory::archive` | Archive methods serialize/deserialize policy + metrics snapshots without loss | Changing JSON field contracts for policy/snapshot structs |
| `agent::{journal, concerns}` | DB CRUD accepts/returns these typed records | Breaking type-field compatibility or db string mappings |

## Notes
- All timestamps stored as RFC 3339 strings in SQLite, parsed back to `chrono::DateTime<Utc>`.
- `ensure_schema()` uses `CREATE TABLE IF NOT EXISTS` -- no formal migration system. Adding columns requires manual ALTER TABLE handling.
- `ensure_schema()` performs manual chat migrations by checking `PRAGMA table_info(...)` and adding missing columns (`conversation_id`, `turn_id`, `session_id`, `runtime_state`, `active_turn_id`) in place.
- Conversation compaction snapshots are stored in `chat_conversation_summaries` and updated opportunistically by the agent loop when message-count thresholds are exceeded.
- Living Loop ll.1 adds four additive tables: `journal_entries`, `concerns`, `orientation_snapshots`, `pending_thoughts_queue`.
- Memory design metadata is stored in `agent_state` under `memory_design_id` and `memory_schema_version`.
- Memory evolution archive uses three tables: `memory_design_archive`, `memory_eval_runs`, `memory_promotion_decisions`.
- `memory_promotion_decisions` enforces rollback fields (`rollback_design_id`, `rollback_schema_version`) as NOT NULL.
- `save_character_card` deletes all existing cards before inserting (singleton pattern).
- The `agent_state` table is a generic key-value store used for `current_system_prompt` and `last_reflection_time`.
- Default chat bootstrap rows are auto-created for both session (`default_session`) and conversation (`default`) compatibility.
