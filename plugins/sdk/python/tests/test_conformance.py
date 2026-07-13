from __future__ import annotations

import io
import json
import unittest
from typing import Any

from ponderer_plugin_sdk import (
    EventAck,
    Plugin,
    PluginEffect,
    PluginMetadata,
    PluginServer,
    PollEvent,
    PromptContribution,
    PromptKind,
    PromptSlot,
    ToolCategory,
    ToolManifest,
    ToolResult,
)
from ponderer_plugin_sdk.testing import (
    FakeHost,
    HostCallError,
    PluginConformanceMixin,
    validate_handshake,
)


def make_example_plugin() -> Plugin:
    plugin = Plugin(
        PluginMetadata("example-plugin", "Example Plugin", "0.1.0"),
        default_settings={"greeting": "hello"},
        requested_capabilities=("example.read",),
    )

    @plugin.tool(
        ToolManifest(
            name="example_echo",
            description="Echo a JSON payload.",
            parameters={
                "type": "object",
                "properties": {"value": {}},
            },
            category=ToolCategory.GENERAL,
            effects=(
                PluginEffect(
                    "example.observe",
                    "Reads the fixture payload without an external side effect.",
                ),
            ),
        )
    )
    def echo(arguments: dict[str, Any]) -> ToolResult:
        if arguments.get("explode"):
            raise RuntimeError("example exploded")
        payload = {"echo": arguments.get("value")}
        if arguments.get("remember"):
            plugin.set_state("last_echo", arguments.get("value"), schema_version=2)
            payload["username"] = (
                plugin.invocation_context.username
                if plugin.invocation_context is not None
                else None
            )
        return ToolResult.json(payload)

    @plugin.on_event("persona_evolved")
    def persona_evolved(event) -> EventAck:
        return EventAck(
            state_changed=True,
            summary=f"saw {event.get('current_self_description', '')}",
        )

    @plugin.on_prompt(PromptSlot.ENGAGED_INSTRUCTIONS)
    def engaged_prompt(_query) -> PromptContribution:
        return PromptContribution(
            slot=PromptSlot.ENGAGED_INSTRUCTIONS,
            kind=PromptKind.INSTRUCTION,
            text="Use the example tool when an echo is useful.",
            priority=10,
            max_chars=100,
        )

    @plugin.on_poll
    def poll() -> list[PollEvent]:
        return [
            PollEvent(
                event_id="event-1",
                source="example",
                author="fixture",
                body="hello",
                parent_ids=("root",),
            )
        ]

    return plugin


class ExamplePluginConformanceTests(PluginConformanceMixin, unittest.TestCase):
    def make_plugin(self) -> Plugin:
        return make_example_plugin()


class SdkBehaviorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.plugin = make_example_plugin()
        self.host = FakeHost(self.plugin)

    def test_handshake_derives_all_capabilities_from_handlers(self) -> None:
        handshake = self.host.handshake()
        self.assertEqual(validate_handshake(handshake), ())
        self.assertEqual(handshake["capabilities"]["tools"], ["example_echo"])
        self.assertEqual(
            handshake["capabilities"]["event_hooks"], ["persona_evolved"]
        )
        self.assertEqual(
            handshake["capabilities"]["prompt_slots"],
            ["engaged.instructions"],
        )
        self.assertEqual(
            handshake["capabilities"]["requested_capabilities"],
            ["example.read"],
        )
        self.assertEqual(
            handshake["tools"][0]["effects"][0]["id"], "example.observe"
        )
        self.assertIs(handshake["capabilities"]["skill_polling"], True)

    def test_configure_merges_defaults_and_replaces_previous_payload(self) -> None:
        self.host.configure({"greeting": "hi", "count": 1})
        self.assertEqual(dict(self.plugin.settings), {"greeting": "hi", "count": 1})
        self.host.configure({"count": 2})
        self.assertEqual(
            dict(self.plugin.settings), {"greeting": "hello", "count": 2}
        )

    def test_host_restores_state_and_tool_mutations_cross_the_rpc_boundary(self) -> None:
        configured = self.host.configure(
            {},
            state={"cursor": {"schema_version": 3, "value": {"page": 4}}},
        )
        self.assertEqual(configured, {"configured": True})
        self.assertEqual(self.plugin.state["cursor"], {"page": 4})

        result = self.host.invoke_tool(
            "example_echo",
            {"value": "durable", "remember": True},
            context={
                "username": "Ponderer",
                "autonomous": True,
                "working_directory": "/tmp/work",
                "invoked_at": "2026-07-13T12:00:00Z",
                "deadline_at": "2026-07-13T12:05:00Z",
            },
        )
        self.assertEqual(result["data"]["username"], "Ponderer")
        self.assertEqual(
            result["state_updates"],
            [
                {
                    "key": "last_echo",
                    "schema_version": 2,
                    "value": "durable",
                    "delete": False,
                }
            ],
        )
        self.assertIsNone(self.plugin.invocation_context)

        with self.assertRaises(ValueError):
            self.plugin.set_state("not_json", float("nan"))

    def test_handshake_does_not_consume_pending_state_mutations(self) -> None:
        self.plugin.set_state("after_handshake", {"tick": 1})
        self.assertEqual(self.host.handshake()["id"], "example-plugin")

        result = self.host.invoke_tool("example_echo", {"value": "next"})
        self.assertEqual(
            result["state_updates"],
            [
                {
                    "key": "after_handshake",
                    "schema_version": 1,
                    "value": {"tick": 1},
                    "delete": False,
                }
            ],
        )

    def test_event_tool_and_poll_callbacks_use_typed_wire_shapes(self) -> None:
        event = self.host.handle_event(
            {"event": "persona_evolved", "current_self_description": "curious"}
        )
        self.assertEqual(
            event, {"state_changed": True, "summary": "saw curious"}
        )
        self.assertEqual(
            self.host.invoke_tool("example_echo", {"value": [1, 2]}),
            {"kind": "json", "data": {"echo": [1, 2]}},
        )
        self.assertEqual(
            self.host.poll_events(),
            [
                {
                    "id": "event-1",
                    "source": "example",
                    "author": "fixture",
                    "body": "hello",
                    "parent_ids": ["root"],
                }
            ],
        )

    def test_prompt_slots_are_canonical_but_reflect_legacy_host_alias(self) -> None:
        modern = self.host.prompt(PromptSlot.ENGAGED_INSTRUCTIONS)
        self.assertEqual(modern[0]["slot"], "engaged.instructions")
        self.assertEqual(modern[0]["plugin_id"], "example-plugin")

        legacy = self.host.prompt(PromptSlot.ENGAGED_INSTRUCTIONS, legacy=True)
        self.assertEqual(legacy[0]["slot"], "engaged_instructions")

    def test_unsupported_negotiation_and_envelope_versions_are_distinct(self) -> None:
        with self.assertRaises(HostCallError) as negotiation:
            self.host.request(
                "plugin.handshake", {"supported_protocol_versions": [99]}
            )
        self.assertEqual(negotiation.exception.code, "unsupported_protocol")

        response = self.host.send_raw(
            json.dumps(
                {
                    "id": "bad-version",
                    "protocol_version": 99,
                    "method": "plugin.handshake",
                    "params": {},
                }
            )
        )
        self.assertEqual(response["id"], "bad-version")
        self.assertEqual(response["error"]["code"], "unsupported_protocol")

    def test_plugin_exception_is_correlated_and_server_remains_available(self) -> None:
        with self.assertRaises(HostCallError) as caught:
            self.host.invoke_tool("example_echo", {"explode": True})
        self.assertEqual(caught.exception.code, "plugin_error")
        self.assertEqual(caught.exception.message, "Plugin handler failed")
        self.assertEqual(
            self.host.invoke_tool("example_echo", {"value": "still alive"})["data"],
            {"echo": "still alive"},
        )

    def test_non_json_plugin_result_is_a_correlated_error_not_a_process_crash(self) -> None:
        original = self.plugin._tools["example_echo"]

        def invalid_result(_arguments):
            self.plugin.set_state("must_rollback", {"attempt": 1})
            return ToolResult.json({"bad": {1}})

        self.plugin._tools["example_echo"] = (
            original[0],
            invalid_result,
        )
        failed = self.host.send_raw(
            json.dumps(
                {
                    "id": "non-json-result",
                    "method": "plugin.invoke_tool",
                    "params": {"tool": "example_echo", "arguments": {}},
                }
            )
        )
        self.assertEqual(failed["id"], "non-json-result")
        self.assertEqual(failed["error"]["code"], "serialization_error")
        self.assertNotIn("must_rollback", self.plugin.state)
        self.plugin._tools["example_echo"] = original
        handshake = self.host.handshake()
        self.assertEqual(handshake["id"], "example-plugin")
        self.assertNotIn("state_updates", handshake)

    def test_malformed_json_and_invalid_params_have_stable_errors(self) -> None:
        malformed = self.host.send_raw("not json")
        self.assertEqual(malformed["id"], "unknown")
        self.assertEqual(malformed["error"]["code"], "invalid_json")

        response = self.host.send_raw(
            json.dumps(
                {
                    "id": "bad-params",
                    "method": "plugin.configure",
                    "params": {"settings": []},
                }
            )
        )
        self.assertEqual(response["id"], "bad-params")
        self.assertEqual(response["error"]["code"], "invalid_params")

        non_standard = self.host.send_raw(
            '{"id":"nan","method":"plugin.configure","params":{"x":NaN}}'
        )
        self.assertEqual(non_standard["error"]["code"], "invalid_json")

    def test_stdio_server_skips_blank_lines_and_flushes_one_line_per_request(self) -> None:
        requests = "\n".join(
            [
                "",
                json.dumps(
                    {
                        "id": "one",
                        "method": "plugin.handshake",
                        "params": {},
                    }
                ),
                "",
                json.dumps(
                    {
                        "id": "two",
                        "method": "plugin.configure",
                        "params": {"settings": {}},
                    }
                ),
                "",
            ]
        )
        output = io.StringIO()
        result = PluginServer(self.plugin).serve(io.StringIO(requests), output)
        self.assertEqual(result, 0)
        response_lines = output.getvalue().splitlines()
        self.assertEqual(len(response_lines), 2)
        self.assertEqual([json.loads(line)["id"] for line in response_lines], ["one", "two"])


if __name__ == "__main__":
    unittest.main()
