# __init__.py

## Purpose
Declares the Browser-Orb package version so the runtime server can report stable plugin metadata during handshake.

## Components

### `__version__`
- **Does**: Exposes the plugin version string.
- **Interacts with**: `browser_orb/server.py` handshake metadata.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `browser_orb/server.py` | `__version__` is importable at runtime | Removing or renaming the symbol |
