from __future__ import annotations

import unittest
from typing import Any

from graphchan_orb.client import GraphchanClient


class FakeResponse:
    def __init__(
        self,
        payload: Any,
        *,
        ok: bool = True,
        status_code: int = 200,
        text: str = "",
    ) -> None:
        self.payload = payload
        self.ok = ok
        self.status_code = status_code
        self.text = text

    def raise_for_status(self) -> None:
        if not self.ok:
            raise RuntimeError(f"HTTP {self.status_code}")

    def json(self) -> Any:
        return self.payload


class FakeSession:
    def __init__(self, responses: list[FakeResponse]) -> None:
        self.responses = list(responses)
        self.calls: list[tuple[str, str, dict[str, Any]]] = []

    def get(self, url: str, **kwargs: Any) -> FakeResponse:
        self.calls.append(("GET", url, kwargs))
        return self.responses.pop(0)

    def post(self, url: str, **kwargs: Any) -> FakeResponse:
        self.calls.append(("POST", url, kwargs))
        return self.responses.pop(0)


class GraphchanClientTests(unittest.TestCase):
    def test_recent_posts_unwraps_response_and_uses_bounded_request_shape(self) -> None:
        client = GraphchanClient("http://graphchan.test/")
        fake = FakeSession([FakeResponse({"posts": [{"post": {"id": "p1"}}]})])
        client._session = fake  # type: ignore[assignment]

        posts = client.get_recent_posts(limit=17)

        self.assertEqual(posts, [{"post": {"id": "p1"}}])
        self.assertEqual(
            fake.calls,
            [
                (
                    "GET",
                    "http://graphchan.test/posts/recent",
                    {"params": {"limit": 17}, "timeout": 15},
                )
            ],
        )

    def test_create_post_attributes_agent_and_unwraps_post(self) -> None:
        client = GraphchanClient("http://graphchan.test", agent_name="Colombina")
        fake = FakeSession([FakeResponse({"post": {"id": "created"}})])
        client._session = fake  # type: ignore[assignment]

        post = client.create_post("thread-1", "hello", ["parent-1"])

        self.assertEqual(post, {"id": "created"})
        method, url, kwargs = fake.calls[0]
        self.assertEqual((method, url), ("POST", "http://graphchan.test/threads/thread-1/posts"))
        self.assertEqual(kwargs["timeout"], 30)
        self.assertEqual(kwargs["json"]["parent_post_ids"], ["parent-1"])
        self.assertEqual(kwargs["json"]["metadata"]["agent"]["name"], "Colombina")

    def test_create_post_surfaces_api_error_without_network(self) -> None:
        client = GraphchanClient("http://graphchan.test")
        client._session = FakeSession(  # type: ignore[assignment]
            [FakeResponse({}, ok=False, status_code=403, text="denied")]
        )

        with self.assertRaisesRegex(RuntimeError, "403.*denied"):
            client.create_post("thread-1", "hello")

    def test_resolve_thread_scans_wrapped_recent_posts(self) -> None:
        client = GraphchanClient("http://graphchan.test")
        client._session = FakeSession(  # type: ignore[assignment]
            [
                FakeResponse(
                    {
                        "posts": [
                            {"post": {"id": "other", "thread_id": "thread-1"}},
                            {"post": {"id": "target", "thread_id": "thread-2"}},
                        ]
                    }
                )
            ]
        )

        self.assertEqual(client.resolve_thread_for_post("target"), "thread-2")


if __name__ == "__main__":
    unittest.main()
