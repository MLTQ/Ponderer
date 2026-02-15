# concerns.rs

## Purpose
Defines the foundational concern-tracking data model for the Living Loop. Concerns represent long-lived topics/projects the agent keeps track of across interactions.

## Components

### `Concern`
- **Does**: Represents one tracked concern with timestamps, salience, typed category, private notes, and linked memory keys
- **Interacts with**: `database.rs` concern CRUD methods and future `ConcernsManager`

### `ConcernType`
- **Does**: Encodes concern domains (project, household, system health, interest, reminder, conversation)
- **Interacts with**: JSON persistence in SQLite and future concern-update logic

### `Salience`
- **Does**: Priority tier for attention budgeting and includes DB mapping helpers (`as_db_str`, `from_db`)
- **Interacts with**: `database.rs` filtering (`get_active_concerns`) and future decay/pruning

### `ConcernContext`
- **Does**: Captures origin and historical update context for a concern
- **Interacts with**: future concern-lifecycle updates and debug introspection

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `database.rs` | Stable `Salience` DB string mappings and serializable `ConcernType` | Renaming variants or changing serde tagging |
| Future concerns manager | `Concern` includes created/last-touched timestamps plus context fields | Removing lifecycle fields used for salience/decay |

## Notes
- This ll.1 file intentionally focuses on typed records only; behavior (creation heuristics/decay/consolidation) lands in later phases.
