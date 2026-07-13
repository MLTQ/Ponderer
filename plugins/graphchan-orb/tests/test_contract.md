# test_contract.py

## Purpose
Verifies Graphchan-Orb's SDK and domain contracts without starting a subprocess
or contacting the network.

## Components

### `FakePluginClient` / `FakeClientFactory`
- **Does**: Inject deterministic Graphchan behavior and record configuration,
  polling, and publication calls.

### `GraphchanSdkConformanceTests`
- **Does**: Applies the shared SDK's legacy/v1 handshake, configuration,
  correlation, and structured-error suite to `GraphchanPlugin`.

### `GraphchanDomainContractTests`
- **Does**: Parses the explicit v1 manifest, proves the handshake exactly equals
  `tools.json`, and covers SDK dependency, URL rejection, requested
  capabilities/effects, approval flags, settings reload, self-post filtering,
  malformed-record isolation, transactional configuration, poll clamping, and
  stable tool result shapes.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| Runtime host | SDK handshake, polling, and tool envelopes remain deserializable | SDK or domain wire-shape changes |
| Safety policy | Reply/post require approval and declare publication effects | Approval/effect changes |
| Existing agents | Legacy Graphchan tool names and outputs remain stable | Renames or result changes |
| Package discovery | `tool_contract_file` points to the same contract registered at runtime | Adding duplicate manifest authority fields |

## Notes
- Every operation uses in-memory fakes; no socket or HTTP request is possible.
