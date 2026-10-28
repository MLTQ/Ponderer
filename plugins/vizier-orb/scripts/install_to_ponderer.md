# install_to_ponderer.py

## Purpose
Copies Vizier and the shared Python SDK beside a Ponderer configuration so the
existing runtime catalog can discover it immediately and on future launches.

## Components
- `install`: Copies into `<plugin_directory>/vizier-orb` and vendors the SDK.
  Excludes caches and tests; rejects symlink targets and differing manifests.

## Contracts
Run from the Ponderer checkout. Does not install or download the Vizier CLI,
change global PATH, edit settings, or restart the agent. Existing matching
packages can be refreshed; other packages in the destination are untouched.
