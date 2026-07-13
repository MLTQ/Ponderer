"""Graphchan HTTP client — validated boundary around the Graphchan REST API."""
from __future__ import annotations

from collections.abc import Mapping
from typing import Any
from urllib.parse import quote, urlsplit, urlunsplit

import requests

MAX_IDENTIFIER_BYTES = 512
MAX_ERROR_BODY_CHARS = 512
URL_DELIMITERS = frozenset("/\\?#%")


def normalize_base_url(base_url: str) -> str:
    """Validate and normalize an operator-configured Graphchan API base URL."""

    if not isinstance(base_url, str) or not base_url.strip():
        raise ValueError("Graphchan API URL must be a non-empty string")
    candidate = base_url.strip()
    if "\\" in candidate or _contains_control_character(candidate):
        raise ValueError("Graphchan API URL contains unsupported characters")

    parsed = urlsplit(candidate)
    scheme = parsed.scheme.lower()
    if scheme not in {"http", "https"}:
        raise ValueError("Graphchan API URL must use http or https")
    if not parsed.netloc or parsed.hostname is None:
        raise ValueError("Graphchan API URL must include a host")
    if parsed.username is not None or parsed.password is not None:
        raise ValueError("Graphchan API URL must not contain user information")
    if parsed.query or parsed.fragment:
        raise ValueError("Graphchan API URL must not contain a query or fragment")
    try:
        parsed.port
    except ValueError as exc:
        raise ValueError("Graphchan API URL contains an invalid port") from exc

    path = parsed.path.rstrip("/")
    return urlunsplit((scheme, parsed.netloc, path, "", ""))


def encode_path_identifier(identifier: str, label: str) -> str:
    """Encode one opaque Graphchan identifier as exactly one URL path segment."""

    if not isinstance(identifier, str) or not identifier.strip():
        raise ValueError(f"{label} must be a non-empty string")
    if _contains_control_character(identifier):
        raise ValueError(f"{label} contains unsupported control characters")
    if len(identifier.encode("utf-8")) > MAX_IDENTIFIER_BYTES:
        raise ValueError(f"{label} exceeds {MAX_IDENTIFIER_BYTES} UTF-8 bytes")
    if identifier in {".", ".."}:
        raise ValueError(f"{label} must not be a relative path segment")
    if any(character in URL_DELIMITERS for character in identifier):
        raise ValueError(f"{label} contains a URL path delimiter")
    return quote(identifier, safe="")


class GraphchanClient:
    def __init__(self, base_url: str, agent_name: str = "Ponderer") -> None:
        self.base_url = normalize_base_url(base_url)
        self.agent_name = agent_name
        self._session = requests.Session()

    # ------------------------------------------------------------------
    # Read operations
    # ------------------------------------------------------------------

    def list_threads(self) -> list[dict[str, Any]]:
        resp = self._session.get(f"{self.base_url}/threads", timeout=15)
        resp.raise_for_status()
        return _record_list(resp.json(), envelope_key="threads", label="threads")

    def get_thread(self, thread_id: str) -> dict[str, Any]:
        encoded_thread_id = encode_path_identifier(thread_id, "thread_id")
        resp = self._session.get(
            f"{self.base_url}/threads/{encoded_thread_id}", timeout=15
        )
        resp.raise_for_status()
        return _record(resp.json(), envelope_key="thread", label="thread")

    def get_recent_posts(self, limit: int = 20) -> list[dict[str, Any]]:
        resp = self._session.get(
            f"{self.base_url}/posts/recent",
            params={"limit": limit},
            timeout=15,
        )
        resp.raise_for_status()
        return _record_list(resp.json(), envelope_key="posts", label="recent posts")

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
        encoded_thread_id = encode_path_identifier(thread_id, "thread_id")
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
            f"{self.base_url}/threads/{encoded_thread_id}/posts",
            json=payload,
            timeout=30,
        )
        if not resp.ok:
            response_preview = str(resp.text).replace("\r", " ").replace("\n", " ")
            response_preview = response_preview[:MAX_ERROR_BODY_CHARS]
            raise RuntimeError(
                f"Failed to create post: {resp.status_code} — {response_preview}"
            )
        return _record(resp.json(), envelope_key="post", label="created post")

    def resolve_thread_for_post(self, post_id: str, limit: int = 200) -> str | None:
        """Find the thread_id for a given post_id by scanning recent posts."""
        posts = self.get_recent_posts(limit=limit)
        for item in posts:
            post = item.get("post", item)
            if not isinstance(post, Mapping) or post.get("id") != post_id:
                continue
            thread_id = post.get("thread_id")
            if isinstance(thread_id, str) and thread_id.strip():
                return thread_id
        return None


def _record_list(payload: Any, *, envelope_key: str, label: str) -> list[dict[str, Any]]:
    records = payload.get(envelope_key) if isinstance(payload, Mapping) else payload
    if not isinstance(records, list):
        raise ValueError(f"Graphchan {label} response must contain an array")
    return [dict(record) for record in records if isinstance(record, Mapping)]


def _record(payload: Any, *, envelope_key: str, label: str) -> dict[str, Any]:
    record = payload.get(envelope_key, payload) if isinstance(payload, Mapping) else None
    if not isinstance(record, Mapping):
        raise ValueError(f"Graphchan {label} response must contain an object")
    return dict(record)


def _contains_control_character(value: str) -> bool:
    return any(ord(character) < 0x20 or ord(character) == 0x7F for character in value)
