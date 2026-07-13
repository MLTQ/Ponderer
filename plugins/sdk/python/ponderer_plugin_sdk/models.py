"""Typed wire models shared by Ponderer Python plugins."""

from __future__ import annotations

import json
from collections.abc import Mapping
from dataclasses import dataclass, field
from enum import Enum
from typing import Any


class ToolCategory(str, Enum):
    FILE_SYSTEM = "file_system"
    SHELL = "shell"
    NETWORK = "network"
    MEMORY = "memory"
    GENERAL = "general"


class PromptSlot(str, Enum):
    ENGAGED_INSTRUCTIONS = "engaged.instructions"
    ENGAGED_CONTEXT = "engaged.context"
    AMBIENT_INSTRUCTIONS = "ambient.instructions"
    ORIENTATION_CONTEXT = "orientation.context"
    REFLECTION_CONSIDERATIONS = "reflection.considerations"
    PERSONA_EVOLUTION_CONSIDERATIONS = "persona_evolution.considerations"

    @property
    def legacy_wire_name(self) -> str:
        return self.value.replace(".", "_")

    @classmethod
    def from_wire(cls, value: Any) -> "PromptSlot":
        if not isinstance(value, str):
            raise ValueError("prompt slot must be a string")
        for slot in cls:
            if value in (slot.value, slot.legacy_wire_name):
                return slot
        raise ValueError(f"unknown prompt slot: {value!r}")


class PromptKind(str, Enum):
    INSTRUCTION = "instruction"
    CONTEXT = "context"
    CONSTRAINT = "constraint"


@dataclass(frozen=True)
class PluginEffect:
    """A semantic side effect declared by a tool for host policy."""

    effect_id: str
    description: str | None = None
    requires_approval: bool = False

    def __post_init__(self) -> None:
        _require_text(self.effect_id, "effect id")
        if self.description is not None and not isinstance(self.description, str):
            raise TypeError("effect description must be a string")

    def to_wire(self) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "id": self.effect_id,
            "requires_approval": self.requires_approval,
        }
        if self.description is not None:
            payload["description"] = self.description
        return payload

    @classmethod
    def from_wire(cls, value: Any) -> "PluginEffect":
        mapping = _require_mapping(value, "plugin effect")
        return cls(
            effect_id=str(mapping.get("id") or ""),
            description=_optional_text(mapping.get("description")),
            requires_approval=bool(mapping.get("requires_approval", False)),
        )


@dataclass(frozen=True)
class PluginMetadata:
    plugin_id: str
    name: str
    version: str

    def __post_init__(self) -> None:
        _require_text(self.plugin_id, "plugin id")
        _require_text(self.name, "plugin name")
        _require_text(self.version, "plugin version")


@dataclass(frozen=True)
class ToolManifest:
    name: str
    description: str
    parameters: dict[str, Any] = field(
        default_factory=lambda: {"type": "object", "properties": {}}
    )
    requires_approval: bool = False
    category: ToolCategory = ToolCategory.GENERAL
    effects: tuple[PluginEffect, ...] = ()

    def __post_init__(self) -> None:
        _require_text(self.name, "tool name")
        _require_text(self.description, "tool description")
        if not isinstance(self.parameters, dict):
            raise TypeError("tool parameters must be a JSON object")
        if self.parameters.get("type", "object") != "object":
            raise ValueError("tool parameters must describe an object schema")
        if not all(isinstance(effect, PluginEffect) for effect in self.effects):
            raise TypeError("tool effects must contain PluginEffect values")

    def to_wire(self) -> dict[str, Any]:
        category = (
            self.category.value
            if isinstance(self.category, ToolCategory)
            else ToolCategory(self.category).value
        )
        return {
            "name": self.name,
            "description": self.description,
            "parameters": dict(self.parameters),
            "requires_approval": bool(self.requires_approval),
            "category": category,
            "effects": [effect.to_wire() for effect in self.effects],
        }

    @classmethod
    def from_wire(cls, value: Any) -> "ToolManifest":
        mapping = _require_mapping(value, "tool manifest")
        raw_effects = mapping.get("effects") or []
        if not isinstance(raw_effects, list):
            raise TypeError("tool manifest effects must be an array")
        parameters = mapping.get("parameters") or {
            "type": "object",
            "properties": {},
        }
        if not isinstance(parameters, dict):
            raise TypeError("tool manifest parameters must be an object")
        return cls(
            name=str(mapping.get("name") or ""),
            description=str(mapping.get("description") or ""),
            parameters=parameters,
            requires_approval=bool(mapping.get("requires_approval", False)),
            category=ToolCategory(mapping.get("category", "general")),
            effects=tuple(PluginEffect.from_wire(effect) for effect in raw_effects),
        )


