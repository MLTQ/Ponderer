import contextlib
import json
import os
import sys
import gc
import subprocess
from importlib import metadata as importlib_metadata
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from urllib.parse import urlparse, unquote

from image_orb import __version__

REPO_ROOT = Path(__file__).resolve().parents[1]
SUPPORTED_MODEL_FAMILIES = {"flux", "sdxl", "sd15"}
LOCAL_MODEL_FILE_SUFFIXES = (
    ".safetensors",
    ".ckpt",
    ".pt",
    ".pth",
    ".bin",
    ".gguf",
    ".onnx",
)
DEFAULT_MAX_PROMPT_CHARS = 1200
DEFAULT_MAX_RSS_MB = 20_000
DEFAULT_SINGLE_FILE_CONFIG_DIR = "./data/models/diffusers-configs"
DEFAULT_SINGLE_FILE_CONFIG_REPO_BY_FAMILY = {
    "sdxl": "stabilityai/stable-diffusion-xl-base-1.0",
    "sd15": "runwayml/stable-diffusion-v1-5",
    "flux": "black-forest-labs/FLUX.1-dev",
}
SINGLE_FILE_CONFIG_ALLOW_PATTERNS = [
    "model_index.json",
    "*.json",
    "**/*.json",
    "*.txt",
    "**/*.txt",
    "*.model",
    "**/*.model",
]


@dataclass
class PluginState:
    settings: dict[str, Any] = field(default_factory=dict)
    instance_id: str = field(
        default_factory=lambda: f"image-orb-{os.getpid()}-{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')}"
    )
    startup_utc: str = field(default_factory=lambda: datetime.now(timezone.utc).isoformat())
    pipeline_load_count: int = 0
    last_pipeline_loaded_utc: str | None = None
    loaded_pipeline_family: str | None = None
    loaded_pipeline_ref: str | None = None
    loaded_pipeline_device: str | None = None
    loaded_pipeline: Any | None = None
    torch_module: Any | None = None
    pipeline_classes: dict[str, Any] = field(default_factory=dict)
    pillow_image_class: Any | None = None

    def merged_settings(self) -> dict[str, Any]:
        merged = {
            "enabled": False,
            "model_family": "sdxl",
            "model_ref": "stabilityai/stable-diffusion-xl-base-1.0",
            "flux_base_model_ref": "black-forest-labs/FLUX.1-dev",
            "flux_gguf_compute_dtype": "auto",
            "cache_dir": "./data/models",
            "output_dir": "./data/output",
            "device": "auto",
            "cuda_device_index": 0,
            "dtype": "auto",
            "attention_impl": "auto",
            "enable_model_cpu_offload": False,
            "memory_safe_mode": True,
            "allow_cpu_fallback": False,
            "unload_after_generation": False,
            "max_prompt_chars": DEFAULT_MAX_PROMPT_CHARS,
            "max_rss_mb": DEFAULT_MAX_RSS_MB,
            "local_files_only": False,
            "single_file_config_dir": DEFAULT_SINGLE_FILE_CONFIG_DIR,
            "single_file_config_repo": "",
            "width_default": 768,
            "height_default": 768,
            "num_inference_steps_default": 24,
            "guidance_scale_default": 4.0,
            "negative_prompt_default": "",
            "seed_default": -1,
            "save_format": "png",
            "lora_stack_json": "[]",
            "prompt_hint": (
                "FLUX prefers natural language scene descriptions. "
                "SDXL/SD1.5 often benefit from explicit negative prompts."
            ),
        }
        merged.update(self.settings)
        return merged


STATE = PluginState()


def main() -> int:
    for raw_line in sys.stdin:
        line = raw_line.strip()
        if not line:
            continue

        response = handle_rpc_line(line)
        sys.stdout.write(json.dumps(response, ensure_ascii=False) + "\n")
        sys.stdout.flush()

    return 0


def handle_rpc_line(line: str) -> dict[str, Any]:
    request_id = "unknown"
    try:
        payload = json.loads(line)
        request_id = str(payload.get("id", "unknown"))
        method = payload.get("method")
        params = payload.get("params") or {}
        result = dispatch(method, params)
        return {"id": request_id, "ok": True, "result": result}
    except Exception as exc:  # pragma: no cover - runtime surface
        return {
            "id": request_id,
            "ok": False,
            "error": {
                "code": "plugin_error",
                "message": str(exc),
            },
        }


def dispatch(method: str, params: dict[str, Any]) -> Any:
    if method == "plugin.handshake":
        return handshake()
    if method == "plugin.configure":
        return configure(params)
    if method == "plugin.handle_event":
        return handle_event(params)
    if method == "plugin.get_prompt_contributions":
        return get_prompt_contributions(params)
    if method == "plugin.invoke_tool":
        return invoke_tool(params)
    raise ValueError(f"unknown method: {method}")


def handshake() -> dict[str, Any]:
    tool_schema = {
        "type": "object",
        "properties": {
            "prompt": {
                "type": "string",
                "description": "Positive prompt describing the image to generate.",
            },
            "negative_prompt": {
                "type": "string",
                "description": "Optional negative prompt (best with SDXL/SD1.5).",
            },
            "model_family": {
                "type": "string",
                "description": "Optional override: flux, sdxl, or sd15.",
            },
            "width": {
                "type": "integer",
                "description": "Output width override.",
            },
            "height": {
                "type": "integer",
                "description": "Output height override.",
            },
            "num_inference_steps": {
                "type": "integer",
                "description": "Sampling steps override.",
            },
            "guidance_scale": {
                "type": "number",
                "description": "Classifier guidance scale override.",
            },
            "seed": {
                "type": "integer",
                "description": "Deterministic seed override. -1 means random.",
            },
        },
        "required": ["prompt"],
    }
    return {
        "id": "image-orb",
        "name": "Image-Orb",
        "version": __version__,
        "capabilities": {
            "tools": ["image_orb_generate"],
            "event_hooks": ["settings_changed"],
            "prompt_slots": ["engaged.instructions"],
        },
        "tools": [
            {
                "name": "image_orb_generate",
                "description": (
                    "Generate an image using the configured model family "
                    "(FLUX, SDXL, or SD1.5), with optional negative prompt and overrides."
                ),
                "parameters": tool_schema,
                "requires_approval": False,
                "category": "general",
            },
        ],
    }


