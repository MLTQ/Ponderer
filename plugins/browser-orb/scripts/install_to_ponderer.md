# install_to_ponderer.sh

## Purpose
Installs a development checkout of `Browser-Orb` into a Ponderer workspace by symlinking the repo into `plugins/browser-orb`.

## Components

### `install_to_ponderer.sh`
- **Does**: Creates target `plugins/` directory if needed, updates the `browser-orb` symlink, and then runs `install_portable.sh` through the installed path.
- **Interacts with**: `plugin.toml` discovery and `scripts/install_portable.sh`.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| Ponderer plugin discovery | `plugins/browser-orb/plugin.toml` is reachable after install | Changing install target name |
| Operators | Existing real directories are never silently overwritten | Removing non-symlink safety check |

## Notes
- For a fully portable package, copying this repo directory into `plugins/browser-orb` also works.
