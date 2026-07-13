# server.py

## Purpose

Provides the Graphchan-Orb process entrypoint. All JSON-lines framing,
negotiation, dispatch, and structured errors are delegated to the shared SDK.

## Components

### `PLUGIN`
- **Does**: Holds the long-lived production `GraphchanPlugin` instance.
- **Interacts with**: `build_plugin` in `plugin.py`.

### `main`
- **Does**: Runs `serve_stdio(PLUGIN)` until the host closes stdin.
- **Interacts with**: `ponderer_plugin_sdk.server` and `plugin.toml`.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `plugin.toml` | Module remains executable as `graphchan_orb.server` | Moving the entrypoint |
| Runtime host | Stdout contains only SDK-framed protocol responses | Printing domain diagnostics to stdout |
| `plugin.py` | Domain behavior remains outside this transport entrypoint | Reintroducing tool or event dispatch here |
