# client.py

## Purpose
Provides the synchronous HTTP boundary between Graphchan-Orb and the Graphchan REST API. It centralizes endpoint paths, timeouts, post attribution, response unwrapping, and HTTP error handling.

## Components

### `GraphchanClient`
- **Does**: Holds normalized API URL, configured agent name, and a reusable `requests.Session`.
- **Interacts with**: all server polling and tool handlers.

### Read methods
- **Does**: List threads, fetch thread details, fetch recent posts, resolve a post's thread, and perform a health check.
- **Interacts with**: `server._poll_events`, `server._tool_list_threads`, `server._tool_reply`.

### `create_post`
- **Does**: Publishes attributed posts or replies and unwraps the returned post object.
- **Interacts with**: `server._tool_reply`, `server._tool_post`.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `server.py` | Recent posts are returned as a list and posts as dictionaries | Return-shape changes |
| Graphchan API | Existing `/threads`, `/posts/recent`, and `/threads/{id}/posts` routes | Endpoint or payload changes |
| Tests | `_session` can be replaced with an offline fake | Hiding or hard-coding transport creation |

## Notes
- Network tests are intentionally avoided; test fakes assert URLs, parameters, payloads, and error behavior.
