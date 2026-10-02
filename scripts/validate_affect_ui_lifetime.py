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
import tomllib
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
    except (FileNotFoundError, ProcessLookupError):
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
        vector_dir = directory / "lab/vectors/contentment-test"
        vector_dir.mkdir(parents=True)
        vector = vector_dir / "vector.gguf"
        helpers.make_gguf(vector, vector=True)
        (vector_dir / "manifest.json").write_text(json.dumps({
            "concept": "contentment", "created_at": time.time(), "model_path": str(model),
            "model_identity": helpers.worker.quick_identity(model), "model_sha256": helpers.worker.sha256_file(model),
            "recipe_sha256": "fixture", "vector_sha256": helpers.worker.sha256_file(vector)}))
        fake = directory / "fake-server"
        fake.write_text(f"#!/bin/sh\nexec {sys.executable} {TEST_HELPER} --fake-server \"$@\"\n")
        fake.chmod(0o700)
        config_path = directory / "ponderer_config.toml"
        config_path.write_text('llm_api_url = "http://127.0.0.1:1/v1"\nllm_model = "original-model"\nusername = "LifecycleTest"\nenable_ambient_loop = false\nenable_self_reflection = false\nenable_screen_capture_in_loop = false\nloose_mode = false\npoll_interval_secs = 86400\ndatabase_path = "fixture.db"\n')
        env = dict(os.environ, PONDERER_BACKEND_BIND="", PONDERER_BACKEND_AUTH_MODE="required", PONDERER_BACKEND_TOKEN="lifecycle-test-token", PONDERER_BACKEND_PARENT_PIPE="1", PONDERER_AFFECT_DATA_DIR=str(directory / "lab"))
        env["PONDERER_AFFECT_TEST_DELAY"] = "0.15"
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
                inventory = request(base, "/affect-lab/devices", {"server_binary": str(fake)})
                assert inventory["devices"][0]["id"] == "CUDA0"
                assert inventory["devices"][0]["name"] == "Fixture GPU"
                state = request(base, "/affect-lab/start", {"model_path": str(model), "server_binary": str(fake), "gpu_layers": -1, "gpu_device": "CUDA0", "context_size": 200_000, "unified_kv_cache": True, "cache_type_k": "q4_1", "cache_type_v": "q4_1", "flash_attention": "on"})
                assert state["running"]
                assert state["inference_settings"]["context_size"] == 200_000
                assert state["inference_settings"]["cache_type_k"] == "q4_1"
                assert state["inference_settings"]["cache_type_v"] == "q4_1"
                assert state["inference_settings"]["unified_kv_cache"]
                assert state["inference_settings"]["flash_attention"] == "on"
                assert state["inference_settings"]["gpu_device"] == "CUDA0"
                assert state["inference_settings"]["gpu_offload"] == "all"
                assert "example_library" in state
                assert state["capabilities"]["automatic_discovery"]
                assert state["capabilities"]["signed_controls"]
                assert len(state["test_prompts"]) == 5
                try:
                    request(base, "/affect-lab/use-for-agent", {})
                    raise AssertionError("An unloaded engine was selectable for the session")
                except HTTPError as error:
                    assert error.code == 400, error
                assert request(base, "/config")["llm_model"] == "original-model"
                request(base, "/affect-lab/load", {})
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    state = request(base, "/affect-lab")
                    if state["job"]["phase"] != "running":
                        break
                    time.sleep(0.05)
                assert state["job"]["phase"] == "complete", state["job"]
                time.sleep(0.2)
                state = request(base, "/affect-lab")
                assert state["native_pid"], "The loaded engine died when its job thread exited"
                loaded_pid = state["native_pid"]
                selected = request(base, "/affect-lab/use-for-agent", {})
                selected["relationship_description"] = "Temporary lifecycle test"
                selected["appearance"] = {"base_color": [183, 156, 220], "dark": False}
                saved = request(base, "/config", selected, "PUT")
                assert saved["appearance"] == selected["appearance"]
                durable = config_path.read_text()
                assert tomllib.loads(durable)["appearance"] == selected["appearance"], "Appearance did not persist through the actual config route"
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
                assert state["native_pid"] == loaded_pid, "Completion reloaded an already loaded neutral engine"
                time.sleep(0.2)
                state = request(base, "/affect-lab")
                assert state["native_pid"] == loaded_pid, "Engine died when its HTTP request thread exited"
                request(base, "/affect-lab/profile", {"strengths": {"contentment": -0.25}, "gain": 4})
                state = request(base, "/affect-lab/study", {"concepts": ["contentment"], "max_tokens": 64})
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline:
                    state = request(base, "/affect-lab")
                    if state["job"]["phase"] == "running" and state["native_pid"] and state["last_study"]:
                        break
                    time.sleep(0.02)
                assert state["job"]["phase"] == "running" and state["native_pid"], "Expected an active model-owned response-study job"
                assert state["requested_profile"]["strengths"]["contentment"] == -0.25, "Study changed the agent's signed mix"
                assert state["requested_profile"]["gain"] == 4
                pids = [backend.pid, state["worker_pid"], state["native_pid"]]
                pids.extend(descendants(state["native_pid"]))
                assert len(set(pids)) >= 4, "Expected backend, worker, native supervisor and inference process"
                backend.stdin.close()
                backend.wait(timeout=5)
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline and not all(helpers.inactive(pid) for pid in pids):
                    time.sleep(0.05)
                assert all(helpers.inactive(pid) for pid in pids), "A model process survived UI-parent pipe closure"
                print("PASS: GPU inventory and explicit all-GPU selection, 200k/Q4_1 settings, appearance config round-trip, explicit load persists across jobs/requests, session provider/config isolation, signed study preserves the mix, and UI closure terminates an active study and the complete inference chain")
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
