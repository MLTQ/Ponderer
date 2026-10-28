# tools.json

## Purpose
Single source of truth for the `vz` and `vz_ssh` model tool contracts.
The plugin loads this same file for its runtime handshake.
Repeated effect IDs have identical declarations across tools so host discovery
can unify their policy metadata without conflicts.

## Components
- `vz`: Compact local wake by default, optional snapshot and verbose output.
- `vz_ssh`: Same modes on a required SSH host, with optional port override.
- Effects: Filesystem reads and process execution; SSH also declares network
  reads. Both retain the host's normal shell approval policy.

## Contracts
No arbitrary CLI arguments, watch streams, install action, or host command
interpolation. Missing installations return a shell command as data. The shell
tool owns any later installation approval and execution.
