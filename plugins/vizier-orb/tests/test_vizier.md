# test_vizier.py

## Purpose
Exercises the real SDK boundary and subprocess runner using temporary fake
executables, with no real SSH connections or machine observation collection.

## Components
- Protocol conformance plus static/runtime contract equality, identical effect
  declarations across tools (required by host discovery), and prompt budgets.
- Real fake CLI calls validate compact/verbose argument semantics and paths
  containing spaces and quotes.
- Remote script execution in a fake HOME checks remote PATH and quoting;
  mocked SSH results distinguish missing installs from authentication failures.
- Bounds tests cover timeout cleanup including inherited collector pipes,
  UTF-8 oversized output, malformed observations and unsupported arguments.
- Portable installation test launches the copied package outside the checkout,
  verifies vendored SDK resolution and protects differing existing manifests.

## Contracts
Run with `python3 -m unittest discover -s plugins/vizier-orb/tests -v`.
These tests do not require network access or a locally installed Vizier binary.
