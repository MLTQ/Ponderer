# Browser-Orb

`Browser-Orb` is a portable `runtime_process` plugin for Ponderer that adds live browser automation through [`agent-browser`](https://github.com/vercel-labs/agent-browser).

## What It Does

- Opens and navigates live websites with `browser_open`
- Reads accessibility-tree snapshots with stable refs via `browser_snapshot`
- Clicks, fills, waits, reads text, and captures screenshots
- Optionally evaluates JavaScript with `browser_eval`
- Optionally reuses authenticated browser state when persistent auth is enabled
- Returns screenshot media metadata so captures can appear directly in chat

## Tool Surface

- `browser_open`
- `browser_snapshot`
- `browser_click`
- `browser_fill`
- `browser_wait`
- `browser_get_text`
- `browser_screenshot`
- `browser_close`
- `browser_eval` (gated by settings, off by default)

## Safety Defaults

- Plugin disabled by default
- Unrestricted navigation disabled by default
- JavaScript eval disabled by default
- Persistent auth disabled by default
- Optional domain allowlist support for public-web browsing

When unrestricted navigation is disabled, Browser-Orb blocks:
- local/private hosts
- non-`http`/`https` URLs
- `javascript:` and `data:` URLs

## Layout

- `plugin.toml`: Ponderer runtime plugin manifest
- `settings.schema.json`: declarative settings tab schema
- `browser_orb/server.py`: stdio JSON-RPC server
- `scripts/install_portable.sh`: local virtualenv setup
- `scripts/run_plugin.sh`: plugin entrypoint used by Ponderer
- `scripts/install_to_ponderer.sh`: dev helper to install into a Ponderer folder

## Portable Usage

1. Install `agent-browser` separately:
   - `npm install -g agent-browser && agent-browser install`
   - or `brew install agent-browser && agent-browser install`
   - or `cargo install agent-browser && agent-browser install`
2. Run `./scripts/install_portable.sh`
3. Enable the plugin in Ponderer settings
4. Ask the agent to use Browser-Orb for a web task

## Notes

- Browser-Orb itself is pure Python stdlib. The heavy dependency is the external `agent-browser` binary and its browser install.
- Persistent auth uses `agent-browser`'s session persistence features. If you enable it, strongly consider also setting `Persistent Auth Key`.
- For portable distribution, keep the entire `browser-orb` directory together with `.venv` and `data/`.