def configure(params: dict[str, Any]) -> dict[str, Any]:
    settings = params.get("settings")
    if settings is None:
        settings = {}
    if not isinstance(settings, dict):
        raise ValueError("plugin.configure expects an object settings payload")

    STATE.settings = dict(settings)
    clear_loaded_pipeline()
    return {"configured": True}


def handle_event(params: dict[str, Any]) -> dict[str, Any]:
    event_name = params.get("event")
    if event_name == "settings_changed":
        return {"state_changed": False, "summary": "Image-Orb settings reloaded."}
    return {"state_changed": False}


def get_prompt_contributions(params: dict[str, Any]) -> dict[str, Any]:
    slot = params.get("slot")
    if slot not in ("engaged_instructions", "engaged.instructions"):
        return {"contributions": []}

    settings = STATE.merged_settings()
    family = normalize_model_family(str(settings.get("model_family") or "sdxl"))
    family_hint = {
        "flux": (
            "Use natural-language scene descriptions with concrete style/lighting/camera cues. "
            "Negative prompts are usually optional."
        ),
        "sdxl": (
            "Use precise descriptive prompts and include negative prompts when artifacts are likely."
        ),
        "sd15": (
            "Use concise subject/style prompts and rely on strong negative prompts to avoid common artifacts."
        ),
    }[family]
    user_hint = str(settings.get("prompt_hint") or "").strip()
    hint_suffix = f" Additional hint: {user_hint}" if user_hint else ""

    return {
        "contributions": [
            {
                "plugin_id": "image-orb",
                "slot": "engaged_instructions",
                "kind": "instruction",
                "text": (
                    "When the user asks for an image, call `image_orb_generate`."
                    f" Current model family is `{family}`. {family_hint}{hint_suffix}"
                ),
                "priority": 45,
                "max_chars": 320,
            }
        ]
    }


def invoke_tool(params: dict[str, Any]) -> dict[str, Any]:
    tool_name = params.get("tool")
    arguments = params.get("arguments") or {}
    if not isinstance(arguments, dict):
        raise ValueError("plugin.invoke_tool arguments must be an object")

    if tool_name == "image_orb_ensure_model":
        return model_status(arguments)

    if tool_name == "image_orb_generate":
        return generate_image(arguments)

    raise ValueError(f"unknown tool: {tool_name}")


def model_status(arguments: dict[str, Any]) -> dict[str, Any]:
    preload = parse_bool(arguments.get("preload"), False)
    settings = STATE.merged_settings()
    model_family = normalize_model_family(str(settings.get("model_family") or "sdxl"))
    model_ref = str(settings.get("model_ref") or "").strip()
    flux_base_model_ref = str(settings.get("flux_base_model_ref") or "").strip()
    expected_ref_key = pipeline_cache_ref_key(model_family, model_ref, flux_base_model_ref)
    already_loaded = (
        STATE.loaded_pipeline is not None
        and STATE.loaded_pipeline_family == model_family
        and STATE.loaded_pipeline_ref == expected_ref_key
    )

    if preload and not already_loaded:
        pipeline = load_pipeline(model_family)
        return {
            "kind": "json",
            "data": {
                "status": "ok",
                "action": "preloaded",
                "model_family": model_family,
                "model_ref": model_ref,
                "model_loaded": pipeline is not None,
                "device": STATE.loaded_pipeline_device,
                "already_loaded": already_loaded,
                **runtime_status_fields(),
            },
        }

    return {
        "kind": "json",
        "data": {
            "status": "ok",
            "action": "status_only",
            "model_family": model_family,
            "model_ref": model_ref,
            "model_loaded": STATE.loaded_pipeline is not None,
            "device": STATE.loaded_pipeline_device,
            "already_loaded": already_loaded,
            **runtime_status_fields(),
        },
    }


