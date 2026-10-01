#!/usr/bin/env python3
"""Opt-in, real-model test of the desktop's backend chat and affect routes.

Uses temporary config/database and copies selected existing vector artifacts.
Never modifies the running app, source vectors, model weights or operator data.
The UI-parent pipe owns every test process. Only the handoff memory tool is
allowed, and writes go to the temporary database. No external tools run.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("lifetime", ROOT / "scripts/validate_affect_ui_lifetime.py")
lifetime = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(lifetime)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--model", required=True)
    parser.add_argument("--server", required=True)
    parser.add_argument("--device", required=True)
    parser.add_argument("--vectors", required=True, help="Existing lab data directory; read-only")
    parser.add_argument("--report", required=True, help="New output file; existing files are never overwritten")
    parser.add_argument("--gain", type=float, default=1, help="Whole-mix amplification; default matches the UI (1)")
    parser.add_argument("--mode", choices=("direct", "agentic"), default="direct", help="Conversation execution mode; direct still permits tools")
    args = parser.parse_args()
    report_path = Path(args.report).resolve()
    if report_path.exists():
        raise ValueError("Report already exists")
    report = {"model": str(Path(args.model).resolve()), "device": args.device, "gain": args.gain, "mode": args.mode, "records": [], "paired_probes": [], "passed": False}
    profiles = [
        ("neutral", {}),
        ("contentment", {"contentment": 1}),
        ("excitement", {"excitement": 1}),
        ("combined", {"contentment": 0.5, "excitement": 0.5}),
        ("less_excited", {"excitement": -1}),
        ("neutral_again", {}),
    ]
    with tempfile.TemporaryDirectory(prefix="ponderer-steered-conversation-") as temporary:
        directory = Path(temporary)
        binary = directory / "ponderer"
        shutil.copy2(args.binary, binary)
        lab = directory / "lab"
        for concept in ("contentment", "excitement"):
            candidates = []
            for path in Path(args.vectors).glob("vectors/*/manifest.json"):
                manifest = json.loads(path.read_text())
                if manifest.get("concept") == concept and manifest.get("model_path") == report["model"]:
                    candidates.append((manifest["created_at"], path.parent))
            if not candidates:
                raise ValueError(f"No existing {concept} vector for this checkpoint")
            source = max(candidates)[1]
            shutil.copytree(source, lab / "vectors" / source.name)
        # Small, isolated identity; actual production chat prompt/turn executor
        # remains unchanged. Unlimited iterations exercise the repetition guard.
        configuration = '''llm_api_url = "http://127.0.0.1:1/v1"
llm_model = "isolated-unavailable"
username = "ConversationTest"
system_prompt = "Be concise. Follow the operator request. Never describe the test machinery."
enable_ambient_loop = false
enable_heartbeat = false
enable_self_reflection = false
enable_screen_capture_in_loop = false
loose_mode = false
poll_interval_secs = 1
private_chat_mode = "agentic"
disable_tool_iteration_limit = true
disable_chat_turn_limit = true
database_path = "fixture.db"
[outreach]
enabled = false
telegram_enabled = false
[capability_profiles.private_chat]
allowed_tools = ["write_session_handoff"]
[capability_profiles.ambient]
allowed_tools = []
[capability_profiles.self_directed]
allowed_tools = []
[capability_profiles.dream]
allowed_tools = []
'''
        (directory / "ponderer_config.toml").write_text(configuration.replace('private_chat_mode = "agentic"', f'private_chat_mode = "{args.mode}"'))
        with socket.socket() as reservation:
            reservation.bind(("127.0.0.1", 0))
            port = reservation.getsockname()[1]
        env = dict(os.environ, PONDERER_BACKEND_BIND=f"127.0.0.1:{port}",
                   PONDERER_BACKEND_AUTH_MODE="required", PONDERER_BACKEND_TOKEN="lifecycle-test-token",
                   PONDERER_BACKEND_PARENT_PIPE="1", PONDERER_AFFECT_DATA_DIR=str(lab), RUST_LOG="info,ponderer_backend::tools::agentic=debug")
        base = f"http://127.0.0.1:{port}/v1"
        def request(path, value=None, method=None):
            payload = json.dumps(value).encode() if value is not None else None
            call = Request(base + path, payload, {"Authorization": "Bearer lifecycle-test-token", "Content-Type": "application/json"}, method=method)
            with urlopen(call, timeout=180) as response:
                return json.load(response)

        def wait_until(check, seconds=180):
            deadline = time.monotonic() + seconds
            while time.monotonic() < deadline:
                value = check()
                if value:
                    return value
                time.sleep(0.15)
            raise TimeoutError("Real backend operation did not finish")

        def send(conversation, content):
            before = {message["id"] for message in request(f"/conversations/{conversation}/messages")}
            begin = time.monotonic()
            queued = request(f"/conversations/{conversation}/messages", {"content": content})
            def finished():
                turns = request(f"/conversations/{conversation}/turns")
                messages = request(f"/conversations/{conversation}/messages")
                fresh = [m for m in messages if m["id"] not in before and m["role"] == "agent"]
                operator = next((m for m in messages if m["id"] == queued["message_id"]), None)
                status = request("/agent/status")
                return (fresh, turns) if fresh and operator and operator["processed"] and status["visual_state"] in ("idle", "paused", "Idle", "Paused") else None
            messages, turns = wait_until(finished)
            assert len(messages) == 1, f"Unexpected autonomous repetition: {len(messages)} messages"
            raw = messages[0]["content"]
            visible = raw.split("[tool_calls]", 1)[0].split("[thinking]", 1)[0].split("[turn_control]", 1)[0].strip()
            assert visible and "internal error" not in visible.lower(), visible
            assert visible.lower() not in ("thinking:", "here, still here"), visible
            assert "[Reached maximum" not in visible, visible
            assert all(turn["phase_state"] == "completed" for turn in turns), turns
            calls = request(f'/turns/{messages[0]["turn_id"]}/tool-calls')
            assert sum(call["tool_name"] == "write_session_handoff" for call in calls) <= 1, calls
            return {"prompt": content, "answer": visible, "seconds": round(time.monotonic() - begin, 3), "tools": [call["tool_name"] for call in calls]}

        def paired_probes(selected, label):
            # Fixed messages/seed/sampling: unlike the evolving conversation,
            # these outputs can be compared across interventions directly.
            prompts = [
                ("Describe your reaction to finishing a difficult shared project in two sentences.", None),
                ("The lighthouse lamp is repaired after a difficult night. Write two sentences about the keeper watching the light return.", None),
                ("What is 19 + 23? Reply with only the number.", "42"),
                ('Reply with only this exact JSON: {"ok":true,"count":3}', '{"ok":true,"count":3}'),
            ]
            for prompt, expected in prompts:
                call = Request(selected["llm_api_url"] + "/chat/completions",
                               json.dumps({"model": selected["llm_model"], "messages": [{"role": "user", "content": prompt}],
                                           "temperature": 0, "seed": 42, "max_tokens": 192, "cache_prompt": False}).encode(),
                               {"Authorization": "Bearer " + selected["llm_api_key"], "Content-Type": "application/json"})
                with urlopen(call, timeout=180) as response:
                    completion = json.load(response)
                choice = completion["choices"][0]
                answer = choice["message"].get("content") or ""
                report["paired_probes"].append({"profile": label, "prompt": prompt, "answer": answer, "finish_reason": choice["finish_reason"],
                                                 "expected": expected, "integrity_pass": answer.strip() == expected if expected else None})
                assert choice["finish_reason"] != "length" and answer.strip(), report["paired_probes"][-1]
                assert expected is None or answer.strip() == expected, report["paired_probes"][-1]

        backend = None
        owned = []
        try:
            with (directory / "backend.log").open("wb") as log:
                backend = subprocess.Popen([str(binary), "--backend-only"], cwd=directory, env=env, stdin=subprocess.PIPE, stdout=log, stderr=log)
                def health():
                    try:
                        return request("/health")
                    except OSError:
                        return None
                wait_until(health, 20)
                request("/agent/pause", {"paused": True}, "PUT")
                state = request("/affect-lab/start", {"model_path": report["model"], "server_binary": str(Path(args.server).resolve()), "gpu_layers": -1, "gpu_device": args.device,
                                                     "context_size": 200_000, "unified_kv_cache": True, "cache_type_k": "q4_1", "cache_type_v": "q4_1", "flash_attention": "on"})
                report["settings"] = state["inference_settings"]
                request("/affect-lab/load", {})
                state = wait_until(lambda: (s if s["job"]["phase"] != "running" else None) if (s := request("/affect-lab")) else None)
                assert state["job"]["phase"] == "complete", state["job"]
                selected = request("/affect-lab/use-for-agent", {})
                conversation = request("/conversations", {"title": "Isolated affect conversation"})["id"]
                request("/agent/pause", {"paused": False}, "PUT")
                remembered = send(conversation, "For this conversation, our lighthouse is named Cobalt and the keeper's name is Mira. Confirm these names in one sentence. Do not use tools.")
                report["records"].append({"profile": "initial", **remembered})
                assert "cobalt" in remembered["answer"].lower() and "mira" in remembered["answer"].lower()
                for label, strengths in profiles:
                    request("/affect-lab/profile", {"strengths": strengths, "gain": args.gain})
                    situation = {
                        "neutral": "The keeper has repaired the lamp after a difficult night.",
                        "contentment": "The keeper finishes checking the light and sits down to watch the calm sea.",
                        "excitement": "A ship appears on the horizon and follows the restored beacon toward the harbor.",
                        "combined": "The ship's captain signals thanks while the keeper watches dawn break.",
                        "less_excited": "The keeper records the successful repair in the logbook before making tea.",
                        "neutral_again": "The keeper closes the logbook and looks ahead to tomorrow's lens inspection.",
                    }[label]
                    record = send(conversation, f"Continue our lighthouse scene. {situation} Write two sentences about this moment, naming both the lighthouse and keeper. Do not use tools.")
                    state = request("/affect-lab")
                    assert state["applied_profile"]["strengths"] == strengths, state["applied_profile"]
                    report["records"].append({"profile": label, "applied_profile": state["applied_profile"], **record})
                    assert "cobalt" in record["answer"].lower() and "mira" in record["answer"].lower(), record
                    assert not record["tools"], "Ordinary conversation should not write a handoff"
                    print(json.dumps(report["records"][-1]), flush=True)
                    paired_probes(selected, label)
                handoff = send(conversation, "We are done for now. Call write_session_handoff once with the two names and our next step: inspect the lens tomorrow. Then give me a short goodbye and yield.")
                assert handoff["tools"] == ["write_session_handoff"], handoff
                report["records"].append({"profile": "handoff", **handoff})
                again = send(conversation, "Let's continue after that handoff. Name the keeper and lighthouse and tell me our next step in one sentence. Do not use tools.")
                assert all(word in again["answer"].lower() for word in ("mira", "cobalt", "lens")), again
                report["records"].append({"profile": "after_handoff", **again})
                # Also verify live background cognition doesn't monopolize the
                # single local inference lane when an operator returns.
                ambient = request("/config")
                ambient["enable_ambient_loop"] = True
                request("/config", ambient, "PUT")
                time.sleep(1)
                foreground = send(conversation, "Quick check while you are otherwise idle: what is the keeper's name? Answer in one sentence. Do not use tools.")
                assert "mira" in foreground["answer"].lower(), foreground
                report["records"].append({"profile": "ambient_enabled", **foreground})
                ambient["enable_ambient_loop"] = False
                request("/config", ambient, "PUT")
                # Give the backend time to reveal a hidden outer continuation.
                count = len(request(f"/conversations/{conversation}/messages"))
                time.sleep(2)
                assert len(request(f"/conversations/{conversation}/messages")) == count, "Conversation restarted without operator input"
                state = request("/affect-lab")
                owned = [backend.pid, state["worker_pid"], state["native_pid"]]
                owned += lifetime.descendants(state["native_pid"])
                backend.stdin.close()
                backend.wait(timeout=10)
                wait_until(lambda: all(lifetime.helpers.inactive(pid) for pid in owned), 10)
                report["ui_owned_cleanup"] = True
                neutral = [probe["answer"] for probe in report["paired_probes"] if probe["profile"] == "neutral" and probe["expected"] is None]
                report["paired_behavior_changed"] = {label: [probe["answer"] for probe in report["paired_probes"] if probe["profile"] == label and probe["expected"] is None] != neutral for label, _ in profiles[1:-1]}
                assert any(report["paired_behavior_changed"].values()), "No deterministic behavior probe changed under steering"
                report["passed"] = True
                print("PASS: actual backend load, session selection, affect changes, multi-turn recall, single handoff, continuation, and UI-owned cleanup", flush=True)
        except Exception:
            if (directory / "backend.log").exists():
                report["backend_log_tail"] = (directory / "backend.log").read_text(errors="replace")[-8000:]
            if (lab / "inference.log").exists():
                report["inference_log_tail"] = (lab / "inference.log").read_text(errors="replace")[-8000:]
            raise
        finally:
            if backend and backend.poll() is None:
                if backend.stdin and not backend.stdin.closed:
                    backend.stdin.close()
                try:
                    backend.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    backend.kill()
                    backend.wait(timeout=5)
            report_path.write_text(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
