# character.rs

## Purpose
Implements the Character Card panel, allowing users to import character cards from PNG files (with embedded metadata), edit character fields, configure mood-state avatar image paths, preview the generated system prompt, and save the character to the agent config.

## Components

### `CharacterPanel`
- **Does**: Holds a mutable `AgentConfig` copy, visibility flag, cached avatar texture, and import error state
- **Interacts with**: `AgentConfig` from `crate::config`, `crate::character_card::parse_character_card`

### `render_mood_avatar_row(ui, label, value)`
- **Does**: Renders one editable mood-avatar path row with `Browse` and `Clear` controls for PNG/JPG/JPEG/GIF files.
- **Interacts with**: `rfd::FileDialog`, `AgentConfig` avatar fields.

### `CharacterPanel::new(config)`
- **Does**: Constructs the panel with default hidden state and no cached texture

### `CharacterPanel::render(ctx) -> Option<AgentConfig>`
- **Does**: Draws the character card window with:
  - **Avatar & Import section**: Shows avatar thumbnail (128x128), browse button using `rfd::FileDialog` for PNG files, drag-and-drop support
  - **Character Details**: Editable fields for name, description, personality, scenario, example dialogue
  - **Mood Avatars (UI States)**: Editable per-state paths (`avatar_idle`, `avatar_thinking`, `avatar_active`) with browse/clear controls
  - **System Prompt Preview**: Collapsible preview of the assembled prompt
  - **Action buttons**: Save, Clear, Cancel
- Returns `Some(config)` on save (after updating `system_prompt` from character fields), `None` otherwise.
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
| `app.rs` | `render()` returns `Option<AgentConfig>`; on save, `app.rs` persists config and reloads agent | Changing return type breaks save flow |
| `AgentConfig` | Fields: `character_name`, `character_description`, `character_personality`, `character_scenario`, `character_example_dialogue`, `character_avatar_path`, `avatar_idle`, `avatar_thinking`, `avatar_active`, `system_prompt` | Renaming any field breaks this panel |
| `crate::character_card` | `parse_character_card(&Path) -> Result<(ParsedCard, format, raw)>` | Changing parse API breaks import |

## Notes
- The `build_system_prompt_preview` method exists solely to work around Rust borrow checker limitations -- it must be called before the mutable `egui::Window` closure.
- Drag-and-drop handling runs after the window closure, processing `ctx.input().raw.dropped_files`.