def generate_image(arguments: dict[str, Any]) -> dict[str, Any]:
    settings = STATE.merged_settings()
    prompt = str(arguments.get("prompt") or "").strip()
    if not prompt:
        raise ValueError("prompt is required")
    prompt, prompt_was_truncated = bound_prompt_text(prompt, settings)
    rss_before_mb = current_rss_mb()
    max_rss_mb = resolve_max_rss_mb(settings)
    if rss_before_mb is not None and rss_before_mb >= max_rss_mb:
        clear_loaded_pipeline()
        raise RuntimeError(
            f"Image-Orb refused generation because process RSS is already high "
            f"({rss_before_mb:.0f}MB >= budget {max_rss_mb:.0f}MB)."
        )

    model_family = normalize_model_family(
        str(arguments.get("model_family") or settings.get("model_family") or "sdxl")
    )
    model_ref = str(settings.get("model_ref") or "").strip()
    if not model_ref:
        raise ValueError("model_ref is required")

    width = clamp_dimension(
        parse_int(arguments.get("width"), parse_int(settings.get("width_default"), 1024)),
        model_family,
    )
    height = clamp_dimension(
        parse_int(arguments.get("height"), parse_int(settings.get("height_default"), 1024)),
        model_family,
    )
    steps = max(1, parse_int(arguments.get("num_inference_steps"), parse_int(settings.get("num_inference_steps_default"), 28)))
    guidance = max(
        0.0,
        parse_float(
            arguments.get("guidance_scale"),
            parse_float(settings.get("guidance_scale_default"), default_guidance_for_family(model_family)),
        ),
    )
    negative_prompt = str(
        arguments.get("negative_prompt") or settings.get("negative_prompt_default") or ""
    ).strip()

    seed = parse_int(arguments.get("seed"), parse_int(settings.get("seed_default"), -1))
    if seed < 0:
        seed = int.from_bytes(os.urandom(8), "big") % 2_147_483_647

    pipeline = load_pipeline(model_family)
    apply_lora_stack(pipeline, settings)

    torch_module = ensure_torch()
    memory_safe_mode = parse_bool(settings.get("memory_safe_mode"), True)
    applied_warnings: list[str] = []
    if memory_safe_mode:
        width, height, steps, applied_warnings = apply_memory_safe_limits(
            model_family,
            STATE.loaded_pipeline_device or resolve_effective_device(settings, torch_module),
            width,
            height,
            steps,
        )

    generator = build_generator(torch_module, STATE.loaded_pipeline_device or "cpu", seed)

    positive_prompt = build_positive_prompt(model_family, prompt)
    invoke_kwargs: dict[str, Any] = {
        "prompt": positive_prompt,
        "num_inference_steps": steps,
        "guidance_scale": guidance,
        "width": width,
        "height": height,
    }
    if generator is not None:
        invoke_kwargs["generator"] = generator
    if model_family in {"sdxl", "sd15"} and negative_prompt:
        invoke_kwargs["negative_prompt"] = negative_prompt

    try:
        result = run_pipeline_inference(pipeline, invoke_kwargs)
    except RuntimeError as exc:
        if "meta tensor" not in str(exc).lower():
            raise
        if not parse_bool(settings.get("allow_cpu_fallback"), False):
            raise RuntimeError(
                "Image-Orb generation hit a meta-tensor failure. "
                "CPU fallback is disabled to avoid large host-RAM spikes. "
                "Try setting device='mps' with dtype='float16' or enable allow_cpu_fallback explicitly."
            ) from exc
        print("Image-Orb: meta-tensor failure; retrying with CPU fallback.", file=sys.stderr)
        clear_loaded_pipeline()
        pipeline = load_pipeline(model_family, force_fallback=True)
        apply_lora_stack(pipeline, settings)
        result = run_pipeline_inference(pipeline, invoke_kwargs)

    image = extract_result_image(result)
    output_format = normalize_save_format(str(settings.get("save_format") or "png"))
    output_dir = ensure_directory(
        resolve_repo_path(str(settings.get("output_dir") or "./data/output"))
    )
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    output_name = f"image_orb_{model_family}_{timestamp}.{output_format}"
    output_path = output_dir / output_name
    image.save(output_path)
    rss_after_mb = current_rss_mb()
    budget_exceeded = rss_after_mb is not None and rss_after_mb >= max_rss_mb
    unload_after_generation = parse_bool(settings.get("unload_after_generation"), False)
    if unload_after_generation or budget_exceeded:
        clear_loaded_pipeline()
    else:
        release_torch_memory()

    return {
        "kind": "json",
        "data": {
            "status": "ok",
            "tool": "image_orb_generate",
            "model_family": model_family,
            "model_ref": model_ref,
            "seed": seed,
            "width": width,
            "height": height,
            "steps": steps,
            "guidance_scale": guidance,
            "path": str(output_path),
            "warnings": applied_warnings,
            "prompt_chars": len(prompt),
            "prompt_was_truncated": prompt_was_truncated,
            "rss_before_mb": rss_before_mb,
            "rss_after_mb": rss_after_mb,
            "max_rss_mb": max_rss_mb,
            "budget_exceeded": budget_exceeded,
            **runtime_status_fields(),
            "media": [
                {
                    "filename": output_name,
                    "path": str(output_path),
                    "media_kind": "image",
                    "mime_type": mime_type_for_format(output_format),
                    "source": "image-orb",
                }
            ],
        },
    }


