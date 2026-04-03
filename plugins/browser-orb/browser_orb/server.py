from __future__ import annotations

import ipaddress
import json
import os
import shutil
import socket
import subprocess
import sys
from dataclasses import dataclass, field
from datetime import datetime, timezone
from fnmatch import fnmatch
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

from browser_orb import __version__

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SCREENSHOT_DIR = "./data/screenshots"
DEFAULT_SNAPSHOT_DEPTH = 6
FORBIDDEN_SCHEMES = {"javascript", "data"}
PUBLIC_WEB_SCHEMES = {"http", "https"}
UNRESTRICTED_EXTRA_SCHEMES = {"file", "about"}


@dataclass
class PluginState:
    settings: dict[str, Any] = field(default_factory=dict)
    instance_id: str = field(
        default_factory=lambda: (
            f"browser-orb-{os.getpid()}-"
            f"{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')}"
        )
    )
    startup_utc: str = field(default_factory=lambda: datetime.now(timezone.utc).isoformat())
    ephemeral_session_id: str = field(
        default_factory=lambda: (
            f"browser-orb-session-{os.getpid()}-"
            f"{datetime.now(timezone.utc).strftime('%H%M%S%f')}"
        )
    )
    last_known_url: str | None = None

    def merged_settings(self) -> dict[str, Any]:
        merged = {
            "enabled": False,
            "agent_browser_command": "agent-browser",
            "allow_unrestricted_navigation": False,
            "allowed_domains": "",
            "allow_eval": False,
            "allow_persistent_auth": False,
            "persistent_auth_session_name": "browser-orb",
            "persistent_auth_encryption_key": "",
            "headed": False,
            "default_snapshot_interactive_only": True,
            "default_snapshot_compact": True,
            "default_snapshot_depth": DEFAULT_SNAPSHOT_DEPTH,
            "screenshot_dir": DEFAULT_SCREENSHOT_DIR,
        }
        merged.update(self.settings)
        return merged


STATE = PluginState()


def main() -> int:
    for raw_line in sys.stdin:
        line = raw_line.strip()
        if not line:
            continue

        response = handle_rpc_line(line)
        sys.stdout.write(json.dumps(response, ensure_ascii=False) + "\n")
        sys.stdout.flush()

    return 0


def handle_rpc_line(line: str) -> dict[str, Any]:
    request_id = "unknown"
    try:
        payload = json.loads(line)
        request_id = str(payload.get("id", "unknown"))
        method = payload.get("method")
        params = payload.get("params") or {}
        result = dispatch(method, params)
        return {"id": request_id, "ok": True, "result": result}
    except Exception as exc:  # pragma: no cover - runtime surface
        return {
            "id": request_id,
            "ok": False,
            "error": {
                "code": "plugin_error",
                "message": str(exc),
            },
        }


def dispatch(method: str, params: dict[str, Any]) -> Any:
    if method == "plugin.handshake":
        return handshake()
    if method == "plugin.configure":
        return configure(params)
    if method == "plugin.handle_event":
        return handle_event(params)
    if method == "plugin.get_prompt_contributions":
        return get_prompt_contributions(params)
    if method == "plugin.invoke_tool":
        return invoke_tool(params)
    raise ValueError(f"unknown method: {method}")


