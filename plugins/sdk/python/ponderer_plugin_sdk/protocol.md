# protocol.py

## Purpose

Owns protocol-v1 negotiation plus the newline-delimited request and response
envelopes. It is deliberately independent of plugin domain behavior.

## Components

### `PROTOCOL_V1` / `SUPPORTED_PROTOCOL_VERSIONS`
- **Does**: Defines the versions this SDK can execute.
- **Interacts with**: handshake dispatch in `server.py`.

### `RpcFault`
- **Does**: Carries a stable machine-readable error code and safe message.
- **Interacts with**: `PluginServer.handle_line` in `server.py`.

### `RpcRequest` / `RpcResponse`
- **Does**: Represent validated, correlated RPC envelopes.
- **Rationale**: Legacy requests omit `protocol_version` and are interpreted as v1;
  responses always include it and remain safe for the old host to ignore.

### `decode_request_line` / `encode_response_line`
- **Does**: Own one-line JSON framing, validation, and newline termination.
- **Policy**: Rejects non-standard numeric constants such as `NaN` and refuses
  to serialize non-JSON plugin results instead of emitting invalid JSON.
- **Interacts with**: production stdio and `FakeHost` in `testing.py`.

### `negotiate_protocol`
- **Does**: Selects the highest common version from
  `supported_protocol_versions`; an empty legacy handshake selects v1.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `server.py` | Decode failures are `RpcFault`; encoded responses end in one newline | Envelope fields or exception contract |
| Ponderer host | Missing request version means v1; every SDK response declares v1 | Default version or field names |
| Plugin tests | Error codes distinguish malformed input and unsupported versions | Error-code changes |

## Notes

Python's JSON module accepts non-finite numbers by default. This boundary opts
into strict JSON in both directions so Rust and Python hosts see the same wire
language.