def load_pipeline(
    requested_family: str | None = None,
    force_fallback: bool = False,
) -> Any:
    settings = STATE.merged_settings()
    model_family = normalize_model_family(
        requested_family or str(settings.get("model_family") or "sdxl")
    )
    model_ref = str(settings.get("model_ref") or "").strip()
    if not model_ref:
        raise ValueError("model_ref is required")
    flux_base_model_ref = str(settings.get("flux_base_model_ref") or "").strip()
    expected_ref_key = pipeline_cache_ref_key(
        model_family, model_ref, flux_base_model_ref
    )

    if (
        not force_fallback
        and STATE.loaded_pipeline is not None
        and STATE.loaded_pipeline_family == model_family
        and STATE.loaded_pipeline_ref == expected_ref_key
    ):
        return STATE.loaded_pipeline
    if STATE.loaded_pipeline is not None:
        clear_loaded_pipeline()

    torch_module, pipeline_classes = ensure_diffusers_runtime()
    pipeline_class = pipeline_classes[model_family]
    cache_dir = ensure_directory(
        resolve_repo_path(str(settings.get("cache_dir") or "./data/models"))
    )

    os.environ["HF_HOME"] = str(cache_dir)
    os.environ["HF_HUB_CACHE"] = str(cache_dir)
    os.environ["TRANSFORMERS_CACHE"] = str(cache_dir)

    model_source = resolve_model_source(model_ref)
    model_source_path = Path(model_source)
    if model_family == "flux" and is_flux_gguf_source(model_source):
        pipeline = load_flux_pipeline_with_gguf(
            pipeline_class,
            model_source,
            settings,
            torch_module,
            force_fallback,
            cache_dir,
        )
    else:
        load_kwargs = build_pipeline_load_kwargs(
            settings, torch_module, force_fallback, cache_dir
        )
        try:
            with contextlib.redirect_stdout(sys.stderr):
                if model_source_path.is_file() and hasattr(pipeline_class, "from_single_file"):
                    single_file_config_dir = ensure_single_file_config_dir(
                        model_family,
                        model_source_path,
                        settings,
                        cache_dir,
                    )
                    if single_file_config_dir is not None:
                        load_kwargs["config"] = str(single_file_config_dir)
                    pipeline = pipeline_class.from_single_file(model_source, **load_kwargs)
                elif model_source_path.is_file():
                    raise RuntimeError(
                        "Configured model_ref resolves to a local file, but this model family "
                        "requires either a directory/repo model source or a supported single-file "
                        "loader."
                    )
                else:
                    pipeline = pipeline_class.from_pretrained(model_source, **load_kwargs)
        except Exception as exc:
            raise RuntimeError(
                "Failed to load model source "
                f"'{model_source}' for family '{model_family}'. "
                "If this is local, verify the path exists and is accessible."
            ) from exc

    device_target = "cpu" if force_fallback else resolve_effective_device(settings, torch_module)
    with contextlib.redirect_stdout(sys.stderr):
        pipeline = pipeline.to(device_target)
    apply_attention_mode(pipeline, str(settings.get("attention_impl") or "auto").strip().lower())
    maybe_enable_cpu_offload(pipeline, settings, device_target)

    if pipeline_has_meta_tensors(pipeline):
        if force_fallback:
            raise RuntimeError(
                "Image-Orb loaded pipeline with meta tensors even in fallback mode."
            )
        if not parse_bool(settings.get("allow_cpu_fallback"), False):
            raise RuntimeError(
                "Image-Orb loaded a meta-tensor pipeline and CPU fallback is disabled. "
                "Try dtype='float16' on MPS or enable allow_cpu_fallback explicitly."
            )
        print("Image-Orb: loaded meta-tensor pipeline; retrying with CPU fallback.", file=sys.stderr)
        return load_pipeline(model_family, force_fallback=True)

    STATE.loaded_pipeline = pipeline
    STATE.loaded_pipeline_family = model_family
    STATE.loaded_pipeline_ref = expected_ref_key
    STATE.loaded_pipeline_device = device_target
    STATE.pipeline_load_count += 1
    STATE.last_pipeline_loaded_utc = datetime.now(timezone.utc).isoformat()
    return STATE.loaded_pipeline


def clear_loaded_pipeline() -> None:
    pipeline = STATE.loaded_pipeline
    STATE.loaded_pipeline = None
    STATE.loaded_pipeline_family = None
    STATE.loaded_pipeline_ref = None
    STATE.loaded_pipeline_device = None
    if pipeline is None:
        release_torch_memory()
        return
    del pipeline
    release_torch_memory()


def ensure_diffusers_runtime() -> tuple[Any, dict[str, Any]]:
    if STATE.torch_module is not None and STATE.pipeline_classes:
        return STATE.torch_module, STATE.pipeline_classes

    try:
        with contextlib.redirect_stdout(sys.stderr):
            import torch as torch_module
    except ImportError as exc:  # pragma: no cover - optional until installed
        raise RuntimeError(
            "torch is not installed. Run ./scripts/install_portable.sh in the image-orb repo."
        ) from exc

    try:
        with contextlib.redirect_stdout(sys.stderr):
            from diffusers import (
                FluxPipeline,
                StableDiffusionPipeline,
                StableDiffusionXLPipeline,
            )
    except ImportError as exc:  # pragma: no cover - optional until installed
        raise RuntimeError(
            "diffusers is not installed or too old. Run ./scripts/install_portable.sh in image-orb."
        ) from exc

    STATE.torch_module = torch_module
    STATE.pipeline_classes = {
        "flux": FluxPipeline,
        "sdxl": StableDiffusionXLPipeline,
        "sd15": StableDiffusionPipeline,
    }
    return torch_module, STATE.pipeline_classes


def ensure_torch() -> Any:
    if STATE.torch_module is None:
        ensure_diffusers_runtime()
    return STATE.torch_module


def ensure_pillow_image_class() -> Any:
    if STATE.pillow_image_class is not None:
        return STATE.pillow_image_class

    try:
        with contextlib.redirect_stdout(sys.stderr):
            from PIL import Image as pillow_image_module
    except ImportError as exc:  # pragma: no cover - optional until installed
        raise RuntimeError(
            "Pillow is not installed. Run ./scripts/install_portable.sh in the image-orb repo."
        ) from exc

    STATE.pillow_image_class = pillow_image_module.Image
    return STATE.pillow_image_class


def pipeline_cache_ref_key(
    model_family: str,
    model_ref: str,
    flux_base_model_ref: str,
) -> str:
    if model_family == "flux" and is_flux_gguf_source(model_ref):
        base_ref = flux_base_model_ref or "black-forest-labs/FLUX.1-dev"
        return f"{model_ref}::base={base_ref}"
    return model_ref


def is_flux_gguf_source(raw: str) -> bool:
    return raw.strip().lower().endswith(".gguf")


