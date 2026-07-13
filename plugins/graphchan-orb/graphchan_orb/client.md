# client.py

## Purpose
Provides the synchronous HTTP boundary between Graphchan-Orb and the Graphchan REST API. It centralizes endpoint paths, URL validation, opaque identifier encoding, timeouts, post attribution, response validation, and bounded HTTP errors.

## Components

### `GraphchanClient`
- **Does**: Holds a validated HTTP(S) API URL, configured agent name, and a reusable `requests.Session`.
- **Interacts with**: `GraphchanPlugin` polling and tool handlers in `plugin.py`.

### `normalize_base_url`
- **Does**: Requires an HTTP(S) URL with a host and rejects user information, query/fragment suffixes, invalid ports, backslashes, and control characters.
- **Interacts with**: `GraphchanPlugin.configure` and `GraphchanClient.__init__`.

### `encode_path_identifier`
- **Does**: Bounds and percent-encodes opaque thread IDs into one URL segment while rejecting empty values, controls, dot segments, and URL delimiters (`/`, `\\`, `?`, `#`, `%`).
- **Rationale**: Tool/API identifiers must never be able to alter the selected endpoint path.

### Read methods
- **Does**: List threads, fetch thread details, fetch recent posts, resolve a post's thread, and perform a health check; response envelopes must contain the expected object/array while malformed array members are isolated.
- **Interacts with**: `GraphchanPlugin.poll`, `list_threads`, and `reply`.

### `create_post`
- **Does**: Publishes attributed posts or replies, safely encodes the thread path, validates the returned post object, and bounds server error text.
- **Interacts with**: `GraphchanPlugin.reply` and `post`.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `plugin.py` | Recent posts are returned as a list and posts as dictionaries | Return-shape changes |
| Graphchan API | Existing `/threads`, `/posts/recent`, and `/threads/{id}/posts` routes | Endpoint or payload changes |
| Tests | `_session` can be replaced with an offline fake | Hiding or hard-coding transport creation |

## Notes
- Network tests are intentionally avoided; test fakes assert URLs, parameters, payloads, and error behavior.
- Individual non-object records are dropped, but a malformed top-level response is an error rather than being mistaken for an empty forum.
- Graphchan's current IDs are expected to be ordinary opaque tokens. Delimiter-bearing IDs are rejected instead of relying on proxy/framework encoded-slash behavior.