def handshake() -> dict[str, Any]:
    return {
        "id": "browser-orb",
        "name": "Browser-Orb",
        "version": __version__,
        "capabilities": {
            "tools": [
                "browser_open",
                "browser_snapshot",
                "browser_click",
                "browser_fill",
                "browser_wait",
                "browser_get_text",
                "browser_screenshot",
                "browser_close",
                "browser_eval",
            ],
            "event_hooks": ["settings_changed"],
            "prompt_slots": ["engaged.instructions"],
        },
        "tools": [
            tool_manifest(
                "browser_open",
                "Open a live website in Browser-Orb.",
                {
                    "type": "object",
                    "properties": {
                        "url": {"type": "string", "description": "URL to open."},
                        "wait_until": {
                            "type": "string",
                            "description": "Optional load state to wait for after opening: load, domcontentloaded, or networkidle.",
                        },
                    },
                    "required": ["url"],
                },
                requires_approval=True,
                category="network",
            ),
            tool_manifest(
                "browser_snapshot",
                "Capture an accessibility-tree snapshot with stable refs for later browser interactions.",
                {
                    "type": "object",
                    "properties": {
                        "interactive_only": {"type": "boolean"},
                        "compact": {"type": "boolean"},
                        "depth": {"type": "integer"},
                        "selector": {"type": "string"},
                    },
                },
                requires_approval=False,
                category="network",
            ),
            tool_manifest(
                "browser_click",
                "Click an element by ref or selector in the active browser session.",
                {
                    "type": "object",
                    "properties": {
                        "target": {"type": "string", "description": "Element ref or selector."},
                        "wait_until": {
                            "type": "string",
                            "description": "Optional load state to wait for after the click.",
                        },
                    },
                    "required": ["target"],
                },
                requires_approval=True,
                category="network",
            ),
            tool_manifest(
                "browser_fill",
                "Fill an input field by ref or selector in the active browser session.",
                {
                    "type": "object",
                    "properties": {
                        "target": {"type": "string", "description": "Element ref or selector."},
                        "text": {"type": "string", "description": "Text to fill."},
                    },
                    "required": ["target", "text"],
                },
                requires_approval=True,
                category="network",
            ),
            tool_manifest(
                "browser_wait",
                "Wait for a selector, text, URL pattern, load state, JavaScript condition, or a fixed duration.",
                {
                    "type": "object",
                    "properties": {
                        "selector": {"type": "string"},
                        "state": {"type": "string", "description": "visible or hidden when waiting on a selector."},
                        "text": {"type": "string"},
                        "url_pattern": {"type": "string"},
                        "load_state": {"type": "string"},
                        "function_js": {"type": "string"},
                        "milliseconds": {"type": "integer"},
                    },
                },
                requires_approval=False,
                category="network",
            ),
            tool_manifest(
                "browser_get_text",
                "Get text content from an element by ref or selector in the active browser session.",
                {
                    "type": "object",
                    "properties": {
                        "target": {"type": "string", "description": "Element ref or selector."},
                    },
                    "required": ["target"],
                },
                requires_approval=False,
                category="network",
            ),
            tool_manifest(
                "browser_screenshot",
                "Capture a screenshot of the active browser page and return media metadata.",
                {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "full_page": {"type": "boolean"},
                        "annotate": {"type": "boolean"},
                    },
                },
                requires_approval=False,
                category="network",
            ),
            tool_manifest(
                "browser_close",
                "Close the current Browser-Orb browser session.",
                {
                    "type": "object",
                    "properties": {
                        "close_all": {"type": "boolean"},
                    },
                },
                requires_approval=False,
                category="network",
            ),
            tool_manifest(
                "browser_eval",
                "Run JavaScript inside the active page. Disabled unless the plugin setting allows it.",
                {
                    "type": "object",
                    "properties": {
                        "js": {"type": "string", "description": "JavaScript source to run in the active page."},
                    },
                    "required": ["js"],
                },
                requires_approval=True,
                category="network",
            ),
        ],
    }


def tool_manifest(
    name: str,
    description: str,
    parameters: dict[str, Any],
    *,
    requires_approval: bool,
    category: str,
) -> dict[str, Any]:
    return {
        "name": name,
        "description": description,
        "parameters": parameters,
        "requires_approval": requires_approval,
        "category": category,
    }


def configure(params: dict[str, Any]) -> dict[str, Any]:
    settings = params.get("settings")
    if settings is None:
        settings = {}
    if not isinstance(settings, dict):
        raise ValueError("plugin.configure expects an object settings payload")

    STATE.settings = dict(settings)
    return {"configured": True}


def handle_event(params: dict[str, Any]) -> dict[str, Any]:
    event_name = params.get("event")
    if event_name == "settings_changed":
        return {"state_changed": False, "summary": "Browser-Orb settings reloaded."}
    return {"state_changed": False}


