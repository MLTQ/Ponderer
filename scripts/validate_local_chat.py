#!/usr/bin/env python3
"""Explicit real-GGUF chat/tool regression; isolated state and no tool execution.

Loads only when invoked with a model, server and engine device ID. Model weights
are read-only. All temporary inference processes are owned by this test process.
No operator configuration, conversation, affect mix or running app is modified.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import tempfile
import threading
import time
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("affect_worker", ROOT / "ponderer_backend/resources/affect_lab/worker.py")
worker = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(worker)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", required=True)
    parser.add_argument("--server", required=True)
    parser.add_argument("--device", required=True)
    parser.add_argument("--context", type=int, default=200_000)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="ponderer-local-chat-") as temporary:
        lab = worker.AffectLab(args.model, temporary, server_binary=args.server, gpu_layers=-1,
                               gpu_device=args.device, context_size=args.context,
                               cache_type_k="q4_1", cache_type_v="q4_1", flash_attention="on")
        proxy = None
        started = time.monotonic()
        try:
            lab.ensure_server(lab.profile)
            pid = lab.child.pid
            print(json.dumps({"loaded_seconds": round(time.monotonic() - started, 2),
                              "settings": lab.inference_settings()}), flush=True)

            def completion(messages, **extra):
                result = lab.completion({"model": worker.ALIAS, "messages": messages,
                                         "temperature": 0, "seed": 42, "max_tokens": 96, **extra})
                assert result["choices"][0]["finish_reason"] != "length", "Truncated response"
                return result["choices"][0]["message"]

            hello = [{"role": "user", "content": "Reply with only the word hello."}]
            begin = time.monotonic()
            message = completion(hello)
            assert message["content"].strip().lower().strip(".!\"") == "hello", message.get("content")
            print(json.dumps({"non_streaming": message["content"], "seconds": round(time.monotonic() - begin, 2)}), flush=True)

            tools = [{"type": "function", "function": {"name": "fixture_value", "description": "Returns a fixture number for a test; no side effects.",
                      "parameters": {"type": "object", "properties": {}, "additionalProperties": False}}}]
            history = [{"role": "user", "content": "Use fixture_value and then reply with only the returned number."}]
            message = completion(history, tools=tools, tool_choice={"type": "function", "function": {"name": "fixture_value"}})
            calls = message.get("tool_calls", [])
            assert len(calls) == 1 and calls[0]["function"]["name"] == "fixture_value", "Missing structured fixture call"
            assert json.loads(calls[0]["function"]["arguments"]) == {}, "Unexpected fixture arguments"
            # No tool is executed. Feed the inert fixture response directly.
            history += [message, {"role": "tool", "tool_call_id": calls[0]["id"], "content": "42"}]
            answer = completion(history, tools=tools, tool_choice="none")
            assert answer["content"].strip().strip(".\"") == "42", answer.get("content")
            print(json.dumps({"structured_tool": "fixture_value", "tool_result_answer": answer["content"]}), flush=True)

            proxy = worker.LabHTTPServer(("127.0.0.1", 0), worker.LabHandler)
            proxy.lab, proxy.token = lab, "isolated-regression-token"
            threading.Thread(target=proxy.serve_forever, daemon=True).start()
            body = {"model": worker.ALIAS, "messages": hello, "temperature": 0, "seed": 42,
                    "max_tokens": 64, "stream": True, "tools": tools}
            request = Request(f"http://127.0.0.1:{proxy.server_port}/v1/chat/completions", json.dumps(body).encode(),
                              {"Authorization": "Bearer " + proxy.token, "Content-Type": "application/json"})
            visible, done = "", False
            with urlopen(request, timeout=180) as response:
                for line in response:
                    if not line.startswith(b"data:"):
                        continue
                    payload = line[5:].strip()
                    if payload == b"[DONE]":
                        done = True
                        break
                    chunk = json.loads(payload)
                    if chunk.get("choices"):
                        visible += chunk["choices"][0]["delta"].get("content") or ""
            assert done and visible.strip().lower().strip(".!\"") == "hello", visible
            assert lab.child.pid == pid, "Unchanged neutral requests reloaded the engine"
            print(json.dumps({"streaming_with_tools": visible, "engine_reused": True}), flush=True)
        finally:
            if proxy:
                proxy.shutdown()
                proxy.server_close()
            lab.close()
        print("PASS: real all-GPU Qwen chat, structured tool call, tool-result follow-up, streaming with tools, and owned cleanup", flush=True)


if __name__ == "__main__":
    main()
