# server.py

## Purpose
Implements the `Browser-Orb` stdio JSON-RPC server for Ponderer. It wraps the external `agent-browser` CLI, translates plugin settings into safe command defaults, and exposes browser automation tools to the normal Ponderer loop.

## Components

### `PluginState`
- **Does**: Stores persisted settings, a plugin instance id, the live agent-browser session id, and the last known URL.
- **Interacts with**: all RPC methods and agent-browser command execution.

### `main` / `handle_rpc_line` / `dispatch`
- **Does**: Runs the newline-delimited JSON-RPC loop and routes methods (`plugin.handshake`, `plugin.configure`, `plugin.handle_event`, `plugin.get_prompt_contributions`, `plugin.invoke_tool`).
- **Interacts with**: Ponderer runtime plugin host.

### `handshake`
- **Does**: Declares Browser-Orb metadata and the LLM-facing tool manifests (`browser_open`, `browser_snapshot`, `browser_click`, `browser_fill`, `browser_wait`, `browser_get_text`, `browser_screenshot`, `browser_close`, `browser_eval`).
- **Interacts with**: runtime tool-proxy registration and approval policy.

### `get_prompt_contributions`
- **Does**: Adds engaged-loop guidance describing the recommended browser workflow plus current safety settings (navigation restrictions, eval availability, persistent auth state).
- **Interacts with**: prompt slot contributions in `runtime_plugin_host.rs`.

### `run_agent_browser`
- **Does**: Builds a single `agent-browser --json ...` command, injects the live session env vars, applies optional domain restrictions, and parses the CLI JSON output.
- **Interacts with**: every Browser-Orb tool implementation.
- **Rationale**: Keeps the plugin itself thin and lets `agent-browser` own browser lifecycle/session behavior.

### Navigation helpers (`validate_navigation_target`, `parse_domain_allowlist`, `is_local_or_private_host`)
- **Does**: Enforces Browser-Orb's default safety model: no `javascript:`/`data:` URLs, no local/private hosts when unrestricted navigation is off, and optional explicit domain allowlists.
- **Interacts with**: `browser_open` and command construction.

### Tool handlers (`browser_open`, `browser_snapshot`, `browser_click`, `browser_fill`, `browser_wait`, `browser_get_text`, `browser_screenshot`, `browser_close`, `browser_eval`)
- **Does**: Map plugin tool calls onto specific `agent-browser` commands and normalize the results into JSON payloads suitable for model reasoning and chat media rendering.
- **Interacts with**: `plugin.invoke_tool` and Ponderer's media metadata contract.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| Ponderer runtime host | One JSON response line per request and stable method names | Changing transport or method names |
| Tool loop | Tool names and argument schema remain stable | Renaming tools or required params |
| Operators | Dangerous capabilities (`browser_eval`, unrestricted navigation, persistent auth) stay disabled by default | Flipping defaults or removing setting gates |
| `agent-browser` CLI | `--json` output remains parseable and commands keep their documented names | Incompatible CLI command/JSON changes |

## Notes
- Browser-Orb itself uses only Python stdlib; the real runtime dependency is the external `agent-browser` binary.
- `browser_eval` is present in the handshake but hard-gated by settings at invocation time.
- Screenshot paths default into plugin-local `data/screenshots/` so the bundle stays portable.
- When a domain allowlist is configured and unrestricted navigation is off, Browser-Orb also forwards that allowlist to `agent-browser`'s own guardrail flag.