def get_prompt_contributions(params: dict[str, Any]) -> dict[str, Any]:
    slot = params.get("slot")
    if slot not in ("engaged_instructions", "engaged.instructions"):
        return {"contributions": []}

    settings = STATE.merged_settings()
    domain_hint = format_domain_hint(parse_domain_allowlist(settings))
    eval_hint = (
        "browser_eval is enabled."
        if parse_bool(settings.get("allow_eval"), False)
        else "browser_eval is disabled in plugin settings."
    )
    auth_hint = (
        "Persistent auth is enabled for this plugin."
        if parse_bool(settings.get("allow_persistent_auth"), False)
        else "Persistent auth is disabled by default."
    )
    nav_hint = (
        "Navigation is unrestricted."
        if parse_bool(settings.get("allow_unrestricted_navigation"), False)
        else "Navigation is restricted to public-web URLs and any configured domain allowlist."
    )

    return {
        "contributions": [
            {
                "plugin_id": "browser-orb",
                "slot": "engaged_instructions",
                "kind": "instruction",
                "text": (
                    "When a task needs a live browser, use Browser-Orb tools. "
                    "Typical flow: `browser_open`, then `browser_snapshot`, then interact using refs from the snapshot. "
                    f"{nav_hint} {domain_hint} {eval_hint} {auth_hint}"
                ),
                "priority": 50,
                "max_chars": 420,
            }
        ]
    }


def invoke_tool(params: dict[str, Any]) -> dict[str, Any]:
    tool_name = params.get("tool")
    arguments = params.get("arguments") or {}
    if not isinstance(arguments, dict):
        raise ValueError("plugin.invoke_tool arguments must be an object")

    if tool_name == "browser_open":
        return browser_open(arguments)
    if tool_name == "browser_snapshot":
        return browser_snapshot(arguments)
    if tool_name == "browser_click":
        return browser_click(arguments)
    if tool_name == "browser_fill":
        return browser_fill(arguments)
    if tool_name == "browser_wait":
        return browser_wait(arguments)
    if tool_name == "browser_get_text":
        return browser_get_text(arguments)
    if tool_name == "browser_screenshot":
        return browser_screenshot(arguments)
    if tool_name == "browser_close":
        return browser_close(arguments)
    if tool_name == "browser_eval":
        return browser_eval(arguments)

    raise ValueError(f"unknown tool: {tool_name}")


def browser_open(arguments: dict[str, Any]) -> dict[str, Any]:
    settings = STATE.merged_settings()
    url = require_string(arguments, "url")
    validate_navigation_target(url, settings)

    run_agent_browser(["open", url], settings)
    wait_until = str(arguments.get("wait_until") or "").strip()
    if wait_until:
        run_agent_browser(["wait", "--load", wait_until], settings)

    url_info = refresh_current_url(settings)
    title_info = run_agent_browser(["get", "title"], settings)
    return json_result(
        {
            "status": "ok",
            "tool": "browser_open",
            "requested_url": url,
            "current_url": url_info,
            "title": extract_scalar(title_info),
            "session_id": STATE.ephemeral_session_id,
            "persistent_auth_enabled": parse_bool(
                settings.get("allow_persistent_auth"), False
            ),
        }
    )


def browser_snapshot(arguments: dict[str, Any]) -> dict[str, Any]:
    settings = STATE.merged_settings()
    interactive_only = parse_bool(
        arguments.get("interactive_only"),
        parse_bool(settings.get("default_snapshot_interactive_only"), True),
    )
    compact = parse_bool(
        arguments.get("compact"),
        parse_bool(settings.get("default_snapshot_compact"), True),
    )
    depth = parse_int(
        arguments.get("depth"),
        parse_int(settings.get("default_snapshot_depth"), DEFAULT_SNAPSHOT_DEPTH),
    )
    selector = str(arguments.get("selector") or "").strip()

    command = ["snapshot"]
    if interactive_only:
        command.append("-i")
    if compact:
        command.append("-c")
    if depth > 0:
        command.extend(["-d", str(depth)])
    if selector:
        command.extend(["-s", selector])

    result = run_agent_browser(command, settings)
    data = extract_data(result)
    return json_result(
        {
            "status": "ok",
            "tool": "browser_snapshot",
            "current_url": refresh_current_url(settings),
            "snapshot": data,
        }
    )


