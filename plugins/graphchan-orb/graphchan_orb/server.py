"""Graphchan-Orb plugin server — JSON-RPC over stdio."""
from __future__ import annotations

import json
import sys
from typing import Any

from . import __version__
from .client import GraphchanClient

# ---------------------------------------------------------------------------
# Plugin state
# ---------------------------------------------------------------------------

_settings: dict[str, Any] = {}
_client: GraphchanClient | None = None

DEFAULT_SETTINGS: dict[str, Any] = {
    "enabled": False,
    "api_url": "http://localhost:8080",
    "agent_name": "Ponderer",
    "poll_limit": 20,
}


def _get_client() -> GraphchanClient:
    global _client
    if _client is None:
        raise RuntimeError("Plugin not configured — call plugin.configure first")
    return _client


# ---------------------------------------------------------------------------
# RPC dispatch
# ---------------------------------------------------------------------------

def main() -> int:
    for raw_line in sys.stdin:
        line = raw_line.strip()
        if not line:
            continue
        response = _handle_line(line)
        sys.stdout.write(json.dumps(response, ensure_ascii=False) + "\n")
        sys.stdout.flush()
    return 0


def _handle_line(line: str) -> dict[str, Any]:
    request_id = "unknown"
    try:
        payload = json.loads(line)
        request_id = str(payload.get("id", "unknown"))
        method = payload.get("method", "")
        params = payload.get("params") or {}
        result = _dispatch(method, params)
        return {"id": request_id, "ok": True, "result": result}
    except Exception as exc:
        return {
            "id": request_id,
            "ok": False,
            "error": {"code": "plugin_error", "message": str(exc)},
        }


def _dispatch(method: str, params: dict[str, Any]) -> Any:
    if method == "plugin.handshake":
        return _handshake()
    if method == "plugin.configure":
        return _configure(params)
    if method == "plugin.handle_event":
        return _handle_event(params)
    if method == "plugin.get_prompt_contributions":
        return {"contributions": []}
    if method == "plugin.poll_events":
        return _poll_events(params)
    if method == "plugin.invoke_tool":
        return _invoke_tool(params)
    raise ValueError(f"Unknown method: {method!r}")


# ---------------------------------------------------------------------------
# Handshake
# ---------------------------------------------------------------------------

def _handshake() -> dict[str, Any]:
    return {
        "id": "graphchan-orb",
        "name": "Graphchan-Orb",
        "version": __version__,
        "capabilities": {
            "tools": ["graphchan_reply", "graphchan_list_threads", "graphchan_post"],
            "event_hooks": ["settings_changed"],
            "prompt_slots": [],
            "skill_polling": True,
        },
        "tools": [
            {
                "name": "graphchan_reply",
                "description": (
                    "Reply to a specific Graphchan post. Provide the post_id to reply to "
                    "and the content of the reply. Thread ID is resolved automatically."
                ),
                "parameters": {
                    "type": "object",
                    "properties": {
                        "post_id": {
                            "type": "string",
                            "description": "ID of the post to reply to",
                        },
                        "content": {
                            "type": "string",
                            "description": "Reply body text",
                        },
                        "thread_id": {
                            "type": "string",
                            "description": "Thread ID (optional — resolved automatically if omitted)",
                        },
                    },
                    "required": ["post_id", "content"],
                },
                "requires_approval": True,
                "category": "network",
            },
            {
                "name": "graphchan_list_threads",
                "description": "List available Graphchan threads.",
                "parameters": {"type": "object", "properties": {}},
                "requires_approval": False,
                "category": "network",
            },
            {
                "name": "graphchan_post",
                "description": (
                    "Publish a new top-level post to a Graphchan thread."
                ),
                "parameters": {
                    "type": "object",
                    "properties": {
                        "thread_id": {
                            "type": "string",
                            "description": "Target thread ID",
                        },
                        "body": {
                            "type": "string",
                            "description": "Post body text",
                        },
                        "reply_to_post_id": {
                            "type": "string",
                            "description": "Optional parent post ID",
                        },
                    },
                    "required": ["thread_id", "body"],
                },
                "requires_approval": True,
                "category": "network",
            },
        ],
    }


# ---------------------------------------------------------------------------
# Configure
# ---------------------------------------------------------------------------

def _configure(params: dict[str, Any]) -> dict[str, Any]:
    global _settings, _client
    settings = params.get("settings")
    if settings is None:
        settings = {}
    if not isinstance(settings, dict):
        raise ValueError("plugin.configure expects an object settings payload")

    _settings = dict(DEFAULT_SETTINGS)
    _settings.update(settings)
    api_url = str(_settings.get("api_url") or "").strip()
    agent_name = str(_settings.get("agent_name") or "Ponderer").strip() or "Ponderer"
    if api_url:
        _client = GraphchanClient(base_url=api_url, agent_name=agent_name)
    else:
        _client = None
    return {"configured": True}


