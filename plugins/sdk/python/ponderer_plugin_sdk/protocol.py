"""Protocol negotiation and JSON-lines RPC envelopes for Ponderer plugins."""

from __future__ import annotations

import json
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import Any

PROTOCOL_V1 = 1
SUPPORTED_PROTOCOL_VERSIONS = (PROTOCOL_V1,)


class RpcFault(Exception):
    """A stable error code and safe message suitable for an RPC response."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message


@dataclass(frozen=True)
class RpcRequest:
    """A validated host request decoded from one JSON line."""

    request_id: str
    method: str
    params: dict[str, Any]
    protocol_version: int = PROTOCOL_V1
    version_was_explicit: bool = False


@dataclass(frozen=True)
class RpcResponse:
    """A correlated success or error response encoded as one JSON line."""

    request_id: str
    ok: bool
    result: Any = None
    error_code: str | None = None
    error_message: str | None = None
    protocol_version: int = PROTOCOL_V1

    @classmethod
    def success(cls, request_id: str, result: Any) -> "RpcResponse":
        return cls(request_id=request_id, ok=True, result=result)

    @classmethod
    def failure(
        cls, request_id: str, code: str, message: str
    ) -> "RpcResponse":
        return cls(
            request_id=request_id,
            ok=False,
            error_code=code,
            error_message=message,
        )

    def to_wire(self) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "id": self.request_id,
            "protocol_version": self.protocol_version,
            "ok": self.ok,
        }
        if self.ok:
            payload["result"] = self.result
        else:
            payload["error"] = {
                "code": self.error_code or "plugin_error",
                "message": self.error_message or "Plugin request failed",
            }
        return payload


def decode_request_line(line: str) -> RpcRequest:
    """Decode and validate one request, defaulting a legacy envelope to v1."""

    try:
        payload = json.loads(
            line,
            parse_constant=lambda value: (_ for _ in ()).throw(
                ValueError(f"non-standard JSON constant {value}")
            ),
        )
    except (json.JSONDecodeError, ValueError) as exc:
        message = exc.msg if isinstance(exc, json.JSONDecodeError) else str(exc)
        raise RpcFault("invalid_json", f"Invalid JSON request: {message}") from exc
    if not isinstance(payload, Mapping):
        raise RpcFault("invalid_request", "RPC request must be a JSON object")

    raw_id = payload.get("id", "unknown")
    request_id = str(raw_id)
    method = payload.get("method")
    if not isinstance(method, str) or not method.strip():
        raise RpcFault("invalid_request", "RPC request method must be a non-empty string")

    raw_params = payload.get("params")
    if raw_params is None:
        params: dict[str, Any] = {}
    elif isinstance(raw_params, Mapping):
        params = dict(raw_params)
    else:
        raise RpcFault("invalid_request", "RPC request params must be an object")

    explicit_version = "protocol_version" in payload
    raw_version = payload.get("protocol_version", PROTOCOL_V1)
    if isinstance(raw_version, bool) or not isinstance(raw_version, int):
        raise RpcFault("invalid_request", "protocol_version must be an integer")
    if raw_version not in SUPPORTED_PROTOCOL_VERSIONS:
        raise RpcFault(
            "unsupported_protocol",
            f"Unsupported protocol version {raw_version}; supported versions: "
            f"{list(SUPPORTED_PROTOCOL_VERSIONS)}",
        )
    return RpcRequest(
        request_id=request_id,
        method=method,
        params=params,
        protocol_version=raw_version,
        version_was_explicit=explicit_version,
    )


def encode_response_line(response: RpcResponse) -> str:
    """Serialize one response with its terminating newline."""

    return (
        json.dumps(
            response.to_wire(),
            ensure_ascii=False,
            separators=(",", ":"),
            allow_nan=False,
        )
        + "\n"
    )


def negotiate_protocol(params: Mapping[str, Any]) -> int:
    """Select the highest common version, treating an empty legacy offer as v1."""

    offered: Any = params.get("supported_protocol_versions")
    if offered is None:
        # Temporary alias used by early design drafts.
        offered = params.get("protocol_versions")
    if offered is None and "protocol_version" in params:
        offered = [params["protocol_version"]]
    if offered is None:
        return PROTOCOL_V1
    if (
        not isinstance(offered, Sequence)
        or isinstance(offered, (str, bytes, bytearray))
        or any(isinstance(item, bool) or not isinstance(item, int) for item in offered)
    ):
        raise RpcFault(
            "invalid_request", "supported_protocol_versions must be an array of integers"
        )

    common = sorted(set(offered).intersection(SUPPORTED_PROTOCOL_VERSIONS), reverse=True)
    if not common:
        raise RpcFault(
            "unsupported_protocol",
            f"No common protocol version; plugin supports "
            f"{list(SUPPORTED_PROTOCOL_VERSIONS)}",
        )
    return common[0]