@dataclass(frozen=True)
class Capabilities:
    tools: tuple[str, ...] = ()
    event_hooks: tuple[str, ...] = ()
    prompt_slots: tuple[PromptSlot, ...] = ()
    skill_polling: bool = False
    requested_capabilities: tuple[str, ...] = ()

    def to_wire(self) -> dict[str, Any]:
        return {
            "tools": list(self.tools),
            "event_hooks": list(self.event_hooks),
            "prompt_slots": [slot.value for slot in self.prompt_slots],
            "skill_polling": self.skill_polling,
            "requested_capabilities": list(self.requested_capabilities),
        }


@dataclass(frozen=True)
class Handshake:
    metadata: PluginMetadata
    capabilities: Capabilities
    tools: tuple[ToolManifest, ...]
    protocol_version: int

    def to_wire(self) -> dict[str, Any]:
        return {
            "id": self.metadata.plugin_id,
            "name": self.metadata.name,
            "version": self.metadata.version,
            "protocol_version": self.protocol_version,
            "capabilities": self.capabilities.to_wire(),
            "tools": [tool.to_wire() for tool in self.tools],
        }


@dataclass(frozen=True)
class ConfigureResult:
    configured: bool = True

    def to_wire(self) -> dict[str, Any]:
        return {"configured": self.configured}

    @classmethod
    def from_value(cls, value: Any) -> "ConfigureResult":
        if isinstance(value, cls):
            return value
        mapping = _require_mapping(value, "configure result")
        return cls(configured=bool(mapping.get("configured", True)))


@dataclass(frozen=True)
class StateMutation:
    key: str
    value: Any = None
    schema_version: int = 1
    delete: bool = False

    def __post_init__(self) -> None:
        _require_text(self.key, "plugin state key")
        if self.schema_version < 1:
            raise ValueError("plugin state schema_version must be positive")
        if not self.delete:
            try:
                json.dumps(self.value, allow_nan=False)
            except (TypeError, ValueError) as exc:
                raise ValueError("plugin state value must be strict JSON") from exc

    def to_wire(self) -> dict[str, Any]:
        return {
            "key": self.key,
            "schema_version": self.schema_version,
            "value": self.value,
            "delete": self.delete,
        }


@dataclass(frozen=True)
class InvocationContext:
    conversation_id: str | None = None
    loop_name: str | None = None
    username: str = ""
    autonomous: bool = False
    working_directory: str = ""
    invoked_at: str = ""
    deadline_at: str | None = None

    @classmethod
    def from_wire(cls, value: Any) -> "InvocationContext":
        mapping = _require_mapping(value or {}, "tool invocation context")
        return cls(
            conversation_id=_optional_text(mapping.get("conversation_id")),
            loop_name=_optional_text(mapping.get("loop_name")),
            username=str(mapping.get("username") or ""),
            autonomous=bool(mapping.get("autonomous", False)),
            working_directory=str(mapping.get("working_directory") or ""),
            invoked_at=str(mapping.get("invoked_at") or ""),
            deadline_at=_optional_text(mapping.get("deadline_at")),
        )