def browser_click(arguments: dict[str, Any]) -> dict[str, Any]:
    settings = STATE.merged_settings()
    target = require_string(arguments, "target")
    run_agent_browser(["click", target], settings)
    wait_until = str(arguments.get("wait_until") or "").strip()
    if wait_until:
        run_agent_browser(["wait", "--load", wait_until], settings)
    return json_result(
        {
            "status": "ok",
            "tool": "browser_click",
            "target": target,
            "current_url": refresh_current_url(settings),
        }
    )


def browser_fill(arguments: dict[str, Any]) -> dict[str, Any]:
    settings = STATE.merged_settings()
    target = require_string(arguments, "target")
    text = require_string(arguments, "text")
    run_agent_browser(["fill", target, text], settings)
    return json_result(
        {
            "status": "ok",
            "tool": "browser_fill",
            "target": target,
            "text_length": len(text),
            "current_url": refresh_current_url(settings),
        }
    )


def browser_wait(arguments: dict[str, Any]) -> dict[str, Any]:
    settings = STATE.merged_settings()
    selector = str(arguments.get("selector") or "").strip()
    state = str(arguments.get("state") or "").strip()
    text = str(arguments.get("text") or "").strip()
    url_pattern = str(arguments.get("url_pattern") or "").strip()
    load_state = str(arguments.get("load_state") or "").strip()
    function_js = str(arguments.get("function_js") or "").strip()
    milliseconds = parse_int(arguments.get("milliseconds"), 0)

    if selector:
        command = ["wait", selector]
        if state:
            command.extend(["--state", state])
    elif text:
        command = ["wait", "--text", text]
    elif url_pattern:
        command = ["wait", "--url", url_pattern]
    elif load_state:
        command = ["wait", "--load", load_state]
    elif function_js:
        command = ["wait", "--fn", function_js]
    elif milliseconds > 0:
        command = ["wait", str(milliseconds)]
    else:
        raise ValueError(
            "browser_wait requires one of selector, text, url_pattern, load_state, function_js, or milliseconds"
        )

    run_agent_browser(command, settings)
    return json_result(
        {
            "status": "ok",
            "tool": "browser_wait",
            "current_url": refresh_current_url(settings),
        }
    )


def browser_get_text(arguments: dict[str, Any]) -> dict[str, Any]:
    settings = STATE.merged_settings()
    target = require_string(arguments, "target")
    result = run_agent_browser(["get", "text", target], settings)
    return json_result(
        {
            "status": "ok",
            "tool": "browser_get_text",
            "target": target,
            "current_url": refresh_current_url(settings),
            "text": extract_scalar(result),
        }
    )


def browser_screenshot(arguments: dict[str, Any]) -> dict[str, Any]:
    settings = STATE.merged_settings()
    output_path = resolve_output_path(
        str(arguments.get("path") or "").strip(),
        str(settings.get("screenshot_dir") or DEFAULT_SCREENSHOT_DIR),
    )
    command = ["screenshot", str(output_path)]
    if parse_bool(arguments.get("full_page"), False):
        command.append("--full")
    if parse_bool(arguments.get("annotate"), False):
        command.append("--annotate")

    run_agent_browser(command, settings)
    return json_result(
        {
            "status": "ok",
            "tool": "browser_screenshot",
            "path": str(output_path),
            "current_url": refresh_current_url(settings),
            "media": [
                {
                    "filename": output_path.name,
                    "path": str(output_path),
                    "media_kind": "image",
                    "mime_type": "image/png",
                    "source": "browser-orb",
                }
            ],
        }
    )


def browser_close(arguments: dict[str, Any]) -> dict[str, Any]:
    settings = STATE.merged_settings()
    close_all = parse_bool(arguments.get("close_all"), False)
    command = ["close", "--all"] if close_all else ["close"]
    run_agent_browser(command, settings)
    if not close_all:
        STATE.last_known_url = None
    return json_result(
        {
            "status": "ok",
            "tool": "browser_close",
            "closed_all": close_all,
        }
    )


def browser_eval(arguments: dict[str, Any]) -> dict[str, Any]:
    settings = STATE.merged_settings()
    if not parse_bool(settings.get("allow_eval"), False):
        raise RuntimeError(
            "Browser-Orb browser_eval is disabled. Enable 'Allow JavaScript Eval' in Browser-Orb settings to use it."
        )

    js = require_string(arguments, "js")
    result = run_agent_browser(["eval", js], settings)
    return json_result(
        {
            "status": "ok",
            "tool": "browser_eval",
            "current_url": refresh_current_url(settings),
            "result": extract_data(result),
        }
    )


