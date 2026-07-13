# test_client.py

## Purpose
Exercises the Graphchan HTTP client boundary using deterministic fake responses and sessions.

## Components

### `FakeResponse` / `FakeSession`
- **Does**: Record request shape and return queued payloads without opening network connections.
- **Interacts with**: `GraphchanClient._session` test seam.

### `GraphchanClientTests`
- **Does**: Covers recent-post unwrapping, URLs/timeouts, post attribution, API errors, and thread resolution.
- **Interacts with**: `graphchan_orb.client.GraphchanClient`.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `server.py` | Client return shapes remain stable | Wrapper or dictionary changes |
| Graphchan API | Request paths and post metadata stay compatible | Endpoint/payload changes |

## Notes
- Tests never use a real `requests.Session` after client construction.
