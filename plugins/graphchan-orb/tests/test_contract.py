from __future__ import annotations

import json
import unittest
from pathlib import Path
from typing import Any

from graphchan_orb import server


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


class RuntimeContractTests(unittest.TestCase):
    def setUp(self) -> None:
        server._settings = dict(server.DEFAULT_SETTINGS)
        server._client = None

    def test_manifest_defaults_disabled_and_uses_runtime_entrypoint(self) -> None:
        root = Path(__file__).resolve().parents[1]
        schema = json.loads((root / "settings.schema.json").read_text())
        enabled = next(field for field in schema["fields"] if field["key"] == "enabled")
        self.assertIs(enabled["default_value"], False)

        manifest = (root / "plugin.toml").read_text()
        self.assertIn('plugin_type = "runtime_process"', manifest)
        self.assertIn('command = ["./scripts/run_plugin.sh"]', manifest)

    def test_handshake_matches_host_contract_and_approval_policy(self) -> None:
        handshake = server._handshake()
        self.assertEqual(handshake["id"], "graphchan-orb")
        self.assertIs(handshake["capabilities"]["skill_polling"], True)
        self.assertEqual(handshake["capabilities"]["event_hooks"], ["settings_changed"])

        tools = {tool["name"]: tool for tool in handshake["tools"]}
        self.assertEqual(set(tools), set(handshake["capabilities"]["tools"]))
        self.assertIs(tools["graphchan_reply"]["requires_approval"], True)
        self.assertIs(tools["graphchan_post"]["requires_approval"], True)
        self.assertIs(tools["graphchan_list_threads"]["requires_approval"], False)

    def test_rpc_envelope_correlates_success_and_error_responses(self) -> None:
        success = server._handle_line(
            json.dumps({"id": "req-1", "method": "plugin.handshake", "params": {}})
        )
        self.assertEqual(success["id"], "req-1")
        self.assertIs(success["ok"], True)

        error = server._handle_line(
            json.dumps({"id": "req-2", "method": "plugin.unknown", "params": {}})
        )
        self.assertEqual(error["id"], "req-2")
        self.assertIs(error["ok"], False)
        self.assertEqual(error["error"]["code"], "plugin_error")

    def test_configure_validates_settings_and_reloads_from_event(self) -> None:
        with self.assertRaisesRegex(ValueError, "object settings payload"):
            server._configure({"settings": "invalid"})

        result = server._configure(
            {
                "settings": {
                    "api_url": "http://graphchan.test/",
                    "agent_name": "Arlecchino",
                }
            }
        )
        self.assertEqual(result, {"configured": True})
        assert server._client is not None
        self.assertEqual(server._client.base_url, "http://graphchan.test")
        self.assertEqual(server._client.agent_name, "Arlecchino")

        event = server._handle_event(
            {
                "event": "settings_changed",
                "plugin_id": "graphchan-orb",
                "settings": {"api_url": "http://new.test", "agent_name": "Pantalone"},
            }
        )
        self.assertIs(event["state_changed"], True)
        assert server._client is not None
        self.assertEqual(server._client.agent_name, "Pantalone")

    def test_polling_filters_self_posts_and_clamps_limit_without_network(self) -> None:
        fake = FakePluginClient(
            posts=[
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
        )
        server._client = fake  # type: ignore[assignment]
        server._settings = {"agent_name": "Ponderer", "poll_limit": 9999}

        result = server._poll_events({})

        self.assertEqual(fake.poll_limits, [200])
        self.assertEqual(
            result,
            {
                "events": [
                    {
                        "id": "other-post",
                        "source": "One",
                        "author": "peer-2",
                        "body": "hello",
                        "parent_ids": ["root"],
                    }
                ]
            },
        )

    def test_tool_results_match_host_envelope_without_network(self) -> None:
        fake = FakePluginClient()
        server._client = fake  # type: ignore[assignment]

        reply = server._invoke_tool(
            {
                "tool": "graphchan_reply",
                "arguments": {"post_id": "post-1", "content": "reply"},
            }
        )
        self.assertEqual(reply["kind"], "json")
        self.assertEqual(reply["data"]["status"], "ok")
        self.assertEqual(fake.created[0]["parent_post_ids"], ["post-1"])

        listing = server._invoke_tool(
            {"tool": "graphchan_list_threads", "arguments": {}}
        )
        self.assertEqual(listing, {"kind": "text", "text": "thread-1: One"})


if __name__ == "__main__":
    unittest.main()
