# test_contract.py

## Purpose
Verifies Graphchan-Orb's Ponderer runtime-process contract without starting a subprocess or contacting the network.

## Components

### `RuntimeContractTests`
- **Does**: Covers disabled defaults, manifest entrypoint, handshake capabilities, approval flags, RPC envelopes, settings reload, self-post filtering, poll clamping, and tool result shapes.
- **Interacts with**: `graphchan_orb.server` using `FakePluginClient`.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| Runtime host | Handshake, polling, and tool envelopes remain deserializable | Wire-shape changes |
| Safety policy | Reply/post require approval; listing does not | Approval flag changes |

## Notes
- Every operation uses in-memory fakes; no socket or HTTP request is possible.
