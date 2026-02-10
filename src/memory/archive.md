# archive.rs

## Purpose
Defines the memory design archive and promotion policy logic for ALMA-lite. Turns eval reports into explicit `promote`/`hold` decisions with deterministic gate checks and an always-recorded rollback target.

## Components

### `MemoryDesignArchiveEntry`
- **Does**: Stores a versioned memory design record plus optional description/metadata
- **Interacts with**: `AgentDatabase` archive persistence methods

### `MemoryEvalRunRecord`
- **Does**: Wraps a `MemoryEvalReport` with run metadata and stable run ID
- **Interacts with**: `AgentDatabase` eval-run archive table

### `MemoryPromotionPolicy`
- **Does**: Defines promotion gates (recall gains, pass-rate floor, latency ratio, non-regression rule)
- **Interacts with**: `evaluate_promotion_policy`

### `PromotionMetricsSnapshot`
- **Does**: Captures baseline/candidate metrics and computed deltas used for policy checks
- **Interacts with**: `MemoryPromotionDecisionRecord` for reproducible decisions

### `MemoryPromotionDecisionRecord`
- **Does**: Persistable promotion decision artifact with rationale, policy, metrics snapshot, and rollback target
- **Interacts with**: `AgentDatabase` promotion-decision table

### `evaluate_promotion_policy(...)`
- **Does**: Evaluates candidate vs baseline using policy gates and emits a deterministic decision record
- **Interacts with**: `MemoryEvalReport` in `eval.rs`, persisted decision archive in `database.rs`

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `database.rs` | Decision includes `rollback_target` on every path | Making rollback optional |
| Future scheduler (`cpf.1.4`) | Same report + policy => same decision outcome and deltas | Non-deterministic gate logic |
| Future promotion UI | `PromotionOutcome` values `promote|hold` | Renaming enum variants |

## Notes
- Policy evaluation is pure and deterministic given report + policy + current design.
- Decision `id`/`created_at` are runtime metadata and do not affect outcome reproducibility.
