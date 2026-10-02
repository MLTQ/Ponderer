#!/usr/bin/env bash
# Build alongside the source without replacing an existing engine or starting a service.
set -euo pipefail
if [[ $# -lt 2 || $# -gt 3 ]]; then
    echo 'Usage: build_local_cuda.sh LLAMA_SOURCE NEW_BUILD_DIRECTORY [CUDA_ARCHITECTURES]' >&2
    exit 2
fi
cmake -S "$1" -B "$2" -DGGML_CUDA=ON -DGGML_CUDA_FA_ALL_QUANTS=ON \
    "-DCMAKE_CUDA_ARCHITECTURES=${3:-75;89}" -DCMAKE_BUILD_TYPE=Release \
    -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_EXAMPLES=OFF -DLLAMA_BUILD_SERVER=ON \
    -DLLAMA_BUILD_UI=OFF -DLLAMA_USE_PREBUILT_UI=OFF
cmake --build "$2" --target llama-server --parallel 4
