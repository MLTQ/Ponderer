"""Graphchan domain adapter implemented on the shared Ponderer plugin SDK."""

from __future__ import annotations

from collections.abc import Mapping
from pathlib import Path
from typing import Any, Protocol

from ponderer_plugin_sdk import (
    ConfigureResult,
    EventAck,
    LifecycleEvent,
    Plugin,
    PluginMetadata,
    PollEvent,
    ToolResult,
    load_tool_contract,
)

from . import __version__
from .client import GraphchanClient, normalize_base_url

PLUGIN_ID = "graphchan-orb"
DEFAULT_SETTINGS: dict[str, Any] = {
    "enabled": False,
    "api_url": "http://localhost:8080",
    "agent_name": "Ponderer",
    "poll_limit": 20,
}

TOOL_CONTRACT = load_tool_contract(Path(__file__).resolve().parent.parent / "tools.json")
REPLY_TOOL = TOOL_CONTRACT["graphchan_reply"]
LIST_THREADS_TOOL = TOOL_CONTRACT["graphchan_list_threads"]
POST_TOOL = TOOL_CONTRACT["graphchan_post"]


class GraphchanApi(Protocol):
    def get_recent_posts(self, limit: int = 20) -> list[dict[str, Any]]: ...

    def list_threads(self) -> list[dict[str, Any]]: ...

    def resolve_thread_for_post(self, post_id: str) -> str | None: ...

    def create_post(
        self,
        thread_id: str,
        body: str,
        parent_post_ids: list[str] | None = None,
    ) -> dict[str, Any]: ...


class GraphchanClientFactory(Protocol):
    def __call__(self, base_url: str, agent_name: str) -> GraphchanApi: ...


