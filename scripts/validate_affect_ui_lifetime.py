#!/usr/bin/env python3
"""Exercise the real UI-parent pipe, backend routes and local-provider lifetime.

Usage: python3 scripts/validate_affect_ui_lifetime.py /absolute/path/to/ponderer
Uses a temporary model fixture and fake inference process; no user config or model
is modified. Closing the simulated UI ownership pipe must kill the entire chain.
"""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from urllib.request import Request, urlopen
from urllib.error import HTTPError

ROOT = Path(__file__).resolve().parents[1]
TEST_HELPER = ROOT / "ponderer_backend/tests/test_affect_lab.py"
spec = importlib.util.spec_from_file_location("affect_test_helpers", TEST_HELPER)
helpers = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helpers)


def request(base, path, value=None, method=None):
    payload = json.dumps(value).encode() if value is not None else None
    call = Request(base + path, payload, {"Authorization": "Bearer lifecycle-test-token", "Content-Type": "application/json"}, method=method)
    with urlopen(call, timeout=10) as response:
        return json.load(response)


def descendants(pid):
    result = []
    try:
        children = Path(f"/proc/{pid}/task/{pid}/children").read_text().split()
    except FileNotFoundError:
        return result
    for child in map(int, children):
        result.append(child)
        result.extend(descendants(child))
    return result


def main():
    binary = Path(sys.argv[1]).resolve()
    if not binary.is_file():
        raise ValueError("Build the desktop executable first")
    with tempfile.TemporaryDirectory(prefix="ponderer-affect-lifetime-") as temporary:
        directory = Path(temporary) / "isolated"
        directory.mkdir()
        executable = directory / "ponderer"
        shutil.copy2(binary, executable)
        model = directory / "fixture.gguf"
        helpers.make_gguf(model)
        fake = directory / "fake-server"
        fake.write_text(f"#!/bin/sh\nexec {sys.executable} {TEST_HELPER} --fake-server \"$@\"\n")
        fake.chmod(0o700)
        config_path = directory / "ponderer_config.toml"
        config_path.write_text('llm_api_url = "http://127.0.0.1:1/v1"\nllm_model = "original-model"\nusername = "LifecycleTest"\nenable_ambient_loop = false\nenable_self_reflection = false\nenable_screen_capture_in_loop = false\nloose_mode = false\npoll_interval_secs = 86400\ndatabase_path = "fixture.db"\n')
        env = dict(os.environ, PONDERER_BACKEND_BIND="", PONDERER_BACKEND_AUTH_MODE="required", PONDERER_BACKEND_TOKEN="lifecycle-test-token", PONDERER_BACKEND_PARENT_PIPE="1", PONDERER_AFFECT_DATA_DIR=str(directory / "lab"))
        with socket.socket() as reservation:
            reservation.bind(("127.0.0.1", 0))
            port = reservation.getsockname()[1]
        env["PONDERER_BACKEND_BIND"] = f"127.0.0.1:{port}"
        base = f"http://127.0.0.1:{port}/v1"
        with (directory / "backend.log").open("wb") as log:
            backend = subprocess.Popen([str(executable), "--backend-only"], cwd=directory, env=env, stdin=subprocess.PIPE, stdout=log, stderr=log)
            pids = []
            try:
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    try:
                        request(base, "/health")
                        break
                    except OSError:
                        time.sleep(0.05)
                request(base, "/agent/pause", {"paused": True}, "PUT")
                state = request(base, "/affect-lab/start", {"model_path": str(model), "server_binary": str(fake), "context_size": 200_000, "unified_kv_cache": True, "cache_type_k": "q4_1", "cache_type_v": "q4_1", "flash_attention": "on"})
                assert state["running"]
                assert state["inference_settings"]["context_size"] == 200_000
                assert state["inference_settings"]["cache_type_k"] == "q4_1"
                assert state["inference_settings"]["cache_type_v"] == "q4_1"
                assert state["inference_settings"]["unified_kv_cache"]
                assert state["inference_settings"]["flash_attention"] == "on"
                selected = request(base, "/affect-lab/use-for-agent", {})
                selected["relationship_description"] = "Temporary lifecycle test"
                request(base, "/config", selected, "PUT")
                durable = config_path.read_text()
                assert "original-model" in durable and "ponderer-local-gguf" not in durable
                assert selected["llm_api_key"] not in durable, "Ephemeral provider token was persisted"
                # Both completion clients see this same compatible API endpoint.
                call = Request(selected["llm_api_url"] + "/chat/completions", json.dumps({"model": selected["llm_model"], "messages": [{"role": "user", "content": "fixture"}]}).encode(), {"Authorization": "Bearer " + selected["llm_api_key"], "Content-Type": "application/json"})
                try:
                    with urlopen(call, timeout=10) as response:
                        assert json.load(response)["choices"][0]["message"]["content"] == "0"
                except HTTPError as error:
                    details = error.read().decode(errors="replace")
                    inference_log = directory / "lab/inference.log"
                    if inference_log.is_file():
                        details += "\n" + inference_log.read_text(errors="replace")[-2000:]
                    raise RuntimeError(f"Fixture inference failed: {details}") from error
                state = request(base, "/affect-lab")
                pids = [backend.pid, state["worker_pid"], state["native_pid"]]
                pids.extend(descendants(state["native_pid"]))
                assert len(set(pids)) >= 4, "Expected backend, worker, native supervisor and inference process"
                backend.stdin.close()
                backend.wait(timeout=5)
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline and not all(helpers.inactive(pid) for pid in pids):
                    time.sleep(0.05)
                assert all(helpers.inactive(pid) for pid in pids), "A model process survived UI-parent pipe closure"
                print("PASS: session provider, durable-config isolation, and UI-close termination of the complete inference chain")
            finally:
                if backend.poll() is None:
                    backend.kill()
                    backend.wait(timeout=5)
                if backend.stdin and not backend.stdin.closed:
                    backend.stdin.close()
                # Cleanup targets are exclusively PIDs created and verified above.
                for pid in pids:
                    if not helpers.inactive(pid):
                        os.kill(pid, 9)


if __name__ == "__main__":
    main()
