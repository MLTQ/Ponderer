# validate_affect_ui_lifetime.py

Runs a compiled desktop binary's backend-only mode in a temporary isolated
directory with fixture GGUF/vector files and a fake inference server. It does not
read or modify user configuration, load real model weights or contact Telegram.

Checks the authenticated GPU-inventory route, explicit single-device all-GPU
selection and 200k/unified-Q4_1/flash settings, explicit load survival across HTTP/job
threads, transient provider isolation when saving config, persisted appearance
through the actual config API/TOML path, signed-study mix preservation and parent
pipe closure terminating an active study and every inference descendant.

Run: `python3 scripts/validate_affect_ui_lifetime.py /absolute/path/to/ponderer`.
Requires local socket/process access. Cleanup targets only the verified fixture
processes created by this run.
