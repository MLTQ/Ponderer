# test_client.py

## Purpose
Exercises the Graphchan HTTP client boundary using deterministic fake responses and sessions.

## Components

### `FakeResponse` / `FakeSession`
- **Does**: Record request shape and return queued payloads without opening network connections.
- **Interacts with**: `GraphchanClient._session` test seam.

### `GraphchanClientTests`
- **Does**: Covers base-URL rejection, single-segment identifier encoding, recent-post validation, URLs/timeouts, post attribution, bounded API errors, and thread resolution.
- **Interacts with**: `graphchan_orb.client.GraphchanClient`.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `plugin.py` | Client return shapes remain stable | Wrapper or dictionary changes |
| Graphchan API | Request paths, identifier escaping, and post metadata stay compatible | Endpoint/payload changes |

## Notes
- Tests never use a real `requests.Session` after client construction.
- Non-delimiter reserved characters are percent-encoded; empty, control, dot-segment, and URL-delimiter IDs are rejected before transport.
