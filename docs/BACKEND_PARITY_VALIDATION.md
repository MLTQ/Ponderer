# Backend Standalone Parity Validation

This document tracks backend-only validation for the split architecture (`ponderer_backend` running without frontend runtime coupling).

## Automated smoke gate

Run:

```bash
./scripts/validate_backend_standalone.sh
```

What it verifies:

1. Backend starts standalone (`cargo run --manifest-path ponderer_backend/Cargo.toml --bin ponderer_backend`)
2. Auth boundary: unauthenticated `/v1/health` returns `401`
3. Auth boundary: authenticated `/v1/health` returns `{"status":"ok"}`
4. Conversations list endpoint works
5. Conversation creation endpoint works
6. Operator message enqueue endpoint works
7. Conversation history endpoint persists/retrieves message
8. Agent status endpoint returns runtime state
9. Plugin manifest endpoint returns `builtin.core`

## Validation matrix

| Capability | Method | Status | Notes |
|-----------|--------|--------|------|
| Standalone process boot | Automated script | Pass | Backend starts and accepts API traffic |
| Token auth boundaries | Automated script | Pass | Deny-by-default confirmed (`401` unauth) |
| Config API | Manual (curl/UI) | Pending | Covered in frontend cutover path; add explicit script check if desired |
| Chat CRUD | Automated script | Pass | List/create/send/history validated |
| WS event stream | Frontend integration + runtime behavior | Pass (integration) | API client uses `/v1/ws/events` with reconnect; exercised during UI runs |
| Agent loop runtime status | Automated script | Pass | `/v1/agent/status` validated |
| Background subtask progression | Manual scenario | Pending external | Requires model-backed long-running prompt |
| Media generation/publish | Manual scenario | Pending external | Requires an enabled media plugin and its external model/runtime |
| Vision/screenshot/camera tools | Manual scenario | Pending external | Requires host permissions + model/tool availability |
| Memory persistence/evolution | Existing backend tests + manual scenario | Pass (tests) / Pending (manual) | DB/test coverage exists; runtime behavior depends on model availability |

## Current known environment limitation

In this environment, default model `llama3.2` is unavailable (`404`), so autonomous-loop capability checks that require successful LLM calls cannot be fully validated here without overriding backend config to an available model.

## Recommended full-stack local verification

1. Set model endpoint/token in backend config to a reachable provider.
2. Start backend with required auth token.
3. Connect frontend client and run:
   - normal chat turn
   - multi-turn tool call turn
   - background subtask turn
   - media generation/publish turn (if a media plugin is configured)
4. Confirm activity stream + chat rendering + persisted history across restart.
