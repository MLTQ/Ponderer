# __init__.py

## Purpose
Defines package exports and version for the `image_orb` runtime plugin package.

## Components

### `__version__`
- **Does**: Exposes plugin package version used by the runtime handshake.
- **Interacts with**: `image_orb/server.py`.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `server.py` | `__version__` exists and is a string | Renaming/removing version export |

