# main.rs

## Purpose
Application entry point for Ponderer. Initializes logging, loads configuration, builds skills, creates the agent, spawns its background loop, and launches the egui desktop UI.

## Components

### `main()`
- **Does**: Orchestrates startup: logging -> config -> skills -> database -> agent -> UI
- **Interacts with**: `config::AgentConfig`, `database::AgentDatabase`, `memory` module (compiled for backend abstractions), `agent::Agent`, `skills::Skill`, `skills::graphchan::GraphchanSkill`, `ui::app::AgentApp`
- **Rationale**: Single-threaded main launches the agent loop on a separate thread with its own Tokio runtime, keeping the UI on the main thread (required by eframe/egui)

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `agent::Agent` | `Vec<Box<dyn Skill>>`, `AgentConfig`, `flume::Sender` | Changing `Agent::new` signature |
| `ui::app::AgentApp` | `flume::Receiver`, `Arc<Agent>`, `AgentConfig`, `Option<Arc<AgentDatabase>>` | Changing `AgentApp::new` signature |
| `skills::graphchan::GraphchanSkill` | Non-empty `config.graphchan_api_url` to be instantiated | Removing the `graphchan_api_url` config field |
| `database::AgentDatabase` | Valid `config.database_path` (SQLite file) | Changing `AgentDatabase::new` return type |

## Notes
- The agent runs on a dedicated OS thread with its own Tokio runtime; the main thread is reserved for the eframe event loop.
- `flume::unbounded()` channel bridges async agent events to the synchronous UI.
- The UI database and agent database share the same SQLite file via WAL mode for concurrent access.
- Memory backend logic is initialized through `AgentDatabase` (currently `kv_v1` default behavior).
- If the database fails to open, the UI still launches (with `ui_database = None`).
