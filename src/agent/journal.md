# journal.rs

## Purpose
Defines the foundational journal data model for the Living Loop. These are private, timestamped inner-life entries that can later be generated and read by orientation/dream logic.

## Components

### `JournalEntry`
- **Does**: Represents one journal note with type, text, context, related concerns, and optional mood values
- **Interacts with**: `database.rs` journal CRUD methods and future `JournalEngine`

### `JournalEntryType`
- **Does**: Enumerates journal note categories and provides DB string conversion helpers (`as_db_str`, `from_db`)
- **Interacts with**: SQLite persistence in `database.rs`

### `JournalContext`
- **Does**: Carries generation context such as trigger, estimated user state, and time-of-day label
- **Interacts with**: Future orientation/journal prompt templates

### `JournalMood`
- **Does**: Stores lightweight affect values captured with an entry (`valence`, `arousal`)
- **Interacts with**: Orientation synthesis and trend analysis

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `database.rs` | Stable `JournalEntryType` DB string mappings | Renaming enum variants or conversion outputs |
| Future journal/orientation engines | `JournalEntry` fields are serializable and timestamped in UTC | Removing fields used for context continuity |

## Notes
- This file intentionally contains only types for ll.1 foundation work; behavior is added in later phases.
