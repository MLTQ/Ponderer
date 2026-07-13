"""Loading for the static per-tool contract shared by package and runtime."""

from __future__ import annotations

import json
from pathlib import Path

from .models import ToolManifest


def load_tool_contract(path: str | Path) -> dict[str, ToolManifest]:
    """Load and validate a canonical ``{"tools": [...]}`` contract file."""

    contract_path = Path(path)
    raw = json.loads(contract_path.read_text(encoding="utf-8"))
    if not isinstance(raw, dict) or not isinstance(raw.get("tools"), list):
        raise ValueError("tool contract must be an object containing a tools array")
    tools: dict[str, ToolManifest] = {}
    for value in raw["tools"]:
        manifest = ToolManifest.from_wire(value)
        if manifest.name in tools:
            raise ValueError(f"duplicate tool contract name: {manifest.name!r}")
        tools[manifest.name] = manifest
    return tools
