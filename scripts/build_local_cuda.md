# Build the local CUDA engine

Run `bash scripts/build_local_cuda.sh /path/to/llama.cpp /path/to/new-build`.
The default CUDA architectures are 75 (RTX 2070) and 89 (RTX 4090); the optional
third argument overrides them. `GGML_CUDA_FA_ALL_QUANTS=ON` is necessary for this
llama.cpp revision's Q4_1 CUDA flash-attention kernels. GPU layer placement alone
does not establish that attention uses the GPU. Web UI assets are disabled;
Ponderer stays native Rust/egui with its Python local-provider worker.

This builds a separate engine and starts no runtime or service. Select its
`bin/llama-server` in Settings. Copying that executable alone is insufficient:
its shared libraries must remain beside it. Ponderer's detected-engine button
prefers `~/Code/llama.cpp-cuda/build-ponderer-q4_1/bin/llama-server` when present.

The worker inspects known shared CUDA backends' capability marker without GPU
initialization. Known unsupported Q4_1/Q5 CUDA attention configurations fail
before model startup; unknown/static packages remain unknown, not verified.
Quantization, context size and offload settings are never silently changed.