# ---------------------------------------------------------------------------
# Event handling
# ---------------------------------------------------------------------------

def _handle_event(params: dict[str, Any]) -> dict[str, Any]:
    event = params.get("event", "")
    if event == "settings_changed":
        plugin_id = params.get("plugin_id", "")
        if plugin_id == "graphchan-orb":
            new_settings = params.get("settings", {})
            _configure({"settings": new_settings})
            return {"state_changed": True, "summary": "Settings reloaded."}
    return {"state_changed": False}


# ---------------------------------------------------------------------------
# Poll events  (plugin.poll_events)
# ---------------------------------------------------------------------------

def _poll_events(_params: dict[str, Any]) -> dict[str, Any]:
    client = _get_client()
    try:
        limit = int(_settings.get("poll_limit", 20))
    except (TypeError, ValueError):
        limit = 20
    limit = max(1, min(limit, 200))
    agent_name = str(_settings.get("agent_name") or "Ponderer").strip()

    try:
        recent = client.get_recent_posts(limit=limit)
    except Exception as exc:
        # Surface as a plugin error so the host can log it
        raise RuntimeError(f"Graphchan poll failed: {exc}") from exc

    events = []
    for item in recent:
        # API wraps posts: {"post": {...}, "thread_title": "..."}
        post = item.get("post", item)
        thread_title = item.get("thread_title", "")

        metadata = post.get("metadata") or {}
        agent_info = metadata.get("agent") or {}
        if agent_name and agent_info.get("name") == agent_name:
            continue

        events.append({
            "id": post.get("id", ""),
            "source": thread_title,
            "author": post.get("author_peer_id") or "Anonymous",
            "body": post.get("body", ""),
            "parent_ids": post.get("parent_post_ids") or [],
        })

    return {"events": events}


# ---------------------------------------------------------------------------
# Tool invocation
# ---------------------------------------------------------------------------

def _invoke_tool(params: dict[str, Any]) -> dict[str, Any]:
    tool = params.get("tool", "")
    args = params.get("arguments") or {}

    if tool == "graphchan_reply":
        return _tool_reply(args)
    if tool == "graphchan_list_threads":
        return _tool_list_threads()
    if tool == "graphchan_post":
        return _tool_post(args)

    raise ValueError(f"Unknown tool: {tool!r}")


def _tool_reply(args: dict[str, Any]) -> dict[str, Any]:
    post_id = (args.get("post_id") or "").strip()
    content = (args.get("content") or "").strip()
    thread_id = (args.get("thread_id") or "").strip() or None

    if not post_id:
        return {"kind": "error", "text": "Missing post_id"}
    if not content:
        return {"kind": "error", "text": "Missing content"}

    client = _get_client()

    if not thread_id:
        thread_id = client.resolve_thread_for_post(post_id)
        if not thread_id:
            return {
                "kind": "error",
                "text": f"Could not resolve thread for post {post_id!r}",
            }

    post = client.create_post(
        thread_id=thread_id,
        body=content,
        parent_post_ids=[post_id],
    )
    return {
        "kind": "json",
        "data": {
            "status": "ok",
            "post_id": post.get("id", "unknown"),
            "thread_id": thread_id,
        },
    }


def _tool_list_threads() -> dict[str, Any]:
    client = _get_client()
    threads = client.list_threads()
    lines = [f"{t.get('id', '?')}: {t.get('title', '(untitled)')}" for t in threads[:10]]
    return {"kind": "text", "text": "\n".join(lines) if lines else "No threads found."}


def _tool_post(args: dict[str, Any]) -> dict[str, Any]:
    thread_id = (args.get("thread_id") or "").strip()
    body = (args.get("body") or "").strip()
    reply_to = (args.get("reply_to_post_id") or "").strip() or None

    if not thread_id:
        return {"kind": "error", "text": "Missing thread_id"}
    if not body:
        return {"kind": "error", "text": "Missing body"}

    client = _get_client()
    parent_ids = [reply_to] if reply_to else []
    post = client.create_post(thread_id=thread_id, body=body, parent_post_ids=parent_ids)
    return {
        "kind": "json",
        "data": {
            "status": "posted",
            "post_id": post.get("id", "unknown"),
            "thread_id": thread_id,
        },
    }


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    raise SystemExit(main())
