"""Fake host and reusable conformance checks for Ponderer plugins."""

from __future__ import annotations

import json
from collections.abc import Mapping, Sequence
from typing import Any

from .models import PromptSlot, ToolCategory
from .plugin import Plugin
from .protocol import PROTOCOL_V1
from .server import PluginServer


class HostCallError(RuntimeError):
    """An error response observed by `FakeHost`."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(f"{code}: {message}")
        self.code = code
        self.message = message


class FakeHost:
    """A deterministic host that crosses the real JSON-line server boundary."""

    def __init__(self, plugin: Plugin) -> None:
        self.server = PluginServer(plugin)
        self._next_id = 1

    def send_raw(self, line: str) -> dict[str, Any]:
        response_line = self.server.handle_line(line)
        if not response_line.endswith("\n") or response_line.endswith("\n\n"):
            raise AssertionError("plugin response must end with exactly one newline")
        response = json.loads(response_line)
        if not isinstance(response, dict):
            raise AssertionError("plugin response must be a JSON object")
        return response

    def request(
        self,
        method: str,
        params: Mapping[str, Any] | None = None,
        *,
        legacy_envelope: bool = False,
    ) -> Any:
        request_id = f"fake-{self._next_id}"
        self._next_id += 1
        request: dict[str, Any] = {
            "id": request_id,
            "method": method,
            "params": dict(params or {}),
        }
        if not legacy_envelope:
            request["protocol_version"] = PROTOCOL_V1
        response = self.send_raw(json.dumps(request))
        if response.get("id") != request_id:
            raise AssertionError(
                f"response id {response.get('id')!r} did not match {request_id!r}"
            )
        if response.get("protocol_version", PROTOCOL_V1) != PROTOCOL_V1:
            raise AssertionError("plugin response did not select protocol v1")
        if not response.get("ok"):
            error = response.get("error") or {}
            raise HostCallError(
                str(error.get("code") or "plugin_error"),
                str(error.get("message") or "Plugin request failed"),
            )
        return response.get("result")

    def handshake(self, *, legacy: bool = False) -> dict[str, Any]:
        params = (
            {}
            if legacy
            else {
                "supported_protocol_versions": [PROTOCOL_V1],
                "host": {"name": "ponderer-test", "version": "0.0.0"},
            }
        )
        result = self.request(
            "plugin.handshake", params, legacy_envelope=legacy
        )
        if not isinstance(result, dict):
            raise AssertionError("handshake result must be an object")
        return result

    def configure(
        self,
        settings: Mapping[str, Any],
        *,
        state: Mapping[str, Any] | None = None,
        legacy: bool = False,
    ) -> dict[str, Any]:
        result = self.request(
            "plugin.configure",
            {"settings": dict(settings), "state": dict(state or {})},
            legacy_envelope=legacy,
        )
        if not isinstance(result, dict):
            raise AssertionError("configure result must be an object")
        return result

    def handle_event(self, event: Mapping[str, Any]) -> dict[str, Any]:
        result = self.request("plugin.handle_event", event)
        if not isinstance(result, dict):
            raise AssertionError("event result must be an object")
        return result

    def prompt(
        self,
        slot: PromptSlot,
        *,
        context: Mapping[str, Any] | None = None,
        legacy: bool = False,
    ) -> list[dict[str, Any]]:
        slot_name = slot.legacy_wire_name if legacy else slot.value
        result = self.request(
            "plugin.get_prompt_contributions",
            {"slot": slot_name, "context": dict(context or {})},
            legacy_envelope=legacy,
        )
        contributions = result.get("contributions") if isinstance(result, dict) else None
        if not isinstance(contributions, list):
            raise AssertionError("prompt result must contain a contributions array")
        return contributions

    def poll_events(self) -> list[dict[str, Any]]:
        result = self.request("plugin.poll_events")
        events = result.get("events") if isinstance(result, dict) else None
        if not isinstance(events, list):
            raise AssertionError("poll result must contain an events array")
        return events

    def invoke_tool(
        self,
        tool: str,
        arguments: Mapping[str, Any],
        *,
        context: Mapping[str, Any] | None = None,
    ) -> dict[str, Any]:
        result = self.request(
            "plugin.invoke_tool",
            {
                "tool": tool,
                "arguments": dict(arguments),
                "context": dict(context or {}),
            },
        )
        if not isinstance(result, dict):
            raise AssertionError("tool result must be an object")
        return result


def validate_handshake(
    handshake: Mapping[str, Any], *, expected_plugin_id: str | None = None
) -> tuple[str, ...]:
    """Return deterministic conformance findings for a handshake payload."""

    findings: list[str] = []
    plugin_id = handshake.get("id")
    if not isinstance(plugin_id, str) or not plugin_id.strip():
        findings.append("id must be a non-empty string")
    elif expected_plugin_id is not None and plugin_id != expected_plugin_id:
        findings.append(
            f"id {plugin_id!r} does not match expected {expected_plugin_id!r}"
        )
    for field in ("name", "version"):
        if not isinstance(handshake.get(field), str) or not handshake[field].strip():
            findings.append(f"{field} must be a non-empty string")

    if handshake.get("protocol_version", PROTOCOL_V1) != PROTOCOL_V1:
        findings.append("protocol_version must select v1")
    supported = handshake.get("supported_protocol_versions", [PROTOCOL_V1])
    if not _string_free_int_sequence(supported) or PROTOCOL_V1 not in supported:
        findings.append("supported_protocol_versions must contain v1")

    capabilities = handshake.get("capabilities")
    if not isinstance(capabilities, Mapping):
        findings.append("capabilities must be an object")
        return tuple(findings)
    tools = handshake.get("tools")
    if not isinstance(tools, list):
        findings.append("tools must be an array")
        return tuple(findings)

    declared_names = capabilities.get("tools")
    if not _string_sequence(declared_names):
        findings.append("capabilities.tools must be an array of strings")
        declared_names = []
    manifest_names: list[str] = []
    for index, tool in enumerate(tools):
        if not isinstance(tool, Mapping):
            findings.append(f"tools[{index}] must be an object")
            continue
        name = tool.get("name")
        if not isinstance(name, str) or not name.strip():
            findings.append(f"tools[{index}].name must be a non-empty string")
        else:
            manifest_names.append(name)
        if not isinstance(tool.get("description"), str) or not tool["description"].strip():
            findings.append(f"tools[{index}].description must be a non-empty string")
        parameters = tool.get("parameters")
        if not isinstance(parameters, Mapping) or parameters.get("type", "object") != "object":
            findings.append(f"tools[{index}].parameters must be an object schema")
        if tool.get("category", ToolCategory.GENERAL.value) not in {
            category.value for category in ToolCategory
        }:
            findings.append(f"tools[{index}].category is unsupported")
        effects = tool.get("effects", [])
        if not isinstance(effects, list):
            findings.append(f"tools[{index}].effects must be an array")
        else:
            for effect_index, effect in enumerate(effects):
                if not isinstance(effect, Mapping):
                    findings.append(
                        f"tools[{index}].effects[{effect_index}] must be an object"
                    )
                    continue
                if not isinstance(effect.get("id"), str) or not effect["id"].strip():
                    findings.append(
                        f"tools[{index}].effects[{effect_index}].id must be a non-empty string"
                    )
    if len(manifest_names) != len(set(manifest_names)):
        findings.append("tool names must be unique")
    if list(declared_names) != manifest_names:
        findings.append("capabilities.tools must exactly match tool manifests")

    if not _string_sequence(capabilities.get("event_hooks", [])):
        findings.append("capabilities.event_hooks must be an array of strings")
    if not _string_sequence(capabilities.get("requested_capabilities", [])):
        findings.append(
            "capabilities.requested_capabilities must be an array of strings"
        )
    prompt_slots = capabilities.get("prompt_slots", [])
    if not _string_sequence(prompt_slots):
        findings.append("capabilities.prompt_slots must be an array of strings")
    else:
        canonical_slots = {slot.value for slot in PromptSlot}
        if any(slot not in canonical_slots for slot in prompt_slots):
            findings.append("capabilities.prompt_slots must use canonical dotted names")
    if not isinstance(capabilities.get("skill_polling", False), bool):
        findings.append("capabilities.skill_polling must be boolean")
    return tuple(findings)


class PluginConformanceMixin:
    """Reusable `unittest` methods; combine with `unittest.TestCase`."""

    def make_plugin(self) -> Plugin:  # pragma: no cover - required test hook
        raise NotImplementedError

    def test_sdk_legacy_host_handshake(self) -> None:
        plugin = self.make_plugin()
        handshake = FakeHost(plugin).handshake(legacy=True)
        self.assertEqual(  # type: ignore[attr-defined]
            validate_handshake(handshake, expected_plugin_id=plugin.metadata.plugin_id),
            (),
        )

    def test_sdk_protocol_v1_handshake(self) -> None:
        plugin = self.make_plugin()
        handshake = FakeHost(plugin).handshake()
        self.assertEqual(handshake["protocol_version"], PROTOCOL_V1)  # type: ignore[attr-defined]
        self.assertEqual(  # type: ignore[attr-defined]
            validate_handshake(handshake, expected_plugin_id=plugin.metadata.plugin_id),
            (),
        )

    def test_sdk_empty_configuration(self) -> None:
        plugin = self.make_plugin()
        result = FakeHost(plugin).configure({})
        self.assertEqual(result, {"configured": True})  # type: ignore[attr-defined]
        self.assertTrue(plugin.configured)  # type: ignore[attr-defined]

    def test_sdk_unknown_method_is_correlated(self) -> None:
        host = FakeHost(self.make_plugin())
        with self.assertRaises(HostCallError) as caught:  # type: ignore[attr-defined]
            host.request("plugin.not_a_method")
        self.assertEqual(caught.exception.code, "unknown_method")  # type: ignore[attr-defined]


def _string_sequence(value: Any) -> bool:
    return isinstance(value, Sequence) and not isinstance(
        value, (str, bytes, bytearray)
    ) and all(isinstance(item, str) for item in value)


def _string_free_int_sequence(value: Any) -> bool:
    return isinstance(value, Sequence) and not isinstance(
        value, (str, bytes, bytearray)
    ) and all(isinstance(item, int) and not isinstance(item, bool) for item in value)
