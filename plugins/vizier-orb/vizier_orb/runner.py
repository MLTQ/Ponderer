"""Bounded one-shot Vizier execution with explicit local/remote identity."""

import json
import os
from pathlib import Path
import re
import selectors
import shlex
import shutil
import signal
import subprocess
import time

MAX_OUTPUT_BYTES = 256_000
TIMEOUT_SECONDS = 45
INSTALL_URL = "https://raw.githubusercontent.com/MLTQ/vizier/master/scripts/install.sh"
REMOTE_PATH = 'export PATH="$HOME/.local/bin:$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"; '


def resolve_vz(settings):
    """Honor an explicit executable; GUI launches may have a minimal PATH."""
    configured = settings.get("vz_path") or os.environ.get("PONDERER_VZ_BIN", "")
    if not isinstance(configured, str):
        raise ValueError("Vizier's vz_path setting must be a path or executable name")
    if configured:
        candidate = shutil.which(os.path.expanduser(configured))
        if not candidate:
            raise ValueError(f"Configured Vizier executable is not executable: {configured}")
        return candidate
    candidates = [shutil.which("vz")]
    home = Path.home()
    candidates.extend(str(home / path) for path in (
        ".local/bin/vz", ".cargo/bin/vz", "Code/vizier/target/release/vz",
        "Code/vizier/target/debug/vz",
    ))
    candidates.extend(("/opt/homebrew/bin/vz", "/usr/local/bin/vz"))
    return next((path for path in candidates if path and Path(path).is_file()
                 and os.access(path, os.X_OK)), None)


def observation_args(arguments, *, remote):
    allowed = {"mode", "verbose"} | ({"host", "port"} if remote else set())
    unknown = set(arguments) - allowed
    if unknown:
        raise ValueError(f"Unknown Vizier arguments: {', '.join(sorted(unknown))}")
    mode = arguments.get("mode", "wake")
    if mode not in ("wake", "snapshot"):
        raise ValueError("mode must be wake or snapshot (watch is a continuous shell command)")
    verbose = arguments.get("verbose", False)
    if not isinstance(verbose, bool):
        raise ValueError("verbose must be a boolean")
    args = ["--no-public-ip"]
    if verbose:
        args.append("--verbose")
    # Bare vz is the compact snapshot. Explicit `snapshot` returns raw rows.
    if mode == "wake":
        args.append("wake")
    return mode, args


def ssh_prefix(arguments):
    host = arguments.get("host")
    if (not isinstance(host, str) or len(host) > 255
            or not re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.@:%+\[\]-]*", host)):
        raise ValueError("host must be an SSH alias or [user@]hostname, without shell syntax or options")
    port = arguments.get("port", 22)
    if type(port) is not int or not 1 <= port <= 65535:
        raise ValueError("port must be an integer from 1 to 65535")
    args = ["ssh", "-T", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10",
            "-o", "ConnectionAttempts=1"]
    # Let ~/.ssh/config choose the port unless the caller explicitly supplies it.
    if "port" in arguments:
        args += ["-p", str(port)]
    return host, args + ["--", host]


def missing_result(target, mode, prefix=()):
    # Fetch first so a failed download cannot be mistaken for a successful install.
    install = (
        'vz_setup=$(mktemp) && '
        f'curl -fsSL {shlex.quote(INSTALL_URL)} -o "$vz_setup" && '
        'sh "$vz_setup"; vz_status=$?; rm -f "$vz_setup"; exit "$vz_status"'
    )
    command = shlex.join([*prefix, install]) if prefix else "sh -c " + shlex.quote(install)
    return {
        "status": "not_installed", "target": target, "mode": mode,
        "message": "Vizier (vz) is not installed on this target. Run install_command with the shell tool, then retry. "
                   "The upstream installer supports Apple Silicon macOS and x86_64 Linux; other targets need a native source build. "
                   "Do not copy a Mac binary to Linux.",
        "install_command": command,
    }


def observe(arguments, settings, *, remote=False):
    mode, args = observation_args(arguments, remote=remote)
    if remote:
        target, prefix = ssh_prefix(arguments)
        script = (REMOTE_PATH + 'if ! command -v vz >/dev/null 2>&1; then '
                  'echo PONDERER_VZ_MISSING >&2; exit 127; fi; exec ' + shlex.join(["vz", *args]))
        command = [*prefix, script]
    else:
        target, prefix = "local", ()
        executable = resolve_vz(settings)
        if executable is None:
            return missing_result(target, mode)
        command = [executable, *args]
    code, stdout, stderr = run_bounded(command)
    if remote and code == 127 and "PONDERER_VZ_MISSING" in stderr:
        return missing_result(target, mode, prefix)
    if code:
        raise RuntimeError(f"Vizier on {target} exited {code}: {stderr.strip()[:2000]}")
    try:
        data = json.loads(stdout)
    except ValueError as error:
        raise RuntimeError(f"Vizier on {target} returned invalid JSON; check the executable and SSH login output") from error
    if not isinstance(data, dict) or "schema_version" not in data:
        raise RuntimeError(f"Vizier on {target} returned an unrecognized observation")
    return {"status": "ok", "target": target, "mode": mode, "observation": data}


def run_bounded(command, *, timeout=TIMEOUT_SECONDS, max_bytes=MAX_OUTPUT_BYTES):
    """Drain both streams without unbounded memory or surviving timed-out children."""
    output = {"stdout": bytearray(), "stderr": bytearray()}
    with subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, start_new_session=True) as process:
        try:
            deadline = time.monotonic() + timeout
            with selectors.DefaultSelector() as selector:
                for name, stream in (("stdout", process.stdout), ("stderr", process.stderr)):
                    os.set_blocking(stream.fileno(), False)
                    selector.register(stream, selectors.EVENT_READ, name)
                while selector.get_map() or process.poll() is None:
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        raise RuntimeError(f"Vizier command timed out after {timeout}s")
                    for key, _ in selector.select(min(remaining, 0.1)):
                        chunk = os.read(key.fd, 8192)
                        if not chunk:
                            selector.unregister(key.fileobj)
                            continue
                        output[key.data].extend(chunk)
                        if sum(map(len, output.values())) > max_bytes:
                            raise RuntimeError("Vizier output exceeded the size limit; use compact mode or a narrower shell query")
            return (process.wait(), output["stdout"].decode("utf-8", errors="replace"),
                    output["stderr"].decode("utf-8", errors="replace"))
        finally:
            # Also reap any collectors retaining pipes after the main process exits.
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
