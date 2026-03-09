# mod.rs

## Purpose
Module declaration file for the `ui` crate. Re-exports all UI submodules that compose the Ponderer desktop GUI.

## Components

### Module declarations
- **`app`**: Main application struct implementing `eframe::App`
- **`avatar`**: Avatar loading and animated GIF playback
- **`chat`**: Event log and private chat rendering
- **`sprite`**: Agent visual state rendering (avatar or emoji fallback)
- **`settings`**: Tabbed settings window for core config plus skill-specific tabs
- **`plugin_settings_form`**: Generic schema-driven renderer for plugin-defined settings fields
- **`character`**: Character card import and editing panel
- **`comfy_settings`**: ComfyUI workflow import and configuration panel
- **`orbweaver_settings`**: OrbWeaver / Graphchan configuration panel

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `app.rs` | All submodules declared here | Removing any module breaks `AgentApp` |
| `main.rs` / lib root | `pub mod ui` exposes `app::AgentApp` | Renaming `app` module breaks app entry point |

## Notes
No logic lives here -- purely module wiring.
