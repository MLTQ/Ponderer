# plugin.py

## Purpose
Adapts Vizier observations to Ponderer's shared protocol-v1 SDK and gives the
model persistent, bounded guidance for local and SSH orientation.

## Components
- `build_plugin`: Registers tools from the static contract and defaults settings.
- `run`: Converts execution failures into actionable tool errors.
- `guidance`: Adds a sub-300-character reminder in engaged/ambient instructions
  and orientation context, without collecting machine data on every prompt.

## Contracts
`vz` and `vz_ssh` stay distinct so the target is explicit. Both return JSON
observations or errors. The adapter does not install software or connect to a
remote host until a tool invocation. It delegates all framing to the SDK.
