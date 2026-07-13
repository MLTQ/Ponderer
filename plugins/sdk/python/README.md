# Ponderer Python Plugin SDK

This stdlib-only package owns Ponderer's protocol-v1 JSON-lines contract for
subprocess plugins. It replaces the repeated RPC loops in individual Orbs with
one tested implementation while remaining compatible with the original host.

The host sends explicit protocol-v1 envelopes and negotiates through
`supported_protocol_versions` during `plugin.handshake`. The SDK also treats a
missing envelope version as v1 so packages remain compatible with older hosts.

## Minimal plugin

```python
from ponderer_plugin_sdk import (
    Plugin,
    PluginMetadata,
    ToolCategory,
    ToolManifest,
    ToolResult,
    serve_stdio,
)

plugin = Plugin(PluginMetadata("example", "Example", "0.1.0"))


@plugin.tool(
    ToolManifest(
        name="example_echo",
        description="Echo text.",
        parameters={
            "type": "object",
            "properties": {"text": {"type": "string"}},
            "required": ["text"],
        },
        category=ToolCategory.GENERAL,
    )
)
def echo(arguments):
    return ToolResult.text(str(arguments.get("text", "")))


raise SystemExit(serve_stdio(plugin))
```

Plugins can additionally register lifecycle-event handlers, prompt providers,
and one poller with `Plugin.on_event`, `Plugin.on_prompt`, and
`Plugin.on_poll`. Subclass `Plugin.configure` when settings need domain-specific
validation or resource reloads.

## Conformance testing

`ponderer_plugin_sdk.testing.FakeHost` sends serialized JSON lines through the
same server boundary used in production. Plugin test suites can inherit
`PluginConformanceMixin` and implement `make_plugin()` to get baseline legacy
and protocol-v1 handshake, configuration, correlation, and error tests.

Run the SDK's own suite from this directory:

```bash
python -m unittest discover -s tests -v
```

## Contract boundaries

- One input line produces one correlated response line.
- Protocol v1 is the only supported version.
- Capability prompt-slot names are dotted (`engaged.instructions`).
- Snake-case prompt slots remain accepted and reflected for the original host.
- Plugins return typed wire models; the host remains responsible for effective
  permissions, process supervision, durable event delivery, and state.
