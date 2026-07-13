from __future__ import annotations

import unittest
from typing import Any

from graphchan_orb.client import GraphchanClient, normalize_base_url


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
    def test_base_url_requires_safe_http_origin(self) -> None:
        self.assertEqual(
            normalize_base_url(" HTTPS://graphchan.test/api/ "),
            "https://graphchan.test/api",
        )
        invalid_urls = (
            "file:///tmp/graphchan",
            "http://",
            "http://user:secret@graphchan.test",
            "http://graphchan.test?admin=true",
            "http://graphchan.test/#fragment",
            "http://graphchan.test\\evil",
            "http://graphchan.test\n.example",
            "http://graphchan.test:invalid",
        )
        for invalid_url in invalid_urls:
            with self.subTest(invalid_url=invalid_url):
                with self.assertRaises(ValueError):
                    GraphchanClient(invalid_url)

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

    def test_dynamic_thread_ids_are_single_encoded_path_segments(self) -> None:
        client = GraphchanClient("http://graphchan.test/api")
        fake = FakeSession([FakeResponse({"post": {"id": "created"}})])
        client._session = fake  # type: ignore[assignment]

        client.create_post("thread:name with space", "hello")

        self.assertEqual(
            fake.calls[0][1],
            "http://graphchan.test/api/threads/thread%3Aname%20with%20space/posts",
        )
        for invalid_id in (
            "",
            "   ",
            ".",
            "..",
            "thread\nheader",
            "../../threads/root",
            "thread?admin=true",
            "thread#fragment",
            "thread%2Fencoded",
        ):
            with self.subTest(invalid_id=invalid_id):
                with self.assertRaises(ValueError):
                    client.create_post(invalid_id, "hello")

    def test_get_thread_encodes_reserved_identifier_characters(self) -> None:
        client = GraphchanClient("http://graphchan.test")
        fake = FakeSession([FakeResponse({"thread": {"id": "a:b c"}})])
        client._session = fake  # type: ignore[assignment]

        self.assertEqual(client.get_thread("a:b c"), {"id": "a:b c"})
        self.assertEqual(
            fake.calls[0][1], "http://graphchan.test/threads/a%3Ab%20c"
        )

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

    def test_response_arrays_drop_malformed_members_but_reject_bad_envelopes(self) -> None:
        client = GraphchanClient("http://graphchan.test")
        fake = FakeSession(
            [
                FakeResponse({"posts": [None, "bad", {"post": {"id": "ok"}}]}),
                FakeResponse({"posts": "not-an-array"}),
            ]
        )
        client._session = fake  # type: ignore[assignment]

        self.assertEqual(client.get_recent_posts(), [{"post": {"id": "ok"}}])
        with self.assertRaisesRegex(ValueError, "must contain an array"):
            client.get_recent_posts()


if __name__ == "__main__":
    unittest.main()
