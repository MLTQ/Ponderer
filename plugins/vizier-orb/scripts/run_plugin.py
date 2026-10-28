"""Launch Vizier with the shared SDK, from a checkout or a portable install."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sdk_root = ROOT / "_vendor"
if not (sdk_root / "ponderer_plugin_sdk").is_dir():
    sdk_root = ROOT.parent / "sdk" / "python"
sys.path[:0] = [str(ROOT), str(sdk_root)]

from ponderer_plugin_sdk import serve_stdio
from vizier_orb.plugin import build_plugin

if __name__ == "__main__":
    raise SystemExit(serve_stdio(build_plugin()))
