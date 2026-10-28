# validate_backend.py

## Purpose
Checks the actual Rust host's discovery, static contract validation, handshake,
configuration and registration against an installed Vizier package. SDK-only
tests cannot exercise host policy unification or lifecycle reconciliation.

## Components
- `validate`: Copies a supplied Ponderer binary into a temporary directory,
  writes an inert local-only test configuration, starts its backend, waits for
  Vizier to run, and checks tool registration and default enablement via HTTP.

## Contracts
Requires loopback sockets and subprocess execution. Does not invoke observation
tools, contact a remote model, or change the operator's saved configuration.
Pass a plugin directory containing only the package under test; enabled packages
in that directory may start. Closes the parent pipe and reaps the test backend.
