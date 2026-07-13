# __init__.py

## Purpose

Defines the stable, compact import surface used by domain plugin packages.
Internal module layout can evolve without forcing every Orb to change imports.

## Components

### Public exports
- **Does**: Re-exports the plugin class, static tool-contract loader, server entry
  point, protocol constants, errors, effects, enums, and typed wire models.
- **Interacts with**: future Graphchan, browser, image, voice, and model-authored
  plugin packages.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| Plugin entry points | `Plugin`, `PluginMetadata`, models, and `serve_stdio` are importable here | Removing or renaming exports |
| Plugin tests | Protocol constants and `PluginServer` are available without internal imports | Export changes |