def run_agent_browser(
    command_parts: list[str],
    settings: dict[str, Any],
    *,
    timeout_secs: int = 120,
) -> dict[str, Any]:
    binary = str(settings.get("agent_browser_command") or "agent-browser").strip()
    if not binary:
        raise RuntimeError("Browser-Orb agent_browser_command is empty.")
    resolved_binary = shutil.which(binary) if not os.path.isabs(binary) else binary
    if not resolved_binary:
        raise RuntimeError(
            f"Browser-Orb could not find '{binary}'. Install agent-browser or point Browser-Orb at the correct executable."
        )

    base_command = [resolved_binary, "--json"]
    if parse_bool(settings.get("headed"), False):
        base_command.append("--headed")

    allowed_domains = parse_domain_allowlist(settings)
    if (
        allowed_domains
        and not parse_bool(settings.get("allow_unrestricted_navigation"), False)
    ):
        base_command.extend(["--allowed-domains", ",".join(allowed_domains)])

    full_command = base_command + command_parts
    env = os.environ.copy()
    env["AGENT_BROWSER_SESSION"] = STATE.ephemeral_session_id

    if parse_bool(settings.get("allow_persistent_auth"), False):
        session_name = str(
            settings.get("persistent_auth_session_name") or "browser-orb"
        ).strip()
        if session_name:
            env["AGENT_BROWSER_SESSION_NAME"] = session_name
        encryption_key = str(
            settings.get("persistent_auth_encryption_key") or ""
        ).strip()
        if encryption_key:
            env["AGENT_BROWSER_ENCRYPTION_KEY"] = encryption_key

    completed = subprocess.run(
        full_command,
        cwd=str(REPO_ROOT),
        env=env,
        text=True,
        capture_output=True,
        timeout=timeout_secs,
    )
    stdout = completed.stdout.strip()
    stderr = completed.stderr.strip()

    if completed.returncode != 0:
        detail = stderr or stdout or f"exit code {completed.returncode}"
        raise RuntimeError(f"agent-browser command failed: {detail}")

    payload = parse_agent_browser_json(stdout)
    success = payload.get("success", True)
    if not success:
        detail = payload.get("error") or payload.get("message") or payload
        raise RuntimeError(f"agent-browser reported failure: {detail}")
    if stderr:
        payload.setdefault("_stderr", stderr)
    return payload


def parse_agent_browser_json(stdout: str) -> dict[str, Any]:
    if not stdout:
        raise RuntimeError("agent-browser returned no stdout.")

    candidates = [stdout]
    candidates.extend(
        line.strip() for line in reversed(stdout.splitlines()) if line.strip()
    )

    for candidate in candidates:
        try:
            parsed = json.loads(candidate)
        except json.JSONDecodeError:
            continue
        if isinstance(parsed, dict):
            return parsed

    raise RuntimeError(
        "Browser-Orb could not parse agent-browser JSON output. "
        f"Last output: {stdout[-400:]}"
    )


def extract_data(payload: dict[str, Any]) -> Any:
    if "data" in payload:
        return payload["data"]
    return payload


def extract_scalar(payload: dict[str, Any]) -> Any:
    data = extract_data(payload)
    if isinstance(data, dict):
        for key in ("text", "value", "title", "url"):
            if key in data:
                return data[key]
    return data


def refresh_current_url(settings: dict[str, Any]) -> str | None:
    try:
        result = run_agent_browser(["get", "url"], settings)
        current = extract_scalar(result)
        STATE.last_known_url = str(current) if current else None
    except Exception:
        pass
    return STATE.last_known_url