class GraphchanPlugin(Plugin):
    """Graphchan configuration, polling, and tool behavior without RPC plumbing."""

    def __init__(
        self, client_factory: GraphchanClientFactory = GraphchanClient
    ) -> None:
        super().__init__(
            PluginMetadata(PLUGIN_ID, "Graphchan-Orb", __version__),
            default_settings=DEFAULT_SETTINGS,
            requested_capabilities=("network.read", "external.publish"),
        )
        self._client_factory = client_factory
        self._client: GraphchanApi | None = None
        self.tool(REPLY_TOOL)(self.reply)
        self.tool(LIST_THREADS_TOOL)(self.list_threads)
        self.tool(POST_TOOL)(self.post)
        self.on_event("settings_changed")(self.settings_changed)
        self.on_poll(self.poll)

    def configure(self, settings: Mapping[str, Any]) -> ConfigureResult:
        candidate_settings = dict(DEFAULT_SETTINGS)
        candidate_settings.update(settings)
        raw_api_url = candidate_settings.get("api_url")
        if not isinstance(raw_api_url, str):
            raise ValueError("api_url must be a string")
        api_url = normalize_base_url(raw_api_url)
        raw_agent_name = candidate_settings.get("agent_name")
        if not isinstance(raw_agent_name, str):
            raise ValueError("agent_name must be a string")
        agent_name = raw_agent_name.strip() or "Ponderer"
        client = self._client_factory(api_url, agent_name)
        result = super().configure(settings)
        self._client = client
        return result

    def settings_changed(self, event: LifecycleEvent) -> EventAck:
        if event.get("plugin_id") != PLUGIN_ID:
            return EventAck()
        settings = event.get("settings")
        if settings is None:
            settings = {}
        if not isinstance(settings, Mapping):
            raise ValueError("settings_changed settings must be an object")
        self.configure(settings)
        return EventAck(state_changed=True, summary="Settings reloaded.")

    def poll(self) -> list[PollEvent]:
        client = self._get_client()
        try:
            limit = int(self.settings.get("poll_limit", 20))
        except (TypeError, ValueError):
            limit = 20
        limit = max(1, min(limit, 200))
        agent_name = str(self.settings.get("agent_name") or "Ponderer").strip()

        try:
            recent = client.get_recent_posts(limit=limit)
        except Exception as exc:
            raise RuntimeError(f"Graphchan poll failed: {exc}") from exc
        if not isinstance(recent, list):
            raise RuntimeError("Graphchan poll failed: recent posts must be an array")

        events: list[PollEvent] = []
        seen_event_ids: set[str] = set()
        for item in recent:
            if not isinstance(item, Mapping):
                continue
            post = item.get("post", item)
            if not isinstance(post, Mapping):
                continue

            event_id = post.get("id")
            body = post.get("body")
            if (
                not isinstance(event_id, str)
                or not event_id.strip()
                or event_id in seen_event_ids
                or not isinstance(body, str)
            ):
                continue

            metadata = post.get("metadata")
            metadata = metadata if isinstance(metadata, Mapping) else {}
            agent_info = metadata.get("agent")
            agent_info = agent_info if isinstance(agent_info, Mapping) else {}
            if agent_name and agent_info.get("name") == agent_name:
                continue

            thread_title = item.get("thread_title")
            thread_id = post.get("thread_id")
            source = (
                thread_title
                if isinstance(thread_title, str) and thread_title.strip()
                else thread_id
                if isinstance(thread_id, str) and thread_id.strip()
                else "Graphchan"
            )
            author = post.get("author_peer_id")
            if not isinstance(author, str) or not author.strip():
                author = "Anonymous"
            raw_parent_ids = post.get("parent_post_ids")
            parent_ids = (
                tuple(
                    parent
                    for parent in raw_parent_ids
                    if isinstance(parent, str) and parent.strip()
                )
                if isinstance(raw_parent_ids, (list, tuple))
                else ()
            )

            events.append(
                PollEvent(
                    event_id=event_id,
                    source=source,
                    author=author,
                    body=body,
                    parent_ids=parent_ids,
                )
            )
            seen_event_ids.add(event_id)
        return events

    def reply(self, arguments: dict[str, Any]) -> ToolResult:
        post_id = str(arguments.get("post_id") or "").strip()
        content = str(arguments.get("content") or "").strip()
        thread_id = str(arguments.get("thread_id") or "").strip() or None
        if not post_id:
            return ToolResult.error("Missing post_id")
        if not content:
            return ToolResult.error("Missing content")

        client = self._get_client()
        if not thread_id:
            thread_id = client.resolve_thread_for_post(post_id)
            if not thread_id:
                return ToolResult.error(
                    f"Could not resolve thread for post {post_id!r}"
                )
        post = client.create_post(
            thread_id=thread_id,
            body=content,
            parent_post_ids=[post_id],
        )
        return ToolResult.json(
            {
                "status": "ok",
                "post_id": post.get("id", "unknown"),
                "thread_id": thread_id,
            }
        )

    def list_threads(self, _arguments: dict[str, Any]) -> ToolResult:
        threads = self._get_client().list_threads()
        lines = [
            f"{thread.get('id', '?')}: {thread.get('title', '(untitled)')}"
            for thread in threads[:10]
        ]
        return ToolResult.text("\n".join(lines) if lines else "No threads found.")

    def post(self, arguments: dict[str, Any]) -> ToolResult:
        thread_id = str(arguments.get("thread_id") or "").strip()
        body = str(arguments.get("body") or "").strip()
        reply_to = str(arguments.get("reply_to_post_id") or "").strip() or None
        if not thread_id:
            return ToolResult.error("Missing thread_id")
        if not body:
            return ToolResult.error("Missing body")

        post = self._get_client().create_post(
            thread_id=thread_id,
            body=body,
            parent_post_ids=[reply_to] if reply_to else [],
        )
        return ToolResult.json(
            {
                "status": "posted",
                "post_id": post.get("id", "unknown"),
                "thread_id": thread_id,
            }
        )

    def _get_client(self) -> GraphchanApi:
        if self._client is None:
            raise RuntimeError("Plugin not configured; call plugin.configure first")
        return self._client


def build_plugin(
    client_factory: GraphchanClientFactory = GraphchanClient,
) -> GraphchanPlugin:
    return GraphchanPlugin(client_factory=client_factory)
