# mod.rs

## Purpose
Defines ll.1 foundation types and a stub `PresenceMonitor` for the Living Loop. It provides a stable data contract (`PresenceState`, `TimeContext`, `SystemLoad`) without introducing behavior changes yet.

## Components

### `PresenceMonitor`
- **Does**: Tracks session start/last interaction and produces stubbed presence samples
- **Interacts with**: Future ambient loop orchestration in `agent/mod.rs`
- **Rationale**: ll.1 lands schemas/types first; platform-specific signal collection is added later

### `PresenceState`
- **Does**: Snapshot of user/system state with idle/session durations, local time context, load, and active process list
- **Interacts with**: Future orientation synthesis inputs

### `TimeContext::now`
- **Does**: Derives coarse temporal flags (weekend, late-night, deep-night, work-hours) from local clock
- **Interacts with**: Future rhythm/disposition logic

### `SystemLoad` / `InterestingProcess` / `ProcessCategory`
- **Does**: Typed envelope for resource/process signals, currently left as stub values
- **Interacts with**: Future platform-specific samplers

### `duration_seconds` (private serde helper)
- **Does**: Serializes `std::time::Duration` as seconds for JSON compatibility
- **Interacts with**: `PresenceState` serde derives

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| Future orientation engine | `PresenceState` fields stay stable and serializable | Renaming/removing core fields |
| Future ambient loop | `PresenceMonitor::sample()` is cheap and non-blocking | Adding blocking/system-heavy work in ll.1 |

## Notes
- This module intentionally avoids platform APIs (`IOKit`, `/proc`, `nvidia-smi`) during ll.1.
- CPU/memory/GPU/process values are placeholders until ll.2.
