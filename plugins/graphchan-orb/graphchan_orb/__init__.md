# __init__.py

## Purpose
Declares Graphchan-Orb package metadata shared by packaging and the SDK-backed
runtime handshake.

## Components

### `__version__`
- **Does**: Exposes the plugin version string.
- **Interacts with**: `graphchan_orb/plugin.py` handshake metadata.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `plugin.py` | `__version__` is importable | Removing or renaming the symbol |
| `plugin.toml` / `pyproject.toml` | Version values remain aligned | Publishing mismatched versions |
