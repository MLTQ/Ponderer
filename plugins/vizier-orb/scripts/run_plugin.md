# run_plugin.py

## Purpose
Launches the Vizier adapter without a virtual environment or downloaded Python
dependencies. Uses the shared repository SDK or a portable copy.

## Components
- SDK discovery: `_vendor/ponderer_plugin_sdk` first, sibling `sdk/python` next.
- Entry point: `serve_stdio(build_plugin())` supplies protocol-v1 framing.

## Contracts
Python 3.10+ is required by the shared SDK. Stdout contains only RPC responses.
Source lookup follows the script's resolved location, independent of cwd.
