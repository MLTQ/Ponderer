# theme.rs

One operator-selected sRGB base derives the native workbench's background, panels,
raised controls, scope, borders, text, muted text, accent, selection/chat fill,
wireframe and trace endpoint. Warning/error hues stay recognizable amber/red.

`Palette::from_config` mixes the base with dark/light neutral surfaces, then
adjusts foregrounds toward white/black until they exceed 4.7:1 contrast against
the worst of the derived surfaces. Tests cover extreme saturated colors and
black/white/gray in both modes, requiring at least 4.5:1 for normal foregrounds.
Thin sphere lines intentionally use dimmer colors and are not text.

`apply` caches the last appearance per egui context to avoid invalidating styles
and font caches on every repaint. Squared controls and technical heading/button
typography are native egui styling. `palette(ui)` supplies the same colors to
custom-painted chat, prompt inspectors and the sphere.

Persistence belongs to the existing backend configuration path; no model state,
web runtime, or additional process is introduced.
