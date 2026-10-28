"""Behavior and protocol coverage without collecting real machine data."""

import importlib.util
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT), str(ROOT.parent / "sdk" / "python")]

from ponderer_plugin_sdk import PromptSlot
from ponderer_plugin_sdk.testing import FakeHost, PluginConformanceMixin
from vizier_orb.plugin import build_plugin, GUIDANCE
from vizier_orb.runner import observe, resolve_vz, run_bounded


def executable(directory, name, body):
    path = Path(directory) / name
    path.write_text(f"#!{sys.executable}\n" + body)
    path.chmod(0o755)
    return path


class ProtocolTests(PluginConformanceMixin, unittest.TestCase):
    def make_plugin(self):
        return build_plugin()

    def test_static_tools_match_handshake_and_guidance_fits_host_budget(self):
        host = FakeHost(build_plugin())
        self.assertEqual(json.loads((ROOT / "tools.json").read_text())["tools"],
                         host.handshake()["tools"])
        # The host unifies effects by ID across the package, not by tool.
        effects = {}
        for tool in host.handshake()["tools"]:
            for effect in tool["effects"]:
                if effect["id"] in effects:
                    self.assertEqual(effects[effect["id"]], effect)
                effects[effect["id"]] = effect
        self.assertLessEqual(len(GUIDANCE), 300)
        for slot in (PromptSlot.ENGAGED_INSTRUCTIONS, PromptSlot.AMBIENT_INSTRUCTIONS,
                     PromptSlot.ORIENTATION_CONTEXT):
            self.assertEqual(host.prompt(slot)[0]["text"], GUIDANCE)
            self.assertEqual(host.prompt(slot, legacy=True)[0]["text"], GUIDANCE)

    def test_bad_invocation_returns_actionable_tool_error(self):
        result = FakeHost(build_plugin()).invoke_tool("vz_ssh", {"host": "-oProxyCommand=bad"})
        self.assertEqual(result["kind"], "error")
        self.assertIn("host must be", result["text"])


class RunnerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="vizier tests '")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.fake = executable(self.root, "fake vz", "import json,sys\nprint(json.dumps({'schema_version':1,'args':sys.argv[1:]}))\n")
        self.settings = {"vz_path": str(self.fake)}

    def test_compact_wake_snapshot_and_verbose_through_protocol(self):
        host = FakeHost(build_plugin())
        host.configure(self.settings)
        for arguments, expected in (({}, ["--no-public-ip", "wake"]),
                                    ({"mode": "snapshot"}, ["--no-public-ip"]),
                                    ({"mode": "snapshot", "verbose": True}, ["--no-public-ip", "--verbose"])):
            result = host.invoke_tool("vz", arguments)
            self.assertEqual(result["kind"], "json")
            self.assertEqual(result["data"]["target"], "local")
            self.assertEqual(result["data"]["observation"]["args"], expected)

    def test_discovery_from_gui_path_and_override_failure(self):
        user_bin = self.root / ".local/bin"
        user_bin.mkdir(parents=True)
        found = executable(user_bin, "vz", "pass\n")
        with patch.dict(os.environ, {"PATH": "/usr/bin:/bin", "PONDERER_VZ_BIN": ""}), patch("pathlib.Path.home", return_value=self.root):
            self.assertEqual(resolve_vz({}), str(found))
            with self.assertRaisesRegex(ValueError, "Configured"):
                resolve_vz({"vz_path": str(self.root / "missing")})

    def test_missing_local_returns_setup_without_running_a_process(self):
        with patch("vizier_orb.runner.resolve_vz", return_value=None), patch("vizier_orb.runner.run_bounded") as run:
            result = observe({}, {})
        run.assert_not_called()
        self.assertEqual(result["status"], "not_installed")
        self.assertIn("raw.githubusercontent.com/MLTQ/vizier", result["install_command"])

    def test_ssh_arguments_keep_host_and_remote_script_separate(self):
        calls = []
        def capture(command):
            calls.append(command)
            return 0, '{"schema_version":1,"machine":{"hostname":"remote"}}', ""
        with patch("vizier_orb.runner.run_bounded", side_effect=capture):
            result = observe({"host":"alice@server", "mode":"wake"}, {}, remote=True)
            observe({"host":"server", "port":2222}, {}, remote=True)
        self.assertEqual(result["target"], "alice@server")
        self.assertNotIn("-p", calls[0])
        self.assertEqual(calls[0][-3:-1], ["--", "alice@server"])
        self.assertIn("BatchMode=yes", calls[0])
        self.assertNotIn("StrictHostKeyChecking=no", calls[0])
        self.assertEqual(calls[1][calls[1].index("-p")+1], "2222")
        # Actually execute the generated script in a fake remote HOME, with vz
        # absent from PATH. This checks quoting and ~/.local/bin discovery.
        remote_home = self.root / "remote home"
        (remote_home / ".local/bin").mkdir(parents=True)
        executable(remote_home / ".local/bin", "vz", "import json,sys\nprint(json.dumps({'schema_version':1,'args':sys.argv[1:]}))\n")
        env = dict(os.environ, HOME=str(remote_home), PATH="/usr/bin:/bin")
        ran = subprocess.run(["/bin/sh", "-c", calls[0][-1]], env=env, capture_output=True, text=True, check=True)
        self.assertEqual(json.loads(ran.stdout)["args"], ["--no-public-ip", "wake"])

    def test_remote_missing_and_ssh_errors_are_distinct(self):
        with patch("vizier_orb.runner.run_bounded", return_value=(127, "", "PONDERER_VZ_MISSING\n")):
            result = observe({"host":"server", "port":2222}, {}, remote=True)
        self.assertEqual(result["target"], "server")
        self.assertEqual(result["status"], "not_installed")
        install = shlex.split(result["install_command"])
        self.assertEqual(install[-3:-1], ["--", "server"])
        self.assertIn("2222", install)
        with patch("vizier_orb.runner.run_bounded", return_value=(255, "", "Host key verification failed")):
            with self.assertRaisesRegex(RuntimeError, "Host key verification failed"):
                observe({"host":"server"}, {}, remote=True)

    def test_invalid_arguments_never_spawn(self):
        bad = [{"host":"server; touch /tmp/x"}, {"host":"$(id)"}, {"host":"server\nother"},
               {"host":"-proxy"}, {"host":"server", "port":True}, {"host":"server", "port":65536},
               {"host":"server", "mode":"watch"}, {"host":"server", "verbose":"yes"},
               {"host":"server", "command":"id"}]
        with patch("vizier_orb.runner.run_bounded") as run:
            for arguments in bad:
                with self.subTest(arguments=arguments), self.assertRaises(ValueError):
                    observe(arguments, {}, remote=True)
        run.assert_not_called()

    def test_bad_json_and_nonzero_exit(self):
        for result, message in (((0, "login banner\n{}", ""), "invalid JSON"),
                                ((0, "[]", ""), "unrecognized observation"),
                                ((1, "", "collector failed"), "collector failed")):
            with patch("vizier_orb.runner.run_bounded", return_value=result):
                with self.assertRaisesRegex(RuntimeError, message):
                    observe({}, self.settings)

    def test_output_limit_and_timeout(self):
        with self.assertRaisesRegex(RuntimeError, "size limit"):
            run_bounded([sys.executable, "-c", "print('雪'*1000)"], max_bytes=100)
        start = time.monotonic()
        with self.assertRaisesRegex(RuntimeError, "timed out"):
            run_bounded([sys.executable, "-c", "import time; time.sleep(30)"], timeout=0.15)
        self.assertLess(time.monotonic() - start, 3)

    def test_timeout_kills_collectors_retaining_pipes(self):
        marker = self.root / "child survived"
        child = "import pathlib,time; time.sleep(.8); pathlib.Path(" + repr(str(marker)) + ").touch()"
        parent = "import subprocess,sys; subprocess.Popen([sys.executable,'-c'," + repr(child) + "])"
        with self.assertRaisesRegex(RuntimeError, "timed out"):
            run_bounded([sys.executable, "-c", parent], timeout=0.15)
        time.sleep(1)
        self.assertFalse(marker.exists())


class InstallTests(unittest.TestCase):
    def test_installed_package_launches_from_unrelated_cwd(self):
        spec = importlib.util.spec_from_file_location("vizier_installer", ROOT / "scripts/install_to_ponderer.py")
        installer = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(installer)
        with tempfile.TemporaryDirectory() as directory:
            target = installer.install(Path(directory) / "plugins")
            self.assertTrue((target / "_vendor/ponderer_plugin_sdk/server.py").is_file())
            request = json.dumps({"id":"test", "protocol_version":1, "method":"plugin.handshake", "params":{"supported_protocol_versions":[1]}})
            output = subprocess.run([sys.executable, str(target / "scripts/run_plugin.py")], input=request+"\n", cwd=directory, capture_output=True, text=True, check=True)
            response = json.loads(output.stdout)
            self.assertTrue(response["ok"], response)
            self.assertEqual(response["result"]["capabilities"]["tools"], ["vz", "vz_ssh"])
            installer.install(Path(directory) / "plugins")
            (target / "plugin.toml").write_text("id = 'different'\n")
            with self.assertRaisesRegex(ValueError, "differs"):
                installer.install(Path(directory) / "plugins")


if __name__ == "__main__":
    unittest.main()
