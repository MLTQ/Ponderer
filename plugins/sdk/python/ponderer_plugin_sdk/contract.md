# contract.py

## Purpose

Loads a package's canonical JSON tool contract so the static host manifest and
runtime SDK registration consume the same schemas, approval hints, and effects.

## Components

### `load_tool_contract`

- **Does**: Parses `{"tools": [...]}`, validates every entry through the typed
  SDK models, rejects duplicate names, and returns manifests keyed by tool ID.
- **Interacts with**: package `plugin.toml` `tool_contract_file`, Rust package
  discovery, and plugin registration code.
- **Rationale**: The handshake attests runtime availability; it must not be a
  second editable source of tool authority.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| Rust host | JSON entries serialize exactly like `RuntimePluginToolManifest` | Diverging field names/defaults |
| Plugin package | Every registered tool comes from this mapping | Reconstructing manifests in code |
| Conformance tests | Duplicate tool names fail during import | Last-write-wins behavior |
