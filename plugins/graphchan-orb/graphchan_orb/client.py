"""Graphchan HTTP client — thin wrapper around the Graphchan REST API."""
from __future__ import annotations

from typing import Any

import requests


class GraphchanClient:
    def __init__(self, base_url: str, agent_name: str = "Ponderer") -> None:
        self.base_url = base_url.rstrip("/")
        self.agent_name = agent_name
        self._session = requests.Session()

    # ------------------------------------------------------------------
    # Read operations
    # ------------------------------------------------------------------

    def list_threads(self) -> list[dict[str, Any]]:
        resp = self._session.get(f"{self.base_url}/threads", timeout=15)
        resp.raise_for_status()
        return resp.json()

    def get_thread(self, thread_id: str) -> dict[str, Any]:
        resp = self._session.get(f"{self.base_url}/threads/{thread_id}", timeout=15)
        resp.raise_for_status()
        return resp.json()

    def get_recent_posts(self, limit: int = 20) -> list[dict[str, Any]]:
        resp = self._session.get(
            f"{self.base_url}/posts/recent",
            params={"limit": limit},
            timeout=15,
        )
        resp.raise_for_status()
        data = resp.json()
        # API returns {"posts": [...]}
        return data.get("posts", data) if isinstance(data, dict) else data

    def health_check(self) -> bool:
        try:
            resp = self._session.get(f"{self.base_url}/threads", timeout=5)
            return resp.ok
        except Exception:
            return False

    # ------------------------------------------------------------------
    # Write operations
    # ------------------------------------------------------------------

    def create_post(
        self,
        thread_id: str,
        body: str,
        parent_post_ids: list[str] | None = None,
        agent_name: str | None = None,
    ) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "thread_id": thread_id,
            "author_peer_id": None,
            "body": body,
            "parent_post_ids": parent_post_ids or [],
            "metadata": {
                "agent": {"name": agent_name or self.agent_name, "version": None},
                "client": "ponderer",
            },
        }
        resp = self._session.post(
            f"{self.base_url}/threads/{thread_id}/posts",
            json=payload,
            timeout=30,
        )
        if not resp.ok:
            raise RuntimeError(
                f"Failed to create post: {resp.status_code} — {resp.text}"
            )
        return resp.json().get("post", resp.json())

    def resolve_thread_for_post(self, post_id: str, limit: int = 200) -> str | None:
        """Find the thread_id for a given post_id by scanning recent posts."""
        posts = self.get_recent_posts(limit=limit)
        for item in posts:
            post = item.get("post", item)
            if post.get("id") == post_id:
                return post.get("thread_id")
        return None
