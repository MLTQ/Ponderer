# test_conformance.py

## Purpose

Exercises the SDK as an installed plugin would use it, without subprocesses or
external I/O. The suite covers both reusable conformance tests and all optional
callback paths.

## Components

### `make_example_plugin`
- **Does**: Builds a fixture with one tool, event hook, prompt provider, poller,
  defaults, and a controllable failure path.

### `ExamplePluginConformanceTests`
- **Does**: Proves `PluginConformanceMixin` can be inherited by domain packages.

### `SdkBehaviorTests`
- **Does**: Covers capability inference, semantic effects, settings replacement,
  typed payloads, dotted/snake prompt compatibility, version rejection,
  exception isolation, state-mutation delivery across handshake, state rollback
  after unserializable results, malformed input, and complete stdio framing.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| SDK maintainers | `python -m unittest discover -s tests -v` uses stdlib only | Test layout or added dependencies |
| Future Orb migrations | Every current host method has a fake-host example | Removing fixture callback coverage |
