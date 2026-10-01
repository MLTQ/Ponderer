# mod.rs

## Purpose
Module declaration file for the `ui` crate. Re-exports all UI submodules that compose the Ponderer desktop GUI.

## Components

### Module declarations
- **`app`**: Main application struct implementing `eframe::App`
- **`avatar`**: Avatar loading and animated GIF playback
- **`chat`**: Event log and private chat rendering
- **`sprite`**: Compact agent avatar or technical `P_` fallback
- **`settings`**: Embedded settings workspace plus schema-driven plugin tabs
- **`plugin_settings_form`**: Generic schema-driven renderer for plugin-defined settings fields
- **`character`**: Character card import and editing panel
- **`token_monitor`**: Live wireframe sphere renderer for token novelty traces
- **`theme`**: Base-color-derived native dark/light palette with contrast correction
- **`affect_lab`**: Embedded experiment panes and shared asynchronous model controller
- `app` privately includes `workbench.rs` for navigation, instrument rail, approvals and event tape.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `app.rs` | All submodules declared here | Removing any module breaks `AgentApp` |
| `main.rs` / lib root | `pub mod ui` exposes `app::AgentApp` | Renaming `app` module breaks app entry point |

## Notes
No logic lives here -- purely module wiring.