@dataclass(frozen=True)
class LifecycleEvent:
    name: str
    payload: dict[str, Any]

    @classmethod
    def from_wire(cls, value: Any) -> "LifecycleEvent":
        mapping = _require_mapping(value, "lifecycle event")
        name = mapping.get("event")
        _require_text(name, "lifecycle event name")
        return cls(name=name, payload=dict(mapping))

    def get(self, key: str, default: Any = None) -> Any:
        return self.payload.get(key, default)


@dataclass(frozen=True)
class EventAck:
    state_changed: bool = False
    summary: str | None = None
    acknowledged_event_id: str | None = None
    state_updates: tuple[StateMutation, ...] = ()

    def to_wire(self) -> dict[str, Any]:
        payload: dict[str, Any] = {"state_changed": self.state_changed}
        if self.summary is not None:
            payload["summary"] = self.summary
        if self.acknowledged_event_id is not None:
            payload["acknowledged_event_id"] = self.acknowledged_event_id
        if self.state_updates:
            payload["state_updates"] = [update.to_wire() for update in self.state_updates]
        return payload

    @classmethod
    def from_value(cls, value: Any) -> "EventAck":
        if value is None:
            return cls()
        if isinstance(value, cls):
            return value
        mapping = _require_mapping(value, "event acknowledgement")
        summary = mapping.get("summary")
        if summary is not None and not isinstance(summary, str):
            raise TypeError("event acknowledgement summary must be a string")
        acknowledged_event_id = _optional_text(mapping.get("acknowledged_event_id"))
        raw_updates = mapping.get("state_updates") or []
        if not isinstance(raw_updates, list):
            raise TypeError("event acknowledgement state_updates must be an array")
        updates = tuple(StateMutation(**dict(_require_mapping(item, "state update"))) for item in raw_updates)
        return cls(
            bool(mapping.get("state_changed", False)),
            summary,
            acknowledged_event_id,
            updates,
        )


@dataclass(frozen=True)
class PromptContext:
    conversation_id: str | None = None
    loop_name: str | None = None
    current_summary: str | None = None
    enabled_tools: tuple[str, ...] = ()

    @classmethod
    def from_wire(cls, value: Any) -> "PromptContext":
        mapping = _require_mapping(value or {}, "prompt context")
        raw_tools = mapping.get("enabled_tools") or []
        if not isinstance(raw_tools, list) or not all(
            isinstance(tool, str) for tool in raw_tools
        ):
            raise TypeError("prompt context enabled_tools must be an array of strings")
        return cls(
            conversation_id=_optional_text(mapping.get("conversation_id")),
            loop_name=_optional_text(mapping.get("loop_name")),
            current_summary=_optional_text(mapping.get("current_summary")),
            enabled_tools=tuple(raw_tools),
        )


@dataclass(frozen=True)
class PromptQuery:
    slot: PromptSlot
    context: PromptContext = field(default_factory=PromptContext)
    received_slot_name: str | None = None

    @property
    def uses_legacy_slot_name(self) -> bool:
        return self.received_slot_name == self.slot.legacy_wire_name

    @classmethod
    def from_wire(cls, value: Any) -> "PromptQuery":
        mapping = _require_mapping(value, "prompt query")
        raw_slot = mapping.get("slot")
        slot = PromptSlot.from_wire(raw_slot)
        return cls(
            slot=slot,
            context=PromptContext.from_wire(mapping.get("context") or {}),
            received_slot_name=raw_slot,
        )


