import argparse
import os

from image_orb import server


def main() -> int:
    parser = argparse.ArgumentParser(description="Image-Orb bootstrap helper")
    parser.add_argument(
        "--prefetch-model",
        action="store_true",
        help="Load/cache the configured model pipeline locally.",
    )
    args = parser.parse_args()

    settings = {}
    if model_family := os.environ.get("IMAGE_ORB_MODEL_FAMILY", "").strip():
        settings["model_family"] = model_family
    if model_ref := os.environ.get("IMAGE_ORB_MODEL_REF", "").strip():
        settings["model_ref"] = model_ref
    if flux_base := os.environ.get("IMAGE_ORB_FLUX_BASE_MODEL_REF", "").strip():
        settings["flux_base_model_ref"] = flux_base
    if flux_gguf_dtype := os.environ.get("IMAGE_ORB_FLUX_GGUF_COMPUTE_DTYPE", "").strip():
        settings["flux_gguf_compute_dtype"] = flux_gguf_dtype
    if cache_dir := os.environ.get("IMAGE_ORB_CACHE_DIR", "").strip():
        settings["cache_dir"] = cache_dir
    if output_dir := os.environ.get("IMAGE_ORB_OUTPUT_DIR", "").strip():
        settings["output_dir"] = output_dir
    if device := os.environ.get("IMAGE_ORB_DEVICE", "").strip():
        settings["device"] = device
    if dtype := os.environ.get("IMAGE_ORB_DTYPE", "").strip():
        settings["dtype"] = dtype
    if attention := os.environ.get("IMAGE_ORB_ATTENTION_IMPL", "").strip():
        settings["attention_impl"] = attention
    if lora_stack := os.environ.get("IMAGE_ORB_LORA_STACK_JSON", "").strip():
        settings["lora_stack_json"] = lora_stack

    if settings:
        server.configure({"settings": settings})

    if args.prefetch_model:
        server.load_pipeline()

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
