# Ponderer
Ponderer is a pedagogical project to better understand and facilitate agentic systems. 
Ponderer is not a coding agent. Ponderer is a buddy. 
I believe in Universal Basic Digimon, and this is a first cut at such a system. 
It is meant to chat with you. Take actions. Have personal desires and thoughts. Take actions on its own behalf.
It isn't supposed to be a tool. Its supposed to be your little buddy.

## Plugin System

Ponderer treats optional capabilities as versioned plugin packages. A plugin can
add:

- typed tools with declared semantic effects;
- polled observations and lifecycle-event handlers;
- bounded prompt contributions;
- a schema-driven settings tab.

There is one package model. “Orb” is a friendly product name for a curated
plugin, not a second runtime or protocol.

### Contract and SDK

`plugin.toml` declares manifest version 1, protocol version 1, identity,
contributions, requested capabilities, and semantic effects. Native plugins use
newline-delimited protocol-v1 RPC over stdio. Python plugins should depend on the
shared SDK in `plugins/sdk/python`; it owns framing, negotiation, dispatch,
typed results, and reusable conformance tests.

Native subprocesses are trusted execution, not a sandbox. The host owns process
supervision, approval minimums, outward-action quotas, durable event recording,
and namespaced plugin state.

New packages must declare `manifest_version = 1`, `protocol_version = 1`, and
`[contributions]` together. Missing fields do not opt a package into legacy
authority. A temporary host-compiled compatibility list admits only the bundled
`browser-orb`, `image-orb`, and `voice-orb` package slots until they are migrated;
plugin authors cannot extend that list from `plugin.toml`.

### Package Locations

Ponderer discovers active development/portable packages in `plugins/` next to
`ponderer_config.toml`. Override it with:


```bash
export PONDERER_PLUGIN_DIR="/absolute/path/to/plugins"
```

Model-authored drafts live in `plugin-workbench/`. Validated versions are staged
immutably under `plugins/store/<id>/<version>/` with `enabled=false`; staging
never executes or activates code.

### Runtime Package Format

Example layout:

```text
plugins/example-plugin/
  plugin.toml
  tools.json
  settings.schema.json
  example_plugin/
    server.py
  tests/
```

Minimal manifest:

```toml
manifest_version = 1
protocol_version = 1
id = "example-plugin"
name = "Example Plugin"
version = "0.1.0"
description = "Adds an example read-only capability."
plugin_type = "runtime_process"
command = ["python3", "-m", "example_plugin.server"]
requested_capabilities = ["network.read"]
tool_contract_file = "tools.json"

[contributions]
event_hooks = []
prompt_slots = []
poll_events = false

[[declared_effects]]
id = "network.read"
requires_approval = false
```

`tools.json` contains `{ "tools": [...] }` using the same typed tool manifests
returned by the SDK handshake. For strict v1 packages, schemas and effects must
match exactly; see `plugins/graphchan-orb/tools.json` for a complete example.

### Settings Tab Schema

Plugins can declare `settings.schema.json`; the desktop renders it dynamically
without integration-specific Rust UI code. Supported field kinds are
`boolean`, `text`, `multiline`, `number`, `select`, `path`, and `secret`.

`secret` masks the desktop control but is not a credential vault in protocol
v1; its value still lives in ordinary plugin configuration. Credential-bearing
plugins should wait for or integrate with host-managed secret handles.

An `enabled` boolean that defaults to `false` is strongly recommended for native
packages.

### Self-Directed Authoring

The built-in `plugin_workbench` tool lets the model create a Python SDK scaffold,
read/write only within that draft, validate it, and stage an immutable disabled
package. It intentionally has no execute, grant, or activate action. Expanding
authority, accessing secrets/sensors, or enabling native code remains a separate
operator decision until a genuine sandbox adapter exists.

### Install a Plugin

1. Put a trusted package directly under the active `plugins/` directory.
2. Install its runtime dependencies using the package's installer.
3. Open its generated settings tab, review settings/authority, enable it, and
   save.

### Build Guidance for Plugin Authors

1. Use the shared SDK instead of copying an RPC loop.
2. Keep stdout protocol-only and send diagnostics to stderr.
3. Keep handshakes lightweight and lazily import large models.
4. Request the narrowest capabilities and declare effects for every tool.
5. Inherit the SDK conformance suite and add domain-specific offline tests.
6. Keep mutable data outside immutable package source when possible.

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
