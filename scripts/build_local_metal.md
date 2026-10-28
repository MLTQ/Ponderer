# Build the local Metal engine

## Purpose
Build llama-server for macOS, including Apple Silicon M1 and newer, with Metal
and embedded shaders. Requires CMake and Xcode command line tools.

## Usage
From the Ponderer repository:

```bash
git clone --depth 1 https://github.com/ggml-org/llama.cpp.git affect_lab/llama.cpp
bash scripts/build_local_metal.sh affect_lab/llama.cpp affect_lab/llama.cpp/build
```

Use a recent llama.cpp revision that supports the selected GGUF architecture.
The `affect_lab` folder is ignored by Git. The script builds a runtime only;
it does not start a server or change another llama.cpp checkout.
Keep the shared libraries beside the resulting `bin/llama-server`.

## UI contract
Settings → Models detects `affect_lab/llama.cpp/build/bin/llama-server` (under
`PONDERER_AFFECT_DATA_DIR` if set), then Homebrew's `/opt/homebrew/bin/llama-server`
or `/usr/local/bin/llama-server`. Custom executable paths remain editable.
Click **Scan GPUs**, select the Apple **MTL0** device, then **Load local model**
and **Use for this session**. Metal uses shared system memory; start with the
default 16,384-token context. The 200k preset is an explicit large allocation.

The desktop owns the backend, worker, supervisor and model. Closing it stops all
of them. GPU probes are bounded and owned too. MLX and multimodal projector
loading are separate features; this path loads text GGUF inference.

## Verification
On 2026-10-28, llama.cpp `c3df4b7` built with this script passed the isolated real
chat regression on Apple M1 Pro (`MTL0`) with LFM3-1.7B-Uncensored-AD-Q6_K.gguf,
16,384 context tokens, f16 K/V, automatic flash attention and all GPU layers.
Normal chat, structured tool call/result, streaming, engine reuse and cleanup
passed. The desktop/backend lifetime fixture also passed on macOS.
