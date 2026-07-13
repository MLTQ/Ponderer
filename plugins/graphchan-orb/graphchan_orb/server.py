"""Graphchan-Orb process entrypoint using the shared Ponderer SDK server."""

from __future__ import annotations

from ponderer_plugin_sdk import serve_stdio

from .plugin import build_plugin

PLUGIN = build_plugin()


def main() -> int:
    return serve_stdio(PLUGIN)


if __name__ == "__main__":
    raise SystemExit(main())
