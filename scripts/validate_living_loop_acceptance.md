# validate_living_loop_acceptance.sh

## Purpose
Runs a deterministic living-loop acceptance baseline in standalone backend mode. It verifies that ambient-loop orientation is active and persisted without requiring external model dependencies.

## What it validates

- Starts `ponderer_backend` headless (auth disabled for WS capture)
- Uses local `mock_openai_server.py` as the LLM provider
- Enables ambient/journal/concern/dream loop settings in a temp config
- Captures `/v1/ws/events`, waits for WS `open`, then forces deterministic traffic by toggling pause state (`POST /v1/agent/toggle-pause`) twice
- Requires websocket event type:
  - `state_changed`
- Validates persisted DB artifacts:
  - `orientation_snapshots >= 1`
  - reports `concerns` and `journal_entries` counts for visibility
  - DB path is resolved robustly: temp-workdir path first, then backend-reported path from logs
- Confirms backend loaded the temp config and reached the mock LLM

## Usage

```bash
./scripts/validate_living_loop_acceptance.sh
```

Optional environment overrides:
- `MOCK_OPENAI_PORT` (default `19091`)
- `PONDERER_BACKEND_BIND` (default `127.0.0.1:8880`)

## Notes

- This is an acceptance baseline, not a full behavioral quality evaluation.
- In restricted sandboxes that disallow local port binding, this script will fail before backend startup; run it on host/dev machine for full validation.
- On WS capture failures, the script now prints both `ws.log` and backend log tail to make root-cause diagnosis immediate.
- It complements:
  - `scripts/validate_backend_standalone.sh` (REST/auth smoke)
  - `scripts/validate_backend_parity_mock.sh` (background-subtask parity)
