"""Copy a self-contained Vizier plugin into a Ponderer plugin directory."""

import argparse
from pathlib import Path
import shutil


def install(plugin_directory):
    source = Path(__file__).resolve().parents[1]
    target = Path(plugin_directory).expanduser().resolve() / "vizier-orb"
    if target == source or source in target.parents:
        raise ValueError("Choose the running Ponderer's plugins directory, not this source package")
    if target.is_symlink():
        raise ValueError(f"Refusing to overwrite a symlink: {target}")
    if target.exists():
        if not (target / "plugin.toml").is_file() or (target / "plugin.toml").read_text() != (source / "plugin.toml").read_text():
            raise ValueError(f"Existing package differs; move it aside before installing: {target}")
        if any(path.is_symlink() for path in target.rglob("*")):
            raise ValueError(f"Refusing to overwrite a package containing symlinks: {target}")
    sdk = source.parent / "sdk" / "python" / "ponderer_plugin_sdk"
    if not sdk.is_dir():
        raise ValueError("Run this installer from the Ponderer repository with plugins/sdk/python present")
    ignore = shutil.ignore_patterns("__pycache__", "*.pyc", "tests", "_vendor")
    shutil.copytree(source, target, dirs_exist_ok=True, ignore=ignore)
    shutil.copytree(sdk, target / "_vendor" / "ponderer_plugin_sdk",
                    dirs_exist_ok=True, ignore=ignore)
    return target


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plugin_directory", help="plugins directory beside ponderer_config.toml")
    args = parser.parse_args()
    print(f"Installed Vizier: {install(args.plugin_directory)}")
