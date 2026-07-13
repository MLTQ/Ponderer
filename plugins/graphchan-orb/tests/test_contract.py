from __future__ import annotations

import json
import tomllib
import unittest
from pathlib import Path
from typing import Any

from graphchan_orb.plugin import GraphchanPlugin
from ponderer_plugin_sdk import Plugin
from ponderer_plugin_sdk.testing import (
    FakeHost,
    HostCallError,
    PluginConformanceMixin,
)


class FakePluginClient:
    def __init__(self, posts: list[dict[str, Any]] | None = None) -> None:
        self.posts = posts or []
        self.poll_limits: list[int] = []
        self.created: list[dict[str, Any]] = []

    def get_recent_posts(self, limit: int = 20) -> list[dict[str, Any]]:
        self.poll_limits.append(limit)
        return self.posts

    def list_threads(self) -> list[dict[str, Any]]:
        return [{"id": "thread-1", "title": "One"}]

    def resolve_thread_for_post(self, post_id: str) -> str | None:
        return "thread-1" if post_id == "post-1" else None

    def create_post(self, **kwargs: Any) -> dict[str, Any]:
        self.created.append(kwargs)
        return {"id": "created-1"}


class FakeClientFactory:
    def __init__(self, client: FakePluginClient | None = None) -> None:
        self.client = client or FakePluginClient()
        self.calls: list[tuple[str, str]] = []

    def __call__(self, base_url: str, agent_name: str) -> FakePluginClient:
        self.calls.append((base_url, agent_name))
        return self.client


class GraphchanSdkConformanceTests(PluginConformanceMixin, unittest.TestCase):
    def make_plugin(self) -> Plugin:
        return GraphchanPlugin(client_factory=FakeClientFactory())


class GraphchanDomainContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.client = FakePluginClient()
        self.factory = FakeClientFactory(self.client)
        self.plugin = GraphchanPlugin(client_factory=self.factory)
        self.host = FakeHost(self.plugin)

    def test_manifest_points_to_canonical_tool_contract(self) -> None:
        root = Path(__file__).resolve().parents[1]
        manifest = tomllib.loads(
            (root / "plugin.toml").read_text(encoding="utf-8")
        )
        schema = json.loads(
            (root / "settings.schema.json").read_text(encoding="utf-8")
        )
        project = tomllib.loads(
            (root / "pyproject.toml").read_text(encoding="utf-8")
        )

        self.assertEqual(manifest["manifest_version"], 1)
        self.assertEqual(manifest["protocol_version"], 1)
        self.assertEqual(manifest["plugin_type"], "runtime_process")
        self.assertEqual(manifest["command"], ["./scripts/run_plugin.sh"])
        self.assertEqual(manifest["tool_contract_file"], "tools.json")
        self.assertEqual(
            manifest["requested_capabilities"],
            ["network.read", "external.publish"],
        )
        self.assertNotIn("provided_tools", manifest)
        self.assertNotIn("declared_effects", manifest)
        enabled = next(
            field for field in schema["fields"] if field["key"] == "enabled"
        )
        self.assertIs(enabled["default_value"], False)
        self.assertIn(
            "ponderer-plugin-sdk>=0.1,<0.2",
            project["project"]["dependencies"],
        )

    def test_handshake_exactly_matches_static_tool_contract(self) -> None:
        handshake = self.host.handshake()
        root = Path(__file__).resolve().parents[1]
        tool_contract = json.loads(
            (root / "tools.json").read_text(encoding="utf-8")
        )
        self.assertEqual(handshake["id"], "graphchan-orb")
        self.assertEqual(handshake["protocol_version"], 1)
        self.assertIs(handshake["capabilities"]["skill_polling"], True)
        self.assertEqual(
            handshake["capabilities"]["event_hooks"], ["settings_changed"]
        )
        self.assertEqual(
            handshake["capabilities"]["requested_capabilities"],
            ["network.read", "external.publish"],
        )
        self.assertEqual(handshake["tools"], tool_contract["tools"])
        self.assertEqual(
            handshake["capabilities"]["tools"],
            [tool["name"] for tool in tool_contract["tools"]],
        )

        tools = {tool["name"]: tool for tool in handshake["tools"]}
        self.assertEqual(
            set(tools),
            {"graphchan_reply", "graphchan_list_threads", "graphchan_post"},
        )
        self.assertIs(tools["graphchan_reply"]["requires_approval"], True)
        self.assertIs(tools["graphchan_post"]["requires_approval"], True)
        self.assertIs(
            tools["graphchan_list_threads"]["requires_approval"], False
        )
        self.assertEqual(
            [effect["id"] for effect in tools["graphchan_reply"]["effects"]],
            ["network.read", "external.publish"],
        )
        self.assertEqual(
            [effect["id"] for effect in tools["graphchan_post"]["effects"]],
            ["external.publish"],
        )

    def test_configure_validates_settings_and_reloads_from_event(self) -> None:
        with self.assertRaises(HostCallError) as invalid:
            self.host.request("plugin.configure", {"settings": "invalid"})
        self.assertEqual(invalid.exception.code, "invalid_params")

        result = self.host.configure(
            {"api_url": "http://graphchan.test/", "agent_name": "Arlecchino"}
        )
        self.assertEqual(result, {"configured": True})
        self.assertEqual(
            self.factory.calls[-1], ("http://graphchan.test", "Arlecchino")
        )

        event = self.host.handle_event(
            {
                "event": "settings_changed",
                "plugin_id": "graphchan-orb",
                "settings": {
                    "api_url": "http://new.test",
                    "agent_name": "Pantalone",
                },
            }
        )
        self.assertEqual(
            event, {"state_changed": True, "summary": "Settings reloaded."}
        )
        self.assertEqual(
            self.factory.calls[-1], ("http://new.test", "Pantalone")
        )

        valid_call_count = len(self.factory.calls)
        for invalid_url in (
            "ftp://graphchan.test",
            "http://user:secret@graphchan.test",
            "http://graphchan.test?override=true",
        ):
            with self.subTest(invalid_url=invalid_url):
                with self.assertRaises(HostCallError) as invalid:
                    self.host.configure({"api_url": invalid_url})
                self.assertEqual(invalid.exception.code, "plugin_error")
        self.assertEqual(len(self.factory.calls), valid_call_count)
        self.assertEqual(self.plugin.settings["api_url"], "http://new.test")
        self.assertEqual(self.plugin.settings["agent_name"], "Pantalone")

    def test_polling_filters_self_posts_and_clamps_limit_without_network(self) -> None:
        self.client.posts = [
            {
                "post": {
                    "id": "self-post",
                    "thread_id": "thread-1",
                    "body": "mine",
                    "metadata": {"agent": {"name": "Ponderer"}},
                },
                "thread_title": "One",
            },
            {
                "post": {
                    "id": "other-post",
                    "thread_id": "thread-1",
                    "author_peer_id": "peer-2",
                    "body": "hello",
                    "parent_post_ids": ["root"],
                },
                "thread_title": "One",
            },
        ]
        self.host.configure({"agent_name": "Ponderer", "poll_limit": 9999})

        self.assertEqual(
            self.host.poll_events(),
            [
                {
                    "id": "other-post",
                    "source": "One",
                    "author": "peer-2",
                    "body": "hello",
                    "parent_ids": ["root"],
                }
            ],
        )
        self.assertEqual(self.client.poll_limits, [200])

    def test_polling_isolates_malformed_and_duplicate_records(self) -> None:
        self.client.posts = [
            None,  # type: ignore[list-item]
            {"post": "not-an-object"},
            {"post": {"id": "", "body": "missing id"}},
            {"post": {"id": "bad-body", "body": {"nested": True}}},
            {
                "post": {
                    "id": "valid",
                    "thread_id": "thread-1",
                    "author_peer_id": ["not", "text"],
                    "body": "hello",
                    "parent_post_ids": ["root", None, ""],
                    "metadata": "not-an-object",
                },
                "thread_title": {"not": "text"},
            },
            {"post": {"id": "valid", "body": "duplicate"}},
            {
                "post": {
                    "id": "self-post",
                    "body": "mine",
                    "metadata": {"agent": {"name": "Ponderer"}},
                }
            },
        ]
        self.host.configure({"agent_name": "Ponderer"})

        self.assertEqual(
            self.host.poll_events(),
            [
                {
                    "id": "valid",
                    "source": "thread-1",
                    "author": "Anonymous",
                    "body": "hello",
                    "parent_ids": ["root"],
                }
            ],
        )

    def test_tool_results_and_write_shapes_are_preserved(self) -> None:
        self.host.configure({})
        reply = self.host.invoke_tool(
            "graphchan_reply", {"post_id": "post-1", "content": "reply"}
        )
        self.assertEqual(reply["kind"], "json")
        self.assertEqual(reply["data"]["status"], "ok")
        self.assertEqual(self.client.created[0]["parent_post_ids"], ["post-1"])

        self.assertEqual(
            self.host.invoke_tool("graphchan_list_threads", {}),
            {"kind": "text", "text": "thread-1: One"},
        )
        posted = self.host.invoke_tool(
            "graphchan_post", {"thread_id": "thread-1", "body": "new"}
        )
        self.assertEqual(posted["data"]["status"], "posted")
        self.assertEqual(self.client.created[-1]["parent_post_ids"], [])

    def test_missing_write_arguments_remain_tool_errors(self) -> None:
        self.host.configure({})
        self.assertEqual(
            self.host.invoke_tool("graphchan_reply", {}),
            {"kind": "error", "text": "Missing post_id"},
        )
        self.assertEqual(
            self.host.invoke_tool("graphchan_post", {}),
            {"kind": "error", "text": "Missing thread_id"},
        )


if __name__ == "__main__":
    unittest.main()
