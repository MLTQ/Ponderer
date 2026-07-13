"""Protocol-v1 dispatch and stdio serving for a Ponderer plugin."""

from __future__ import annotations

import json
import sys
import traceback
from collections.abc import Mapping
from typing import Any, TextIO

from .models import (
    ConfigureResult,
    InvocationContext,
    LifecycleEvent,
    PromptQuery,
    ToolResult,
)
from .plugin import Plugin
from .protocol import (
    RpcFault,
    RpcRequest,
    RpcResponse,
    decode_request_line,
    encode_response_line,
    negotiate_protocol,
)


class PluginServer:
    """Turn newline-delimited RPC requests into correlated plugin responses."""

    def __init__(self, plugin: Plugin) -> None:
        self.plugin = plugin
        self._state_updates_attached = False

    def handle_line(self, line: str) -> str:
        """Handle one request line and always return one response line."""

        request_id = _best_effort_request_id(line)
        state_checkpoint = self.plugin.state_checkpoint()
        dispatched = False
        self._state_updates_attached = False
        try:
            request = decode_request_line(line)
            request_id = request.request_id
            result = self.dispatch(request)
            response = RpcResponse.success(request_id, result)
            dispatched = True
        except RpcFault as exc:
            self.plugin.restore_state_checkpoint(state_checkpoint)
            response = RpcResponse.failure(request_id, exc.code, exc.message)
        except Exception:  # Plugin boundary: return safe shape, stay alive.
            self.plugin.restore_state_checkpoint(state_checkpoint)
            traceback.print_exc(file=sys.stderr)
            response = RpcResponse.failure(
                request_id,
                "plugin_error",
                "Plugin handler failed",
            )
        try:
            encoded = encode_response_line(response)
        except (TypeError, ValueError):
            self.plugin.restore_state_checkpoint(state_checkpoint)
            traceback.print_exc(file=sys.stderr)
            return encode_response_line(
                RpcResponse.failure(
                    request_id,
                    "serialization_error",
                    "Plugin returned a value that is not valid JSON",
                )
            )
        if dispatched and self._state_updates_attached:
            self.plugin.commit_state_updates()
        return encoded

    def dispatch(self, request: RpcRequest) -> Any:
        """Route a validated protocol method onto the typed `Plugin` surface."""

        params = request.params
        if request.method == "plugin.handshake":
            selected = negotiate_protocol(params)
            return self.plugin.handshake(selected).to_wire()

        if request.method == "plugin.configure":
            settings = params.get("settings")
            if settings is None:
                settings = {}
            if not isinstance(settings, Mapping):
                raise RpcFault(
                    "invalid_params", "plugin.configure settings must be an object"
                )
            state = params.get("state") or {}
            if not isinstance(state, Mapping):
                raise RpcFault(
                    "invalid_params", "plugin.configure state must be an object"
                )
            self.plugin.restore_state(state)
            payload = ConfigureResult.from_value(self.plugin.configure(settings)).to_wire()
            return self._attach_state_updates(payload)

        if request.method == "plugin.handle_event":
            payload = self.plugin.handle_event(LifecycleEvent.from_wire(params)).to_wire()
            ledger = params.get("ledger")
            if isinstance(ledger, Mapping):
                event_id = ledger.get("event_id")
                if isinstance(event_id, str) and event_id:
                    payload["acknowledged_event_id"] = event_id
            return self._attach_state_updates(payload)

        if request.method == "plugin.get_prompt_contributions":
            query = PromptQuery.from_wire(params)
            contributions = self.plugin.get_prompt_contributions(query)
            return self._attach_state_updates({
                "contributions": [
                    contribution.to_wire(
                        legacy_slot_name=query.uses_legacy_slot_name
                    )
                    for contribution in contributions
                ]
            })

        if request.method == "plugin.poll_events":
            return self._attach_state_updates(
                {"events": [event.to_wire() for event in self.plugin.poll_events()]}
            )

        if request.method == "plugin.invoke_tool":
            tool_name = params.get("tool")
            if not isinstance(tool_name, str) or not tool_name.strip():
                raise RpcFault(
                    "invalid_params", "plugin.invoke_tool tool must be a string"
                )
            arguments = params.get("arguments")
            if arguments is None:
                arguments = {}
            if not isinstance(arguments, Mapping):
                raise RpcFault(
                    "invalid_params", "plugin.invoke_tool arguments must be an object"
                )
            context = InvocationContext.from_wire(params.get("context") or {})
            result = self.plugin.invoke_tool(tool_name, arguments, context)
            return self._attach_state_updates(ToolResult.from_value(result).to_wire())

        raise RpcFault("unknown_method", f"Unknown method: {request.method!r}")

    def _attach_state_updates(self, payload: dict[str, Any]) -> dict[str, Any]:
        self._state_updates_attached = True
        updates = self.plugin.pending_state_updates()
        if updates:
            existing = payload.get("state_updates") or []
            payload["state_updates"] = [
                *existing,
                *(update.to_wire() for update in updates),
            ]
        return payload

    def serve(self, input_stream: TextIO, output_stream: TextIO) -> int:
        """Serve requests until the host closes stdin."""

        for raw_line in input_stream:
            if not raw_line.strip():
                continue
            output_stream.write(self.handle_line(raw_line))
            output_stream.flush()
        return 0


def serve_stdio(plugin: Plugin) -> int:
    """Run a plugin on the process stdin/stdout transport expected by Ponderer."""

    return PluginServer(plugin).serve(sys.stdin, sys.stdout)


def _best_effort_request_id(line: str) -> str:
    try:
        payload = json.loads(line)
    except (json.JSONDecodeError, TypeError):
        return "unknown"
    if isinstance(payload, Mapping):
        return str(payload.get("id", "unknown"))
    return "unknown"
