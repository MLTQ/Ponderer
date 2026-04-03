# server.py

## Purpose
Implements the `Image-Orb` stdio JSON-RPC server for Ponderer. It loads diffusion pipelines on demand, applies LoRA stacks from plugin settings, and exposes image-generation tools.

## Components

### `PluginState`
- **Does**: Stores persisted settings and lazily loaded runtime state (torch module, pipeline classes, active pipeline family/ref/device).
- **Interacts with**: all RPC methods.

### `main` / `handle_rpc_line` / `dispatch`
- **Does**: Runs the newline-delimited JSON-RPC loop and routes methods (`plugin.handshake`, `plugin.configure`, `plugin.handle_event`, `plugin.get_prompt_contributions`, `plugin.invoke_tool`).
- **Interacts with**: Ponderer runtime plugin host.

### `handshake`
- **Does**: Declares plugin metadata and exposes only the LLM-facing generation manifest (`image_orb_generate`). Internal diagnostics remain callable but are intentionally omitted from handshake to keep command surface minimal.
- **Interacts with**: runtime tool-proxy registration.

### `load_pipeline`
- **Does**: Loads the configured diffusion family (`flux`, `sdxl`, `sd15`) from model ref/path, applies device/dtype strategy, optional attention/offload toggles, and caches the active pipeline. Pipeline swaps now clear the previously loaded pipeline before loading a new one to prevent temporary double-residency spikes. For single-file checkpoints, it resolves a plugin-local Diffusers config directory and passes it to `from_single_file` so local-only operation does not trigger Hub lookups. For FLUX `.gguf` model refs, it composes a quantized transformer into a FLUX base pipeline via GGUF quantization config.
- **Interacts with**: `diffusers` pipeline classes and plugin-local cache path.
- **Rationale**: Rejects unsupported local file sources early (before calling `from_pretrained`) so users get deterministic local-path guidance rather than generic remote URL validation errors.

### `ensure_single_file_config_dir` / `bootstrap_single_file_config_dir`
- **Does**: Ensures a local Diffusers config cache exists for local single-file checkpoints (`.safetensors` / `.ckpt`). When `local_files_only=false`, it bootstraps JSON/text config artifacts from a family-default (or user-configured) repo into `single_file_config_dir`; when `local_files_only=true`, it fails fast with an actionable missing-cache error.
- **Interacts with**: `huggingface_hub.snapshot_download`, `load_pipeline`, and settings schema fields (`single_file_config_dir`, `single_file_config_repo`).

### `load_flux_pipeline_with_gguf` / `ensure_gguf_runtime`
- **Does**: Validates GGUF runtime prerequisites (`gguf>=0.10.0`), loads a FLUX transformer from single-file GGUF, then injects it into a FLUX base pipeline.
- **Interacts with**: `diffusers.FluxTransformer2DModel`, `GGUFQuantizationConfig`, Python package `gguf`.

### `apply_lora_stack`
- **Does**: Parses settings-backed JSON LoRA stack and activates adapters with `load_lora_weights` + `set_adapters` (or `fuse_lora` fallback).
- **Interacts with**: loaded diffusion pipeline.
- **Rationale**: Handles missing PEFT backend explicitly; empty/disabled LoRA stacks should not fail plugin startup.

### `generate_image`
- **Does**: Validates tool arguments, derives family-aware defaults, optionally applies memory-safe width/height/step caps per model family + device, enforces prompt-length and RSS budget guardrails, invokes pipeline sampling under torch inference mode, and writes output image metadata for chat rendering (including runtime telemetry such as `pipeline_load_count`, `rss_before_mb`, and `rss_after_mb`).
- **Interacts with**: tool invocation path and media payload contract.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| Ponderer runtime host | One JSON response line per request and stable method names | Changing transport or method names |
| Tool loop | Tool name `image_orb_generate` and argument schema remain stable | Renaming tools or required params |
| Operators | Relative cache/output paths resolve inside plugin repo for portability | Switching to machine-global paths |
| Offline operators | Single-file checkpoints use local Diffusers config under `single_file_config_dir` | Removing bootstrap logic or changing default config layout |

## Notes
- Heavy imports (`torch`, `diffusers`) are lazy so handshake remains lightweight.
- LoRA stack configuration is JSON for now because the generic settings UI currently supports scalar fields only.
- Prompt handling is family-aware via contributions + defaults; generation still accepts direct prompt overrides per call.
- `memory_safe_mode` (default `true`) clamps oversized requests on CPU/MPS and returns warning strings in tool output metadata when limits are applied.
- Explicit CUDA mode now honors the settings-tab `cuda_device_index` so multi-GPU machines can pin Image-Orb to `cuda:1`, `cuda:2`, and so on; `auto` still prefers `cuda:0` when CUDA is present and otherwise falls back to MPS/CPU.
- Auto dtype now uses `float16` on MPS/CUDA (and `float32` on CPU) to reduce unified-memory pressure on local machines.
- CPU fallback on meta-tensor failures is explicit (`allow_cpu_fallback=true`) rather than automatic, preventing surprise host-RAM spikes.
- A soft process-memory budget (`max_rss_mb`) blocks new generation when already over budget and forces unload if a generation result exceeds budget.
- FLUX GGUF mode expects `model_family=flux`, `model_ref` ending in `.gguf`, and a compatible `flux_base_model_ref`.
- FLUX GGUF mode requires Python package `gguf>=0.10.0` in the plugin venv.
- LoRA activation requires Python package `peft`; if absent, Image-Orb returns an actionable plugin error instead of a low-level backend exception.
- Local model refs now normalize `~`, env vars, and `file://` URIs before loading, and resolve relative paths against both the runtime working directory and plugin directory.
- If a value looks like a local path but does not exist, the plugin fails early with a path-focused error (including the checked locations) instead of surfacing a remote-URL validation error.
- For local single-file SDXL/SD1.5 checkpoints, Image-Orb now uses a plugin-local Diffusers config cache (`single_file_config_dir`, default `./data/models/diffusers-configs`) and passes it explicitly to Diffusers so runtime does not depend on Hub lookups once bootstrapped.
