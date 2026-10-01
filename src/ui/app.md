# app.rs

## Purpose

`AgentApp` owns the native frontend's API client, event intake, backend status,
chat history/media, live streams/tool output, continuity summaries, approvals,
shared settings/identity draft and token trace state. `workbench.rs` renders its
workspace shell; `theme.rs` applies the operator's appearance draft.

## Runtime and data

`new` starts websocket intake, fetches configuration/plugin manifests and loads
status, conversations, history and schedules. REST refresh remains every two
seconds for consistency. `update` processes events, advances the existing Affect
Lab async controller, renders the workbench and dispatches ordinary API actions.
No daemon, background desktop process, or model-loading ownership is added here.

Generation lifecycle/metric events feed independent retained traces rather than
the event tape. Typing invokes configured clear-on-interaction behavior. State,
orientation, intention, journal and action summaries come from actual backend
events/status; model-reported material is labeled, not represented as experience.

## Actions and persistence

Chat create/send/rename/delete, pause/resume, turn stop, exact prompt inspection
and media rendering retain their existing API contracts. Prompt inspection uses
the base-derived palette and explicit source labels instead of fixed rainbow
section colors. Destructive chat actions retain confirmation dialogs.

`persist_config` saves through the backend, replaces the saved baseline only on
success and reloads avatars. Provider transitions block saves. Failures retain
drafts, surface errors and drop queued schedule mutations (editors can retry).
Provider-only sync preserves unrelated drafts. Identity does not regenerate a
custom system prompt without the operator's explicit checkbox.

Approvals remain in persistent top chrome across navigation. They are removed
only after successful API approval or explicit local dismissal. Loose arming
still requires confirmation; the header offers one-click disarm.

## Tests and contracts

`main.rs` still uses `AgentApp::new(ApiClient, AgentConfig)`. UI subpanels expose
`render_contents` for embedded workspaces; Settings owns the shared draft/baseline.
The default app never constructs synthetic fixtures.

`isolated_snapshot` and `render_snapshot` are compiled only for tests or the
opt-in `ui-snapshot` feature. They use a loopback port-1 placeholder, no event
stream, no backend polling, no model loading and visibly labeled synthetic data.
The `models` fixture previews local GPU selection with explicitly synthetic
inventory and no probe or load.
Headless navigation/layout tests and the real eframe screenshot example use this
factory without affecting live configuration. See [workbench](workbench.md).