def validate_navigation_target(url: str, settings: dict[str, Any]) -> None:
    parsed = urlparse(url)
    scheme = (parsed.scheme or "").lower()
    if not scheme:
        raise ValueError("Browser-Orb expects an absolute URL with a scheme.")
    if scheme in FORBIDDEN_SCHEMES:
        raise ValueError(f"Browser-Orb does not allow {scheme}: URLs.")

    unrestricted = parse_bool(settings.get("allow_unrestricted_navigation"), False)
    allowed_schemes = set(PUBLIC_WEB_SCHEMES)
    if unrestricted:
        allowed_schemes.update(UNRESTRICTED_EXTRA_SCHEMES)
    if scheme not in allowed_schemes:
        raise ValueError(
            f"Browser-Orb navigation to scheme '{scheme}' is disabled by current settings."
        )

    host = parsed.hostname
    if scheme in PUBLIC_WEB_SCHEMES:
        if not host:
            raise ValueError("Browser-Orb requires a hostname for http(s) navigation.")
        if not unrestricted and is_local_or_private_host(host):
            raise ValueError(
                "Browser-Orb blocked navigation to a local or private host. "
                "Enable unrestricted navigation if you explicitly want that."
            )

        allowed_domains = parse_domain_allowlist(settings)
        if allowed_domains and not domain_matches_allowlist(host, allowed_domains):
            raise ValueError(
                f"Browser-Orb blocked '{host}' because it is outside the configured domain allowlist."
            )


def parse_domain_allowlist(settings: dict[str, Any]) -> list[str]:
    raw = str(settings.get("allowed_domains") or "")
    parts = []
    for chunk in raw.replace(",", "\n").splitlines():
        value = chunk.strip().lower()
        if value:
            parts.append(value)
    return parts


def format_domain_hint(allowed_domains: list[str]) -> str:
    if not allowed_domains:
        return "No explicit domain allowlist is configured."
    return "Configured allowed domains: " + ", ".join(allowed_domains) + "."


def domain_matches_allowlist(host: str, allowed_domains: list[str]) -> bool:
    lowered = host.lower()
    for pattern in allowed_domains:
        if fnmatch(lowered, pattern):
            return True
        if pattern.startswith("*.") and lowered == pattern[2:]:
            return True
        if lowered == pattern:
            return True
    return False


def is_local_or_private_host(host: str) -> bool:
    lowered = host.lower()
    if lowered in {"localhost", "localhost.localdomain"}:
        return True

    try:
        ip = ipaddress.ip_address(lowered)
        return is_private_ip(ip)
    except ValueError:
        pass

    try:
        infos = socket.getaddrinfo(lowered, None, proto=socket.IPPROTO_TCP)
    except socket.gaierror:
        return False

    for info in infos:
        sockaddr = info[4]
        if not sockaddr:
            continue
        ip_text = sockaddr[0]
        try:
            ip = ipaddress.ip_address(ip_text)
        except ValueError:
            continue
        if is_private_ip(ip):
            return True

    return False


def is_private_ip(ip: ipaddress._BaseAddress) -> bool:
    return bool(
        ip.is_private
        or ip.is_loopback
        or ip.is_link_local
        or ip.is_multicast
        or ip.is_reserved
        or ip.is_unspecified
    )


def resolve_output_path(raw_path: str, default_dir: str) -> Path:
    if raw_path:
        path = Path(raw_path).expanduser()
        if not path.is_absolute():
            path = REPO_ROOT / path
    else:
        directory = REPO_ROOT / Path(default_dir)
        timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        path = directory / f"browser_orb_{timestamp}.png"

    path.parent.mkdir(parents=True, exist_ok=True)
    return path


def json_result(data: dict[str, Any]) -> dict[str, Any]:
    return {"kind": "json", "data": data}


def require_string(arguments: dict[str, Any], key: str) -> str:
    value = str(arguments.get(key) or "").strip()
    if not value:
        raise ValueError(f"{key} is required")
    return value


def parse_bool(value: Any, default: bool) -> bool:
    if value is None:
        return default
    if isinstance(value, bool):
        return value
    if isinstance(value, (int, float)):
        return bool(value)
    lowered = str(value).strip().lower()
    if lowered in {"1", "true", "yes", "on"}:
        return True
    if lowered in {"0", "false", "no", "off"}:
        return False
    return default


def parse_int(value: Any, default: int) -> int:
    if value is None or value == "":
        return default
    try:
        return int(value)
    except (TypeError, ValueError):
        return default


if __name__ == "__main__":
    raise SystemExit(main())
