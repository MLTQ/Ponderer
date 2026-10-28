# plugin.toml

## Purpose
Declares the Vizier protocol-v1 package, enabled by default when installed.
Keeps system perception in the existing subprocess plugin architecture.

## Components
- `command`: Runs the stdlib Python adapter using the shared SDK.
- `contributions`: Supplies short engaged, ambient, and orientation guidance;
  performs no background polling or observation during prompt assembly.
- `requested_capabilities`: Matches the fixed CLI and SSH tool effects.

## Contracts
The static `tools.json`, runtime handshake and SDK registrations must match.
The host owns plugin enablement, supervision and normal tool approvals.
