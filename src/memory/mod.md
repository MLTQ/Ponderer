# mod.rs

## Purpose
Defines the versioned memory backend contract for Ponderer and provides the baseline `kv_v1` implementation that preserves current working-memory behavior. Also includes migration-registry scaffolding for future memory design upgrades.

## Components

### `archive` (submodule)
- **Does**: Memory design archive types and promotion policy evaluator with deterministic gate logic
- **Interacts with**: `eval` report types and `AgentDatabase` archive tables

### `candidate_backends` (submodule)
- **Does**: First candidate memory designs (`fts_v2`, `episodic_v3`) implementing `MemoryBackend`
- **Interacts with**: `eval` candidate builder for shadow comparisons against `kv_v1`

### `eval` (submodule)
- **Does**: Offline memory replay/evaluation harness with deterministic scoring and JSON reports
- **Interacts with**: `MemoryBackend` trait for candidate comparison; future scheduler/archive tasks in ALMA-lite roadmap

### `MemoryDesignVersion`
- **Does**: Identifies the active memory design (`design_id`) and schema version (`schema_version`)
- **Interacts with**: `AgentDatabase` in `../database.rs` for persisted metadata in `agent_state`

### `WorkingMemoryEntry`
- **Does**: Typed working-memory row (`key`, `content`, `updated_at`)
- **Interacts with**: `AgentDatabase` public memory APIs

### `MemoryBackend`
- **Does**: Defines the memory backend interface (`set/get/list/delete`) plus `design_version()`
- **Interacts with**: `AgentDatabase` delegates all working-memory CRUD through this trait
- **Rationale**: Keeps memory API stable while enabling backend evolution (KV -> FTS -> episodic)

### `KvMemoryBackend`
- **Does**: Baseline backend that reads/writes the existing `working_memory` table
- **Interacts with**: SQLite `Connection` provided by `AgentDatabase`

### `MemoryMigration` / `MemoryMigrationRegistry`
- **Does**: Registers and applies direct memory-design migrations
- **Interacts with**: `AgentDatabase::apply_memory_migration`
- **Rationale**: Scaffolding for controlled, auditable schema/design upgrades

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `../database.rs` | `MemoryBackend` methods are synchronous over `rusqlite::Connection` | Changing trait signatures |
| `../database.rs` | `MemoryDesignVersion::kv_v1()` is valid default | Renaming/removing default constructor |
| `eval.rs` | `candidate_backends` exports stable `fts_v2` and `episodic_v3` trait implementations | Renaming/removing candidate backend types |
| Future migration runner | `MemoryMigrationRegistry::apply_direct` errors when no path exists | Auto-fallback behavior change |

## Notes
- Current registry supports direct migrations only; path planning across multiple hops is intentionally deferred.
- `KvMemoryBackend` is behavior-preserving and intended as the stable baseline for eval comparisons.