def load_flux_pipeline_with_gguf(
    flux_pipeline_class: Any,
    gguf_model_source: str,
    settings: dict[str, Any],
    torch_module: Any,
    force_fallback: bool,
    cache_dir: Path,
) -> Any:
    try:
        with contextlib.redirect_stdout(sys.stderr):
            from diffusers import FluxTransformer2DModel
    except ImportError as exc:  # pragma: no cover - optional until installed
        raise RuntimeError(
            "Current diffusers build does not expose FluxTransformer2DModel. "
            "Upgrade diffusers to a GGUF-capable version."
        ) from exc

    ensure_gguf_runtime()
    gguf_quant_config_class = resolve_gguf_quantization_config_class()
    compute_dtype = resolve_gguf_compute_dtype(settings, torch_module, force_fallback)
    base_model_ref = str(settings.get("flux_base_model_ref") or "").strip()
    if not base_model_ref:
        base_model_ref = "black-forest-labs/FLUX.1-dev"
    base_model_source = resolve_model_source(base_model_ref)

    transformer_kwargs: dict[str, Any] = {
        "torch_dtype": compute_dtype,
        "quantization_config": gguf_quant_config_class(compute_dtype=compute_dtype),
        "cache_dir": str(cache_dir),
    }
    if parse_bool(settings.get("local_files_only"), False):
        transformer_kwargs["local_files_only"] = True

    print(
        (
            "Image-Orb: loading FLUX GGUF transformer from "
            f"{gguf_model_source} with base model {base_model_ref}"
        ),
        file=sys.stderr,
    )
    try:
        with contextlib.redirect_stdout(sys.stderr):
            transformer = FluxTransformer2DModel.from_single_file(
                gguf_model_source,
                **transformer_kwargs,
            )
    except Exception as exc:
        raise RuntimeError(
            "Failed to load FLUX GGUF transformer weights. "
            "Ensure this is a Diffusers-compatible FLUX transformer GGUF file "
            "(not a full checkpoint or incompatible conversion)."
        ) from exc

    pipeline_kwargs = build_pipeline_load_kwargs(
        settings, torch_module, force_fallback, cache_dir
    )
    pipeline_kwargs["transformer"] = transformer
    with contextlib.redirect_stdout(sys.stderr):
        return flux_pipeline_class.from_pretrained(base_model_source, **pipeline_kwargs)


def resolve_gguf_quantization_config_class() -> Any:
    try:
        with contextlib.redirect_stdout(sys.stderr):
            from diffusers import GGUFQuantizationConfig as config_class
        return config_class
    except ImportError:
        pass

    try:
        with contextlib.redirect_stdout(sys.stderr):
            from diffusers.quantizers import GGUFQuantizationConfig as config_class
        return config_class
    except ImportError as exc:  # pragma: no cover - optional until installed
        raise RuntimeError(
            "Current diffusers build does not expose GGUFQuantizationConfig. "
            "Install/upgrade diffusers with GGUF support."
        ) from exc


def ensure_gguf_runtime() -> None:
    try:
        with contextlib.redirect_stdout(sys.stderr):
            import gguf as gguf_module  # type: ignore
    except ImportError as exc:
        raise RuntimeError(
            "GGUF runtime is missing. Install Python package 'gguf>=0.10.0' "
            "and rerun ./scripts/install_portable.sh."
        ) from exc

    try:
        installed = importlib_metadata.version("gguf")
    except importlib_metadata.PackageNotFoundError:
        installed = getattr(gguf_module, "__version__", "")

    version_parts = []
    for token in str(installed).split("."):
        if token.isdigit():
            version_parts.append(int(token))
        else:
            break

    while len(version_parts) < 2:
        version_parts.append(0)

    if tuple(version_parts[:2]) < (0, 10):
        raise RuntimeError(
            f"Installed gguf version '{installed}' is too old. "
            "Image-Orb requires gguf>=0.10.0 for FLUX GGUF loading."
        )


def resolve_gguf_compute_dtype(
    settings: dict[str, Any], torch_module: Any, force_fallback: bool
) -> Any:
    if force_fallback:
        return torch_module.float32

    dtype_name = str(settings.get("flux_gguf_compute_dtype") or "auto").strip().lower()
    if dtype_name == "bfloat16":
        return torch_module.bfloat16
    if dtype_name == "float16":
        return torch_module.float16
    if dtype_name == "float32":
        return torch_module.float32

    # Auto: follow regular pipeline dtype heuristic.
    return resolve_torch_dtype(settings, torch_module)


def build_pipeline_load_kwargs(
    settings: dict[str, Any], torch_module: Any, force_fallback: bool, cache_dir: Path
) -> dict[str, Any]:
    kwargs: dict[str, Any] = {"cache_dir": str(cache_dir)}

    if force_fallback:
        kwargs["torch_dtype"] = torch_module.float32
        kwargs["low_cpu_mem_usage"] = False
        if parse_bool(settings.get("local_files_only"), False):
            kwargs["local_files_only"] = True
        return kwargs

    dtype = resolve_torch_dtype(settings, torch_module)
    kwargs["torch_dtype"] = dtype
    kwargs["low_cpu_mem_usage"] = False
    if parse_bool(settings.get("local_files_only"), False):
        kwargs["local_files_only"] = True
    return kwargs


def resolve_effective_device(settings: dict[str, Any], torch_module: Any) -> str:
    configured = str(settings.get("device") or "auto").strip().lower()
    if configured and configured != "auto":
        if configured == "cuda":
            return f"cuda:{resolve_cuda_device_index(settings)}"
        if configured.startswith("cuda:"):
            return configured
        return configured

    if torch_module.cuda.is_available():
        return "cuda:0"

    mps_backend = getattr(getattr(torch_module, "backends", None), "mps", None)
    if mps_backend is not None and mps_backend.is_available():
        return "mps"

    return "cpu"


def resolve_cuda_device_index(settings: dict[str, Any]) -> int:
    raw = settings.get("cuda_device_index", 0)
    if isinstance(raw, bool):
        return 0
    if isinstance(raw, int):
        return max(raw, 0)
    if isinstance(raw, float):
        return max(int(raw), 0)
    try:
        return max(int(str(raw).strip()), 0)
    except (TypeError, ValueError):
        return 0


def resolve_torch_dtype(settings: dict[str, Any], torch_module: Any) -> Any:
    dtype_name = str(settings.get("dtype") or "auto").strip().lower()
    if dtype_name == "bfloat16":
        return torch_module.bfloat16
    if dtype_name == "float16":
        return torch_module.float16
    if dtype_name == "float32":
        return torch_module.float32

    # Auto defaults: float16 on CUDA/MPS to reduce memory pressure; float32 on CPU.
    device = resolve_effective_device(settings, torch_module)
    if device.startswith("cuda") or device == "mps":
        return torch_module.float16
    return torch_module.float32


