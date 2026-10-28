# affect_lab_load_tests.rs

## Purpose
Verifies automatic chat-provider activation through the real HTTP client and
panel controller, using a bounded loopback fixture without model or config writes.

## Components
- `run_load_flow`: Records startup, load, status and activation requests while
  ticking the panel with Settings hidden.
- Success regression: Activation follows successful native readiness exactly once
  and returns the config used to synchronize the main app and footer.
- Failure/cancellation regression: Neither terminal failure activates the provider.
- Stale-status regression: A reply from an older epoch cannot supply readiness.

## Contracts
Included as a child of affect_lab.rs to exercise its private controller. The fixture
uses the existing backend route protocol, closes each connection and bounds waits.
Accepted fixture sockets explicitly use blocking mode on macOS.
