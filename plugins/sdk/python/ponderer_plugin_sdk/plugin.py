"""Registration and callback surface for a Ponderer subprocess plugin."""

from __future__ import annotations

from copy import deepcopy
from collections.abc import Callable, Iterable, Mapping
from dataclasses import replace
from types import MappingProxyType
from typing import Any

from .models import (
    Capabilities,
    ConfigureResult,
    EventAck,
    Handshake,
    InvocationContext,
    LifecycleEvent,
    PluginMetadata,
    PollEvent,
    PromptContribution,
    PromptQuery,
    PromptSlot,
    StateMutation,
    ToolManifest,
    ToolResult,
)

ToolHandler = Callable[[dict[str, Any]], ToolResult | Mapping[str, Any]]
EventHandler = Callable[[LifecycleEvent], EventAck | Mapping[str, Any] | None]
PromptProvider = Callable[
    [PromptQuery],
    PromptContribution
    | Mapping[str, Any]
    | Iterable[PromptContribution | Mapping[str, Any]]
    | None,
]
PollHandler = Callable[
    [], PollEvent | Mapping[str, Any] | Iterable[PollEvent | Mapping[str, Any]] | None
]


class Plugin:
    """A synchronous plugin whose declared capabilities are inferred from handlers."""

    def __init__(
        self,
        metadata: PluginMetadata,
        *,
        default_settings: Mapping[str, Any] | None = None,
        requested_capabilities: Iterable[str] = (),
    ) -> None:
        self.metadata = metadata
        self._default_settings = dict(default_settings or {})
        self._settings = dict(self._default_settings)
        self._configured = False
        self._requested_capabilities = _unique_names(
            requested_capabilities, "requested capability"
        )
        self._tools: dict[str, tuple[ToolManifest, ToolHandler]] = {}
        self._event_handlers: dict[str, EventHandler] = {}
        self._prompt_providers: dict[PromptSlot, PromptProvider] = {}
        self._poll_handler: PollHandler | None = None
        self._state: dict[str, Any] = {}
        self._state_schema_versions: dict[str, int] = {}
        self._pending_state_updates: list[StateMutation] = []
        self._invocation_context: InvocationContext | None = None

    @property
    def settings(self) -> Mapping[str, Any]:
        """Expose current settings without allowing accidental in-place mutation."""

        return MappingProxyType(deepcopy(self._settings))

    @property
    def configured(self) -> bool:
        return self._configured

    @property
    def state(self) -> Mapping[str, Any]:
        """Current restored state without allowing accidental in-place mutation."""

        return MappingProxyType(deepcopy(self._state))

    @property
    def invocation_context(self) -> InvocationContext | None:
        """Host scope/time metadata for the currently executing tool callback."""

        return self._invocation_context

    def restore_state(self, snapshot: Mapping[str, Any]) -> None:
        """Replace process state from the host-owned durable snapshot."""

        restored: dict[str, Any] = {}
        versions: dict[str, int] = {}
        for key, raw in snapshot.items():
            if not isinstance(key, str) or not key.strip():
                raise ValueError("plugin state keys must be non-empty strings")
            if not isinstance(raw, Mapping):
                raise TypeError("plugin state values must be objects")
            schema_version = int(raw.get("schema_version", 1))
            if schema_version < 1:
                raise ValueError("plugin state schema versions must be positive")
            restored[key] = deepcopy(raw.get("value"))
            versions[key] = schema_version
        self._state = restored
        self._state_schema_versions = versions
        self._pending_state_updates.clear()

    def set_state(self, key: str, value: Any, *, schema_version: int = 1) -> None:
        """Stage an upsert in this plugin's host-owned namespace."""

        stored_value = deepcopy(value)
        mutation = StateMutation(key, deepcopy(stored_value), schema_version, False)
        self._state[key] = stored_value
        self._state_schema_versions[key] = schema_version
        self._pending_state_updates.append(mutation)

    def delete_state(self, key: str) -> None:
        """Stage deletion of one key in this plugin's host-owned namespace."""

        mutation = StateMutation(key, None, self._state_schema_versions.get(key, 1), True)
        self._state.pop(key, None)
        self._state_schema_versions.pop(key, None)
        self._pending_state_updates.append(mutation)

    def pending_state_updates(self) -> tuple[StateMutation, ...]:
        """Return mutations without consuming them before response serialization."""

        return tuple(self._pending_state_updates)

    def commit_state_updates(self) -> None:
        """Consume mutations after a successful response has been serialized."""

        self._pending_state_updates.clear()

    def state_checkpoint(
        self,
    ) -> tuple[dict[str, Any], dict[str, int], list[StateMutation]]:
        """Capture request-local durable state for failure rollback."""

        return (
            deepcopy(self._state),
            dict(self._state_schema_versions),
            deepcopy(self._pending_state_updates),
        )

    def restore_state_checkpoint(
        self,
        checkpoint: tuple[dict[str, Any], dict[str, int], list[StateMutation]],
    ) -> None:
        """Undo state changes made by a failed or unserializable callback."""

        state, versions, pending = checkpoint
        self._state = state
        self._state_schema_versions = versions
        self._pending_state_updates = pending

    def tool(self, manifest: ToolManifest) -> Callable[[ToolHandler], ToolHandler]:
        """Register a tool handler and return the original decorated function."""

        def register(handler: ToolHandler) -> ToolHandler:
            if manifest.name in self._tools:
                raise ValueError(f"tool already registered: {manifest.name!r}")
            self._tools[manifest.name] = (manifest, handler)
            return handler

        return register

    def on_event(self, event_name: str) -> Callable[[EventHandler], EventHandler]:
        """Register one lifecycle-event handler."""

        if not isinstance(event_name, str) or not event_name.strip():
            raise ValueError("event name must be a non-empty string")

        def register(handler: EventHandler) -> EventHandler:
            if event_name in self._event_handlers:
                raise ValueError(f"event handler already registered: {event_name!r}")
            self._event_handlers[event_name] = handler
            return handler

        return register

    def on_prompt(
        self, slot: PromptSlot | str
    ) -> Callable[[PromptProvider], PromptProvider]:
        """Register one provider for a canonical prompt slot."""

        normalized_slot = slot if isinstance(slot, PromptSlot) else PromptSlot.from_wire(slot)

        def register(provider: PromptProvider) -> PromptProvider:
            if normalized_slot in self._prompt_providers:
                raise ValueError(
                    f"prompt provider already registered: {normalized_slot.value!r}"
                )
            self._prompt_providers[normalized_slot] = provider
            return provider

        return register

    def on_poll(self, handler: PollHandler) -> PollHandler:
        """Register the plugin's sole external-event poller."""

        if self._poll_handler is not None:
            raise ValueError("poll handler already registered")
        self._poll_handler = handler
        return handler

    def handshake(self, protocol_version: int) -> Handshake:
        """Build capabilities from handlers so declarations cannot silently drift."""

        manifests = tuple(manifest for manifest, _handler in self._tools.values())
        capabilities = Capabilities(
            tools=tuple(self._tools),
            event_hooks=tuple(self._event_handlers),
            prompt_slots=tuple(self._prompt_providers),
            skill_polling=self._poll_handler is not None,
            requested_capabilities=self._requested_capabilities,
        )
        return Handshake(
            metadata=self.metadata,
            capabilities=capabilities,
            tools=manifests,
            protocol_version=protocol_version,
        )

    def configure(self, settings: Mapping[str, Any]) -> ConfigureResult:
        """Replace settings with defaults plus the host payload.

        Domain plugins may override this method for validation/resource reloads,
        but should call ``super().configure(settings)`` first.
        """

        self._settings = dict(self._default_settings)
        self._settings.update(settings)
        self._configured = True
        return ConfigureResult()

    def handle_event(self, event: LifecycleEvent) -> EventAck:
        handler = self._event_handlers.get(event.name)
        if handler is None:
            return EventAck()
        return EventAck.from_value(handler(event))

    def get_prompt_contributions(
        self, query: PromptQuery
    ) -> tuple[PromptContribution, ...]:
        provider = self._prompt_providers.get(query.slot)
        if provider is None:
            return ()
        contributions = tuple(
            PromptContribution.from_value(value)
            for value in _as_values(provider(query), "prompt provider")
        )
        normalized: list[PromptContribution] = []
        for contribution in contributions:
            if contribution.slot != query.slot:
                raise ValueError(
                    "prompt provider returned a contribution for "
                    f"{contribution.slot.value!r} while handling {query.slot.value!r}"
                )
            normalized.append(
                replace(contribution, plugin_id=self.metadata.plugin_id)
            )
        return tuple(normalized)

    def poll_events(self) -> tuple[PollEvent, ...]:
        if self._poll_handler is None:
            return ()
        return tuple(
            PollEvent.from_value(value)
            for value in _as_values(self._poll_handler(), "poll handler")
        )

    def invoke_tool(
        self,
        tool_name: str,
        arguments: Mapping[str, Any],
        context: InvocationContext | None = None,
    ) -> ToolResult:
        registered = self._tools.get(tool_name)
        if registered is None:
            raise ValueError(f"unknown tool: {tool_name!r}")
        _manifest, handler = registered
        self._invocation_context = context
        try:
            return ToolResult.from_value(handler(dict(arguments)))
        finally:
            self._invocation_context = None


def _as_values(value: Any, label: str) -> tuple[Any, ...]:
    if value is None:
        return ()
    if isinstance(value, Mapping) or isinstance(value, (PromptContribution, PollEvent)):
        return (value,)
    if isinstance(value, Iterable) and not isinstance(value, (str, bytes, bytearray)):
        return tuple(value)
    raise TypeError(f"{label} must return a wire model, object, iterable, or None")


def _unique_names(values: Iterable[str], label: str) -> tuple[str, ...]:
    names: list[str] = []
    for value in values:
        if not isinstance(value, str) or not value.strip():
            raise ValueError(f"{label} must be a non-empty string")
        if value in names:
            raise ValueError(f"duplicate {label}: {value!r}")
        names.append(value)
    return tuple(names)
