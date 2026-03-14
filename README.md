# Ponderer
Ponderer is a pedagogical project to better understand and facilitate agentic systems. 
Ponderer is not a coding agent. Ponderer is a buddy. 
I believe in Universal Basic Digimon, and this is a first cut at such a system. 
It is meant to chat with you. Take actions. Have personal desires and thoughts. Take actions on its own behalf.
It isn't supposed to be a tool. Its supposed to be your little buddy.

## Plugin System

Ponderer supports optional capabilities through installable plugin bundles.  
Plugins are loaded from disk at startup and can add:

- tools the agent can call
- skill guidance (`SKILL.md`)
- their own settings tab in the UI

### Skills vs Plugins

- **Skill**: agent guidance/instructions (usually markdown), no runtime required.
- **Plugin**: installable capability bundle that can add settings, tools, and runtime behavior.

Most advanced integrations (like Voice-Orb / qwen3-TTS) are runtime plugins and may also ship a skill file.

### Where Plugins Live (Portable)

By default, Ponderer looks in a local `plugins/` folder next to `ponderer_config.toml` and `ponderer_memory.db` (portable install behavior).  
If missing, Ponderer creates this directory automatically at startup.

You can override the location with:

```bash
export PONDERER_PLUGIN_DIR="/absolute/path/to/plugins"
```

### Plugin Types

Ponderer currently supports two filesystem plugin bundle types:

1. **`runtime_process`**  
   A subprocess plugin (for example Python) launched by Ponderer over JSON-RPC stdio.
2. **`comfy_workflow`**  
   A data-only ComfyUI workflow bundle (`workflow.json` + bindings + schema).

### Runtime Plugin Bundle Format (`plugin_type = "runtime_process"`)

Example layout:

```text
plugins/voice-orb/
  plugin.toml
  settings.schema.json
  SKILL.md                  (optional but recommended)
  scripts/run_plugin.sh
  ...
```

Minimal `plugin.toml`:

```toml
id = "voice-orb"
name = "Voice-Orb"
version = "0.1.0"
description = "Portable Qwen3-TTS runtime plugin for Ponderer."
plugin_type = "runtime_process"
command = ["./scripts/run_plugin.sh"]
settings_schema_file = "settings.schema.json"   # optional if default name used
settings_tab_title = "Voice-Orb"                # optional
settings_tab_order = 320                        # optional
```

Runtime process methods expected by Ponderer:

- `plugin.handshake`
- `plugin.configure`
- `plugin.handle_event`
- `plugin.get_prompt_contributions`
- `plugin.invoke_tool`

Transport is JSON-RPC style over newline-delimited stdio.

### Comfy Workflow Bundle Format (`plugin_type = "comfy_workflow"`)

Example layout:

```text
plugins/my-workflow/
  plugin.toml
  settings.schema.json
  workflow.json
  bindings.json
  SKILL.md                  (optional)
```

This type is data-only and runs through Ponderer’s built-in ComfyUI transport.

### Settings Tab Schema

Plugins can declare a settings UI via `settings.schema.json`.  
The desktop app renders it dynamically (no frontend recompile required).

Supported field kinds:

- `boolean`
- `text`
- `multiline`
- `number`
- `select`
- `path`
- `secret`

For runtime plugins, include an `enabled` boolean (recommended) so users can enable/disable the plugin from its tab.

### Install a Plugin

1. Put the plugin folder under Ponderer’s runtime-local `plugins/` directory.
2. If the plugin has an installer script (like Voice-Orb), run it in that plugin directory.
3. Start/restart Ponderer.
4. Open Settings, go to the plugin’s tab, set `Enabled`, and save.

Voice-Orb development helper:

```bash
./scripts/install_to_ponderer.sh /path/to/Ponderer
```

This links the repo into `plugins/voice-orb` and runs its portable installer.

### Build Guidance for Plugin Authors

For robust plugins:

1. Keep everything self-contained under the plugin directory (`.venv`, models/cache, outputs, state) for portability.
2. Avoid writing protocol data except JSON responses to stdout (log to stderr).
3. Keep handshake lightweight; lazily import heavy dependencies.
4. Provide a clear settings schema with safe defaults.
5. Expose a narrow, typed tool surface instead of a generic shell interface.

## Telegram Bot Setup

Ponderer has a built-in Telegram bot that lets you message the agent from your phone. It uses a dedicated conversation separate from the desktop UI.

### 1. Create a bot with BotFather

1. Open Telegram and search for **@BotFather**
2. Send `/newbot` and follow the prompts (pick a name and username)
3. BotFather will give you a **bot token** — save it

### 2. Find your chat ID

1. Start a conversation with your new bot (send any message)
2. Open this URL in a browser, replacing `<TOKEN>` with your token:
   ```
   https://api.telegram.org/bot<TOKEN>/getUpdates
   ```
3. Look for `"chat":{"id":...}` in the response — that number is your **chat ID**

### 3. Configure environment variables

Set these before starting Ponderer:

```bash
export TELEGRAM_BOT_TOKEN="<your-bot-token>"
export TELEGRAM_CHAT_ID="<your-chat-id>"   # optional but recommended
```

`TELEGRAM_CHAT_ID` restricts the bot to your account only. If omitted, anyone who messages the bot can talk to the agent.

### 4. Start Ponderer

No extra steps — the bot starts automatically when `TELEGRAM_BOT_TOKEN` is set. You should see a log line:

```
Telegram bot active (allowed_chat_id: Some(<id>))
```

Messages you send to the bot are routed into a conversation named `"telegram"` and replies come back as Telegram messages.

---
