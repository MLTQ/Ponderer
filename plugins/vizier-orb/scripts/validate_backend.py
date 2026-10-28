"""Smoke-test an installed Vizier package against an isolated Ponderer backend."""

import argparse
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time
import urllib.request


def validate(binary, plugin_directory):
    with tempfile.TemporaryDirectory(prefix="ponderer-vizier-host-") as temporary:
        directory = Path(temporary).resolve()
        executable = directory / "ponderer"
        shutil.copy2(Path(binary).resolve(), executable)
        (directory / "ponderer_config.toml").write_text(
            'llm_api_url = "http://127.0.0.1:1/v1"\nllm_model = "fixture"\n'
            'username = "VizierTest"\nenable_ambient_loop = false\n'
            'enable_self_reflection = false\nenable_screen_capture_in_loop = false\n'
            'loose_mode = false\npoll_interval_secs = 86400\ndatabase_path = "fixture.db"\n'
        )
        with socket.socket() as reservation:
            reservation.bind(("127.0.0.1", 0))
            port = reservation.getsockname()[1]
        env = dict(
            os.environ,
            PONDERER_PLUGIN_DIR=str(Path(plugin_directory).resolve()),
            PONDERER_BACKEND_BIND=f"127.0.0.1:{port}",
            PONDERER_BACKEND_AUTH_MODE="required",
            PONDERER_BACKEND_TOKEN="vizier-test-token",
            PONDERER_BACKEND_PARENT_PIPE="1",
        )

        def read(path):
            request = urllib.request.Request(
                f"http://127.0.0.1:{port}/v1{path}",
                headers={"Authorization": "Bearer vizier-test-token"},
            )
            with urllib.request.urlopen(request, timeout=2) as response:
                return json.load(response)

        log_path = directory / "backend.log"
        with log_path.open("wb") as log:
            process = subprocess.Popen(
                [str(executable), "--backend-only"], cwd=directory, env=env,
                stdin=subprocess.PIPE, stdout=log, stderr=log,
            )
            try:
                statuses = []
                deadline = time.monotonic() + 20
                while time.monotonic() < deadline:
                    try:
                        statuses = read("/plugins/status")
                        vizier = next((row for row in statuses if row["plugin_id"] == "vizier-orb"), None)
                        if vizier and vizier["state"] == "running":
                            break
                    except OSError:
                        pass
                    time.sleep(0.2)
                else:
                    raise AssertionError({"statuses": statuses, "log": log_path.read_text()[-4000:]})
                manifest = next(row for row in read("/plugins") if row["id"] == "vizier-orb")
                assert manifest["provided_tools"] == ["vz", "vz_ssh"], manifest["provided_tools"]
                assert vizier["desired_enabled"] and vizier["negotiated_protocol_version"] == 1, vizier
                print("PASS: Vizier discovered, enabled, running on protocol v1; vz and vz_ssh registered")
            finally:
                process.stdin.close()
                try:
                    process.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary")
    parser.add_argument("plugin_directory")
    args = parser.parse_args()
    validate(args.binary, args.plugin_directory)
