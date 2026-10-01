# character.rs

The workspace heading is Identity. Boundary/principle labels describe the fields
without decorative slash-separated subtitles; boundary enforcement is unchanged.

## Purpose
Implements the embedded Identity workspace: agent/operator names, relationship, fixed boundaries, guiding principles, optional PNG character cards, execution-state artwork and prompt preview. It shares the Settings configuration draft.

## Components

### `CharacterPanel`
- **Does**: Holds a mutable `AgentConfig` copy, visibility flag, cached avatar texture, and import error state
- **Interacts with**: `AgentConfig` from `crate::config`, `crate::character_card::parse_character_card`

### `render_mood_avatar_row(ui, label, value)`
- **Does**: Renders one editable mood-avatar path row with `Browse` and `Clear` controls for PNG/JPG/JPEG/GIF files.
- **Interacts with**: `rfd::FileDialog`, `AgentConfig` avatar fields.

### `CharacterPanel::new(config)`
- **Does**: Constructs the panel with default hidden state and no cached texture

### `CharacterPanel::render_contents(ui) -> Option<AgentConfig>`
- **Does**: Draws the embedded identity editor with:
  - **Avatar & Import section**: Shows avatar thumbnail (128x128), browse button using `rfd::FileDialog` for PNG files, drag-and-drop support
  - **Character Details**: Editable fields for name, description, personality, scenario, example dialogue
  - **Mood Avatars (UI States)**: Editable per-state paths (`avatar_idle`, `avatar_thinking`, `avatar_active`) with browse/clear controls
  - **System Prompt Preview**: Collapsible preview of the assembled prompt
  - **Action buttons**: Save identity & configuration, Clear Character
- Returns `Some(config)` on save. The existing system prompt is preserved unless the operator explicitly checks “Rebuild system prompt from character fields on save.” Settings' Revert drafts also restores the shared identity draft.
- **Interacts with**: `rfd::FileDialog`, `image` crate for avatar display, `egui::Context::input` for drag-and-drop

### `CharacterPanel::import_character_card(path)`
- **Does**: Parses a PNG character card via `crate::character_card::parse_character_card`, populates config fields (name, description, personality, scenario, example_dialogue, avatar_path), clears cached texture
- **Interacts with**: `crate::character_card::parse_character_card`

### `CharacterPanel::build_system_prompt() -> String`
- **Does**: Assembles a system prompt string from character fields, joining non-empty sections with double newlines. Falls back to a generic prompt if name is empty.

### `CharacterPanel::build_system_prompt_preview() -> String`
- **Does**: Delegates to `build_system_prompt` (exists to separate borrow from render closure)

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `workbench.rs` / `app.rs` | `render_contents()` returns `Option<AgentConfig>`; the shared draft is synchronized before and after render; backend persistence reports errors | Changing return type or losing unsaved fields breaks save flow |
| `AgentConfig` | Fields: `character_name`, `character_description`, `character_personality`, `character_scenario`, `character_example_dialogue`, `character_avatar_path`, `avatar_idle`, `avatar_thinking`, `avatar_active`, `system_prompt` | Renaming any field breaks this panel |
| `crate::character_card` | `parse_character_card(&Path) -> Result<(ParsedCard, format, raw)>` | Changing parse API breaks import |

## Notes
- Prompt preview is built before the mutable editor closure. Drag-and-drop runs afterwards using `ctx.input().raw.dropped_files`.