def apply_attention_mode(pipeline: Any, mode: str) -> None:
    if mode in {"", "auto", "none"}:
        return
    if mode == "xformers":
        if hasattr(pipeline, "enable_xformers_memory_efficient_attention"):
            pipeline.enable_xformers_memory_efficient_attention()
        return
    if mode == "slicing":
        if hasattr(pipeline, "enable_attention_slicing"):
            pipeline.enable_attention_slicing()
        return


def maybe_enable_cpu_offload(pipeline: Any, settings: dict[str, Any], device_target: str) -> None:
    if not parse_bool(settings.get("enable_model_cpu_offload"), False):
        return
    if not device_target.startswith("cuda"):
        return
    if hasattr(pipeline, "enable_model_cpu_offload"):
        pipeline.enable_model_cpu_offload()


def apply_memory_safe_limits(
    model_family: str, device_target: str, width: int, height: int, steps: int
) -> tuple[int, int, int, list[str]]:
    max_side, max_pixels, max_steps = memory_safe_limits(model_family, device_target)
    warnings: list[str] = []
    base = 16 if model_family == "flux" else 8

    safe_width = min(width, max_side)
    safe_height = min(height, max_side)
    safe_width = max(base, (safe_width // base) * base)
    safe_height = max(base, (safe_height // base) * base)

    while safe_width * safe_height > max_pixels:
        if safe_width >= safe_height and safe_width > base:
            safe_width = max(base, safe_width - base)
            continue
        if safe_height > base:
            safe_height = max(base, safe_height - base)
            continue
        break

    safe_steps = min(steps, max_steps)

    if safe_width != width or safe_height != height:
        warnings.append(
            "memory_safe_mode adjusted resolution "
            f"from {width}x{height} to {safe_width}x{safe_height} "
            f"for {model_family} on {device_target}."
        )
    if safe_steps != steps:
        warnings.append(
            "memory_safe_mode adjusted steps "
            f"from {steps} to {safe_steps} for {model_family} on {device_target}."
        )

    return safe_width, safe_height, safe_steps, warnings


def memory_safe_limits(model_family: str, device_target: str) -> tuple[int, int, int]:
    if model_family == "flux":
        if device_target.startswith("cuda"):
            return 1024, 1_048_576, 30
        return 768, 589_824, 20
    if model_family == "sdxl":
        if device_target.startswith("cuda"):
            return 1024, 1_048_576, 35
        return 768, 589_824, 24
    # sd15
    if device_target.startswith("cuda"):
        return 1024, 1_048_576, 40
    return 768, 589_824, 30


def peft_backend_available() -> bool:
    try:
        with contextlib.redirect_stdout(sys.stderr):
            from diffusers.utils.import_utils import is_peft_available
        return bool(is_peft_available())
    except Exception:
        return False


def is_missing_peft_backend_error(error: Exception) -> bool:
    text = str(error)
    if "PEFT backend is required" in text:
        return True
    lowered = text.lower()
    return "peft" in lowered and "required" in lowered


def lora_peft_install_hint() -> str:
    return (
        "LoRA stack requires the PEFT backend. Install package 'peft' in the Image-Orb venv "
        "(run ./scripts/install_portable.sh), or clear lora_stack_json to disable LoRAs."
    )


def parse_lora_stack(raw_value: Any) -> list[dict[str, Any]]:
    raw = str(raw_value or "").strip()
    if not raw:
        return []

    parsed = json.loads(raw)
    if not isinstance(parsed, list):
        raise ValueError("lora_stack_json must be a JSON array")

    entries: list[dict[str, Any]] = []
    for index, item in enumerate(parsed):
        if not isinstance(item, dict):
            raise ValueError(f"LoRA entry #{index} must be a JSON object")
        enabled = bool(item.get("enabled", True))
        source = str(item.get("source") or "").strip()
        if enabled and not source:
            raise ValueError(f"LoRA entry #{index} is enabled but missing 'source'")
        adapter_name = str(item.get("adapter_name") or f"lora_{index}").strip() or f"lora_{index}"
        weight_name = str(item.get("weight_name") or "").strip() or None
        scale = parse_float(item.get("scale"), 1.0)
        entries.append(
            {
                "enabled": enabled,
                "source": source,
                "adapter_name": adapter_name,
                "weight_name": weight_name,
                "scale": scale,
            }
        )
    return entries


def apply_lora_stack(pipeline: Any, settings: dict[str, Any]) -> None:
    entries = parse_lora_stack(settings.get("lora_stack_json"))
    active_entries = [entry for entry in entries if entry["enabled"]]
    if not active_entries:
        if hasattr(pipeline, "unload_lora_weights"):
            try:
                pipeline.unload_lora_weights()
            except Exception as exc:
                if is_missing_peft_backend_error(exc):
                    return
                raise
        return

    if not peft_backend_available():
        raise RuntimeError(lora_peft_install_hint())

    if hasattr(pipeline, "unload_lora_weights"):
        try:
            pipeline.unload_lora_weights()
        except Exception as exc:
            if is_missing_peft_backend_error(exc):
                raise RuntimeError(lora_peft_install_hint()) from exc
            raise

    adapter_names: list[str] = []
    adapter_scales: list[float] = []

    for entry in active_entries:
        if not hasattr(pipeline, "load_lora_weights"):
            raise RuntimeError("Loaded pipeline does not support LoRA loading")
        source = resolve_model_source(entry["source"])
        kwargs = {"adapter_name": entry["adapter_name"]}
        if entry["weight_name"]:
            kwargs["weight_name"] = entry["weight_name"]
        try:
            with contextlib.redirect_stdout(sys.stderr):
                pipeline.load_lora_weights(source, **kwargs)
        except Exception as exc:
            if is_missing_peft_backend_error(exc):
                raise RuntimeError(lora_peft_install_hint()) from exc
            raise
        adapter_names.append(entry["adapter_name"])
        adapter_scales.append(float(entry["scale"]))

    if hasattr(pipeline, "set_adapters"):
        try:
            pipeline.set_adapters(adapter_names, adapter_weights=adapter_scales)
        except Exception as exc:
            if is_missing_peft_backend_error(exc):
                raise RuntimeError(lora_peft_install_hint()) from exc
            raise
        return

    if len(adapter_names) == 1 and hasattr(pipeline, "fuse_lora"):
        try:
            pipeline.fuse_lora(lora_scale=adapter_scales[0])
        except Exception as exc:
            if is_missing_peft_backend_error(exc):
                raise RuntimeError(lora_peft_install_hint()) from exc
            raise
        return

    raise RuntimeError(
        "Pipeline does not support multi-adapter LoRA activation (missing set_adapters)"
    )


def build_positive_prompt(model_family: str, prompt: str) -> str:
    prompt = prompt.strip()
    if not prompt:
        return prompt
    if model_family == "flux":
        return prompt
    # SDXL/SD1.5 often benefit from explicit quality anchoring.
    return f"{prompt}, high quality, detailed"


def extract_result_image(result: Any) -> Any:
    images = getattr(result, "images", None)
    if not images:
        raise RuntimeError("Image pipeline returned no images")
    image = images[0]
    image_class = ensure_pillow_image_class()
    if not isinstance(image, image_class):
        raise RuntimeError("Image pipeline result is not a PIL image")
    return image


def pipeline_has_meta_tensors(pipeline: Any) -> bool:
    candidates = [pipeline]
    for attribute in ("transformer", "unet", "vae", "text_encoder", "text_encoder_2"):
        nested = getattr(pipeline, attribute, None)
        if nested is not None:
            candidates.append(nested)

    for candidate in candidates:
        parameters = getattr(candidate, "parameters", None)
        if not callable(parameters):
            continue
        try:
            for parameter in parameters():
                device = getattr(parameter, "device", None)
                if getattr(device, "type", None) == "meta":
                    return True
        except Exception:
            continue
    return False


def build_generator(torch_module: Any, device: str, seed: int) -> Any | None:
    generator_device = "cuda" if device.startswith("cuda") else "cpu"
    try:
        generator = torch_module.Generator(device=generator_device)
        generator.manual_seed(int(seed))
        return generator
    except Exception:
        return None


def run_pipeline_inference(pipeline: Any, invoke_kwargs: dict[str, Any]) -> Any:
    torch_module = STATE.torch_module
    inference_mode = getattr(torch_module, "inference_mode", None) if torch_module else None
    if callable(inference_mode):
        with inference_mode():
            return pipeline(**invoke_kwargs)
    return pipeline(**invoke_kwargs)


def normalize_model_family(raw: str) -> str:
    family = raw.strip().lower()
    if family not in SUPPORTED_MODEL_FAMILIES:
        raise ValueError(
            f"Unsupported model_family '{raw}'. Expected one of: flux, sdxl, sd15."
        )
    return family


def default_guidance_for_family(model_family: str) -> float:
    if model_family == "flux":
        return 3.5
    if model_family == "sd15":
        return 7.5
    return 7.0


def clamp_dimension(value: int, model_family: str) -> int:
    base = 16 if model_family == "flux" else 8
    value = max(256, min(2048, value))
    return max(base, (value // base) * base)


def parse_int(raw: Any, default: int) -> int:
    try:
        return int(raw)
    except Exception:
        return default


def parse_float(raw: Any, default: float) -> float:
    try:
        return float(raw)
    except Exception:
        return default


def parse_bool(raw: Any, default: bool = False) -> bool:
    if isinstance(raw, bool):
        return raw
    if raw is None:
        return default
    if isinstance(raw, (int, float)):
        return raw != 0
    text = str(raw).strip().lower()
    if text in {"true", "1", "yes", "y", "on"}:
        return True
    if text in {"false", "0", "no", "n", "off"}:
        return False
    return default


def current_rss_mb() -> float | None:
    try:
        output = subprocess.check_output(
            ["ps", "-o", "rss=", "-p", str(os.getpid())],
            text=True,
        ).strip()
        if not output:
            return None
        # `ps rss` reports KiB on macOS and Linux.
        return max(0.0, int(output) / 1024.0)
    except Exception:
        return None


def resolve_max_rss_mb(settings: dict[str, Any]) -> float:
    configured = parse_float(settings.get("max_rss_mb"), DEFAULT_MAX_RSS_MB)
    return max(2_000.0, min(configured, 131_072.0))


def bound_prompt_text(prompt: str, settings: dict[str, Any]) -> tuple[str, bool]:
    limit = parse_int(settings.get("max_prompt_chars"), DEFAULT_MAX_PROMPT_CHARS)
    safe_limit = max(120, min(limit, 16_000))
    if len(prompt) <= safe_limit:
        return prompt, False
    return prompt[:safe_limit].rstrip(), True


def release_torch_memory() -> None:
    torch_module = STATE.torch_module
    if torch_module is None:
        gc.collect()
        return
    try:
        if torch_module.cuda.is_available():
            torch_module.cuda.empty_cache()
            if hasattr(torch_module.cuda, "ipc_collect"):
                torch_module.cuda.ipc_collect()
    except Exception:
        pass
    try:
        mps_module = getattr(torch_module, "mps", None)
        if mps_module is not None and hasattr(mps_module, "empty_cache"):
            mps_module.empty_cache()
    except Exception:
        pass
    gc.collect()


def runtime_status_fields() -> dict[str, Any]:
    return {
        "pid": os.getpid(),
        "instance_id": STATE.instance_id,
        "plugin_started_at_utc": STATE.startup_utc,
        "pipeline_load_count": STATE.pipeline_load_count,
        "last_pipeline_loaded_at_utc": STATE.last_pipeline_loaded_utc,
        "rss_mb": current_rss_mb(),
    }


def resolve_model_source(model_ref: str) -> str:
    normalized = normalize_model_source_ref(model_ref)
    candidate, checked_paths = resolve_existing_model_source_path(normalized)
    if candidate is not None:
        return str(candidate)
    if looks_like_local_source_ref(normalized):
        checked_display = ", ".join(f"'{path}'" for path in checked_paths)
        if not checked_display:
            checked_display = f"'{normalized}'"
        raise ValueError(
            f"Model source '{model_ref}' looks like a local path, but it does not exist "
            f"(checked: {checked_display})."
        )
    return normalized


def ensure_single_file_config_dir(
    model_family: str,
    model_source_path: Path,
    settings: dict[str, Any],
    cache_dir: Path,
) -> Path | None:
    if not model_source_path.is_file():
        return None
    if model_family == "flux" and model_source_path.suffix.lower() == ".gguf":
        return None

    configured_root = str(settings.get("single_file_config_dir") or "").strip()
    base_dir = (
        resolve_repo_path(configured_root)
        if configured_root
        else (cache_dir / "diffusers-configs")
    )
    if (base_dir / "model_index.json").exists():
        target_dir = base_dir
    else:
        target_dir = base_dir / model_family
    model_index_path = target_dir / "model_index.json"
    if model_index_path.exists():
        return target_dir

    if parse_bool(settings.get("local_files_only"), False):
        raise RuntimeError(
            "Image-Orb is configured for local-only loading, but no local single-file pipeline "
            f"config exists at '{target_dir}'. Set local_files_only=false once to bootstrap, "
            "or place a Diffusers config directory there with model_index.json."
        )

    repo_id = resolve_single_file_config_repo(model_family, settings)
    bootstrap_single_file_config_dir(repo_id, target_dir)
    if not model_index_path.exists():
        raise RuntimeError(
            "Image-Orb attempted to bootstrap single-file pipeline config "
            f"from '{repo_id}', but '{model_index_path}' is still missing."
        )
    return target_dir


def resolve_single_file_config_repo(
    model_family: str,
    settings: dict[str, Any],
) -> str:
    configured = str(settings.get("single_file_config_repo") or "").strip()
    if configured:
        return configured
    return DEFAULT_SINGLE_FILE_CONFIG_REPO_BY_FAMILY.get(
        model_family, "stabilityai/stable-diffusion-xl-base-1.0"
    )


def bootstrap_single_file_config_dir(repo_id: str, target_dir: Path) -> None:
    ensure_directory(target_dir)
    try:
        with contextlib.redirect_stdout(sys.stderr):
            from huggingface_hub import snapshot_download
    except ImportError as exc:
        raise RuntimeError(
            "huggingface_hub is required to bootstrap local single-file configs. "
            "Run ./scripts/install_portable.sh in the image-orb repo."
        ) from exc

    print(
        (
            "Image-Orb: bootstrapping single-file config cache from "
            f"{repo_id} into {target_dir}"
        ),
        file=sys.stderr,
    )
    try:
        with contextlib.redirect_stdout(sys.stderr):
            snapshot_download(
                repo_id=repo_id,
                local_dir=str(target_dir),
                allow_patterns=SINGLE_FILE_CONFIG_ALLOW_PATTERNS,
            )
    except Exception as exc:
        raise RuntimeError(
            "Failed to bootstrap local single-file pipeline config from "
            f"'{repo_id}' into '{target_dir}'."
        ) from exc


def resolve_existing_model_source_path(raw_path: str) -> tuple[Path | None, list[str]]:
    expanded = os.path.expandvars(os.path.expanduser(raw_path or ""))
    if not expanded:
        return None, []

    candidate = Path(expanded)
    search_paths: list[Path] = []
    if candidate.is_absolute():
        search_paths.append(candidate)
    else:
        search_paths.append((Path.cwd() / candidate).resolve())
        search_paths.append((REPO_ROOT / candidate).resolve())

    checked: list[str] = []
    for path in search_paths:
        display = str(path)
        if display in checked:
            continue
        checked.append(display)
        if path.exists():
            return path, checked

    return None, checked


def resolve_repo_path(raw_path: str) -> Path:
    expanded = os.path.expandvars(os.path.expanduser(raw_path))
    candidate = Path(expanded)
    if candidate.is_absolute():
        return candidate
    return REPO_ROOT / candidate


def normalize_model_source_ref(raw: str) -> str:
    value = (raw or "").strip()
    if value.startswith("file://"):
        parsed = urlparse(value)
        return unquote(parsed.path)
    return value


def looks_like_local_source_ref(raw: str) -> bool:
    value = (raw or "").strip()
    if not value:
        return False
    if value.startswith(("/", "./", "../", "~", "file://")):
        return True
    if value.lower().endswith(LOCAL_MODEL_FILE_SUFFIXES):
        return True
    if "\\" in value:
        return True
    # Windows absolute path hint, e.g. C:\models\...
    if len(value) > 2 and value[1] == ":" and value[2] in ("\\", "/"):
        return True
    return False


def ensure_directory(path: Path) -> Path:
    path.mkdir(parents=True, exist_ok=True)
    return path


def normalize_save_format(raw: str) -> str:
    value = raw.strip().lower()
    if value in {"jpg", "jpeg"}:
        return "jpg"
    if value == "webp":
        return "webp"
    return "png"


def mime_type_for_format(save_format: str) -> str:
    if save_format == "jpg":
        return "image/jpeg"
    if save_format == "webp":
        return "image/webp"
    return "image/png"


if __name__ == "__main__":
    raise SystemExit(main())
