# workbench.rs

Native Rust/egui application shell privately included by `app.rs`.

- Conversation keeps the existing chat, media, exact-prompt inspector and bounded live-tool output.
- Mind presents actual continuity, orientation, reflection and recorded activity.
- Identity edits the shared configuration draft and preserves a custom system prompt unless regeneration is explicitly selected.
- Affect lab embeds existing mixer/library/evidence/discovery controls.
- Settings embeds Models, Appearance, Connections, autonomy, memory, budgets, prompt, schedules and manifest-discovered plugin tabs.

The right instrument rail starts with the novelty sphere, independent of the
selected workspace. Selected-token logprob/entropy and lexical fallback provenance
are explicit. Approvals occupy persistent top chrome, never the hideable rail or
event tape. Successful API approval is required before removing an approval.
Permissions retains deliberate Loose arming and one-click disarm. The bottom
event tape resizes/collapses independently. Closing the UI still owns shutdown.
The shell uses single-purpose headings (Novelty, Journal, Mind, Identity, Settings)
and shows actual runtime/provider state without slogans or repeated ownership
labels. Composer keyboard help and palette explanations use tooltips. Removing
those labels does not alter shutdown, approvals or metric provenance.

Settings and Identity share one draft. Provider-only changes update provider
fields without erasing appearance/identity edits. Backend save failures remain
visible, preserve configuration drafts and prevent queued schedule mutations.

Headless tests exercise native navigation, palette presets/revert, shared drafts,
all five workspaces at 840×640 and 1240×900, approval/composer visibility and finite
mesh geometry and compact composer visibility during extreme tool output. `examples/ui_snapshot.rs`, gated by `ui-snapshot`, renders the same
shell in eframe using clearly labeled synthetic fixtures, no backend polling or
model loading, writes one PNG and exits. CLI: destination, workspace, six-digit
base hex, dark/light, width, height. Workspace choices: conversation, mind,
identity, lab, settings, appearance, models. It refuses to overwrite an existing PNG.
