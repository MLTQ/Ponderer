# Image-Orb Skill

Use `image_orb_generate` when the user asks for image creation or visual concept iteration.

## Guidance

- Prefer concise, concrete subject/style prompts.
- For `sdxl` / `sd15`, use `negative_prompt` when quality/artifact control matters.
- For `flux`, natural language scene descriptions usually work best; negative prompts are optional.
- If the user requests a specific model family look, pass `model_family` explicitly in tool arguments.

