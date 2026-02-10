# candidate_backends.rs

## Purpose
Implements the first ALMA-lite candidate memory designs behind the `MemoryBackend` trait: `fts_v2` and `episodic_v3`. Both preserve core CRUD semantics while changing storage/indexing strategies for shadow evaluation against `kv_v1`.

## Components

### `FtsMemoryBackendV2`
- **Does**: Stores canonical rows in `working_memory_fts_docs` and mirrors content into `working_memory_fts_index` (FTS5)
- **Interacts with**: `MemoryBackend` trait, shadow-eval candidates in `eval.rs`
- **Rationale**: Introduces full-text indexing while preserving key-value API surface

### `EpisodicMemoryBackendV3`
- **Does**: Models writes as append-only episodes in `working_memory_episodes`, with `active` row pointers for current state
- **Interacts with**: `MemoryBackend` trait, shadow-eval candidates in `eval.rs`
- **Rationale**: Keeps historical memory evolution while maintaining current-state reads

### `ensure_fts_tables` / `ensure_episodic_tables`
- **Does**: Lazily initializes backend-specific tables/indexes on first use
- **Interacts with**: All backend CRUD operations

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `memory/eval.rs` | Both candidate backends satisfy `MemoryBackend` core APIs | Changing method behavior/signatures |
| Future migration tasks | Design versions are `fts_v2:2` and `episodic_v3:3` | Renaming design IDs or schema versions |
| Existing agent memory flow | CRUD behavior remains compatible with current expectations | Returning stale/duplicate active rows |

## Notes
- FTS index is maintained in parallel for future search APIs; current evaluation still uses `list_entries` ranking logic.
- Episodic backend preserves historical rows; delete marks episodes inactive rather than hard delete.
