#!/usr/bin/env bash
# Build a Metal engine without replacing another checkout or starting a service.
set -euo pipefail
if [[ $# -ne 2 ]]; then
    echo 'Usage: build_local_metal.sh LLAMA_SOURCE BUILD_DIRECTORY' >&2
    exit 2
fi
if [[ "$(uname -s)" != Darwin ]]; then
    echo 'The Metal engine requires macOS.' >&2
    exit 2
fi
cmake -S "$1" -B "$2" -DGGML_METAL=ON -DGGML_METAL_EMBED_LIBRARY=ON \
    -DCMAKE_BUILD_TYPE=Release -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_EXAMPLES=OFF \
    -DLLAMA_BUILD_SERVER=ON -DLLAMA_BUILD_UI=OFF -DLLAMA_USE_PREBUILT_UI=OFF
cmake --build "$2" --target llama-server --parallel 4
