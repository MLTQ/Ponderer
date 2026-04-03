# Image-Orb

`Image-Orb` is a portable `runtime_process` plugin for Ponderer that provides in-line image generation without requiring ComfyUI.

## What It Does

- Generates images through a single tool: `image_orb_generate`
- Supports model families:
  - `flux`
  - `sdxl`
  - `sd15`
- Supports FLUX GGUF transformer files (`.gguf`) when used with a compatible FLUX base model
- Applies optional LoRA stacks from plugin settings
- Returns media metadata so generated images appear directly in chat
- Includes `memory_safe_mode` to auto-cap generation size/steps on constrained devices

## Layout

- `plugin.toml`: Ponderer runtime plugin manifest
- `settings.schema.json`: declarative settings tab schema
- `image_orb/server.py`: stdio JSON-RPC server
- `scripts/install_portable.sh`: local virtualenv + dependency install
- `scripts/run_plugin.sh`: plugin entrypoint used by Ponderer
- `scripts/install_to_ponderer.sh`: dev helper to install into a Ponderer folder

## Portable Usage

1. Run `./scripts/install_portable.sh`
2. Enable the plugin in Ponderer settings
3. Set model family + model reference in the Image-Orb tab
4. Ask the agent to generate an image

For multi-GPU systems, set `Device = CUDA` and `CUDA Device Index = 1` (or another index) in the Image-Orb settings tab to pin image generation to a secondary GPU. On macOS, leave `Device = Auto` so Image-Orb keeps preferring MPS automatically.

`model_ref` accepts either:
- A Hugging Face repo id (for example `stabilityai/stable-diffusion-xl-base-1.0`)
- A local model path (absolute or relative). Relative paths are checked against the Ponderer runtime working directory first, then the plugin directory.

For FLUX GGUF:

1. Set `model_family = flux`
2. Set `model_ref` to your local `.gguf` file path
3. Set `flux_base_model_ref` to a compatible FLUX base model repo/path

If FLUX GGUF fails to load with checkpoint/weights errors:
- Re-run `./scripts/install_portable.sh` so `gguf>=0.10.0` is installed in `.venv`
- Confirm the file is a Diffusers-compatible FLUX transformer GGUF (not an arbitrary merged/full checkpoint conversion)

If LoRA calls fail with `PEFT backend is required`:
- Install/update plugin deps via `./scripts/install_portable.sh` (includes `peft`)
- Or clear/disable `lora_stack_json` in Image-Orb settings

If generation fails because the plugin process exits with `SIGKILL`:
- Keep `memory_safe_mode` enabled (default)
- Reduce `width_default` / `height_default` / `num_inference_steps_default`
- Prefer smaller model families (`sd15` < `sdxl` < `flux`) on low-memory systems

For portable distribution, keep the entire `image-orb` directory together with `.venv` and `data/`.