@dataclass(frozen=True)
class PromptContribution:
    slot: PromptSlot
    kind: PromptKind
    text: str
    priority: int = 0
    max_chars: int = 300
    plugin_id: str | None = None

    def __post_init__(self) -> None:
        _require_text(self.text, "prompt contribution text")
        if self.max_chars <= 0:
            raise ValueError("prompt contribution max_chars must be positive")

    def to_wire(self, *, legacy_slot_name: bool = False) -> dict[str, Any]:
        slot = self.slot.legacy_wire_name if legacy_slot_name else self.slot.value
        return {
            "plugin_id": self.plugin_id or "",
            "slot": slot,
            "kind": self.kind.value,
            "text": self.text,
            "priority": self.priority,
            "max_chars": self.max_chars,
        }

    @classmethod
    def from_value(cls, value: Any) -> "PromptContribution":
        if isinstance(value, cls):
            return value
        mapping = _require_mapping(value, "prompt contribution")
        return cls(
            plugin_id=_optional_text(mapping.get("plugin_id")),
            slot=PromptSlot.from_wire(mapping.get("slot")),
            kind=PromptKind(mapping.get("kind")),
            text=str(mapping.get("text") or ""),
            priority=int(mapping.get("priority", 0)),
            max_chars=int(mapping.get("max_chars", 300)),
        )


@dataclass(frozen=True)
class PollEvent:
    event_id: str
    source: str
    author: str
    body: str
    parent_ids: tuple[str, ...] = ()

    def __post_init__(self) -> None:
        _require_text(self.event_id, "poll event id")

    def to_wire(self) -> dict[str, Any]:
        return {
            "id": self.event_id,
            "source": self.source,
            "author": self.author,
            "body": self.body,
            "parent_ids": list(self.parent_ids),
        }

    @classmethod
    def from_value(cls, value: Any) -> "PollEvent":
        if isinstance(value, cls):
            return value
        mapping = _require_mapping(value, "poll event")
        raw_parents = mapping.get("parent_ids") or []
        if not isinstance(raw_parents, list) or not all(
            isinstance(parent, str) for parent in raw_parents
        ):
            raise TypeError("poll event parent_ids must be an array of strings")
        return cls(
            event_id=str(mapping.get("id") or ""),
            source=str(mapping.get("source") or ""),
            author=str(mapping.get("author") or ""),
            body=str(mapping.get("body") or ""),
            parent_ids=tuple(raw_parents),
        )


@dataclass(frozen=True)
class ToolResult:
    kind: str
    text_value: str | None = None
    data: Any = None
    state_updates: tuple[StateMutation, ...] = ()

    def __post_init__(self) -> None:
        if self.kind not in ("text", "json", "error"):
            raise ValueError(f"unknown tool result kind: {self.kind!r}")

    @classmethod
    def text(cls, text: str) -> "ToolResult":
        return cls("text", text_value=text)

    @classmethod
    def json(cls, data: Any) -> "ToolResult":
        return cls("json", data=data)

    @classmethod
    def error(cls, message: str) -> "ToolResult":
        return cls("error", text_value=message)

    def to_wire(self) -> dict[str, Any]:
        payload: dict[str, Any] = {"kind": self.kind}
        if self.text_value is not None:
            payload["text"] = self.text_value
        if self.data is not None:
            payload["data"] = self.data
        if self.state_updates:
            payload["state_updates"] = [update.to_wire() for update in self.state_updates]
        return payload

    @classmethod
    def from_value(cls, value: Any) -> "ToolResult":
        if isinstance(value, cls):
            return value
        mapping = _require_mapping(value, "tool result")
        text = mapping.get("text")
        if text is not None and not isinstance(text, str):
            raise TypeError("tool result text must be a string")
        raw_updates = mapping.get("state_updates") or []
        if not isinstance(raw_updates, list):
            raise TypeError("tool result state_updates must be an array")
        updates = tuple(StateMutation(**dict(_require_mapping(item, "state update"))) for item in raw_updates)
        return cls(
            kind=str(mapping.get("kind") or ""),
            text_value=text,
            data=mapping.get("data"),
            state_updates=updates,
        )


def _require_mapping(value: Any, label: str) -> Mapping[str, Any]:
    if not isinstance(value, Mapping):
        raise TypeError(f"{label} must be an object")
    return value


def _require_text(value: Any, label: str) -> None:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{label} must be a non-empty string")


def _optional_text(value: Any) -> str | None:
    if value is None:
        return None
    if not isinstance(value, str):
        raise TypeError("optional text field must be a string")
    return value
