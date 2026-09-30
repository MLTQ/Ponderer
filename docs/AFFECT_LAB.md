# GGUF Affect Lab

Affect Lab builds experimental directions from matched descriptions of the
assistant's own state, applies them during local inference, and records neutral
and steered completions. It uses llama.cpp and a small bundled extractor. Existing
API providers remain available; the local provider can be selected for one session.

## Desktop use

Rebuild and restart Ponderer, then open **Affect Lab** beside Settings.

1. Select a single model GGUF or an LM Studio directory containing one model.
   Projector files beginning with `mmproj` are excluded from directory selection.
2. Choose CPU threads, GPU layers, context size and the `llama-server` executable.
   GPU layers zero uses CPU. The model is loaded when an experiment or completion
   needs it, rather than at provider startup.
3. Start the local provider. Choose contentment, satisfaction or excitement and
   build a matched-pair vector. Other constructs use the custom target/control
   recipe editor. Every recipe is an experimental hypothesis.
4. Run a neutral-versus-steered comparison before selecting a manual strength.
   Both conditions use the same prompt, seed, temperature and token budget. Reports
   include exact model/vector/recipe fingerprints, layer bounds and raw output.
5. Apply a manual state and select **Use for this session** to route ordinary
   completions, reflection and streaming tool calls through the local provider.
   This selection cancels the current agent turn. It supports text inference.

Requested and applied profiles are displayed separately. New settings take effect
at request boundaries. Changing a profile restarts native inference and discards
KV and recurrent caches; identical profiles can reuse the host. Requests and
experiments are serialized to avoid cross-request steering or loading two copies
of the model. Neutral/reset selects zero steering for subsequent requests.

**Stop / restore provider** restores the prior URL, key, model and reflection/
decision overrides. Ephemeral URLs and tokens are not saved into ordinary settings.
Closing the owning UI terminates the backend, worker, native supervisors and model
or extraction process groups. This feature currently requires Linux and the UI's
backend parent pipe. An externally launched backend cannot start it.

## Dependencies and artifacts

Use Python 3, a C++17 compiler, `llama-server`, and matching llama.cpp development
headers/shared libraries. Python uses only its standard library. The bundled
extractor compiles on its first build; a failed compile reports its log path.

Artifacts live under `affect_lab/` in Ponderer's working directory, or the directory
specified by `PONDERER_AFFECT_DATA_DIR`. They are excluded from Git. Each successful
build has a vector GGUF, manifest, recipe, target/control texts and extraction log.
Comparison reports are JSON files in `comparisons/`. Model weights are read only.
Changing model files or vector content invalidates their fingerprints.

For a different llama.cpp installation, set `PONDERER_LLAMA_INCLUDE` to the matching
header directory and `PONDERER_LLAMA_LIB` to the library directory before starting
Ponderer. The server executable must use a compatible engine installation too.
GPU extraction needs a library build with the relevant device support; selecting
GPU layers does not add GPU support to a CPU-only library.

## Extraction method and limits

The bootstrap dataset contains eight matched situations per construct. State
descriptions are placed in the assistant's own reflection, followed by the same
continuation in both conditions. The extractor reads the final prompt position,
averages target-minus-control activations across pairs and normalizes each layer.
It clears all KV/recurrent state between examples. Tensor `l_out-N` maps directly
to control direction `N`, preserving native layer coordinates.

Prediction/MTP layers, layer zero and the final language layer are excluded. Layer
zero has no native control-vector slot, while output graph aliasing can remove the
final layer's identifiable residual tensor. Every remaining direction must have
the expected dimensions, a finite unit norm and a unique layer index. Checkpoint
and vector SHA-256 are verified before applying steering. Split GGUFs and non-ChatML
extraction templates are currently unsupported and produce an explicit error.

Strengths are nonnegative and their combined value is limited to one. This is a
mechanical intervention bound, not a validated affect scale. A direction can encode
language, persona, topic or other correlated properties. Neither emotion words nor
changed choices establish subjective experience. Intrinsic reward, learning from
outcomes and automatic appraisal-driven changes are separate future work.

## Reproducing the local experiment

The tested `Qwen3.8-27B-OBLITERATED-Q4_K_M.gguf` identifies itself as `qwen35`, with
64 language layers, one additional prediction layer, and 5,120-dimensional states.
Its exact SHA-256 is
`1f74330b211a8253c96f1bf586cba6eb56d37117c97ed9e6eec18c198a4e7fe5`.

The installed stock generator (llama.cpp build 9247) aborts on its layer-count
assumption. The bundled extractor handles the prediction layer and exports 62
correctly indexed directions. The real model was tested on CPU with four threads.
The assistant-reflection contentment, satisfaction and excitement vectors are
available locally under `affect_lab/`. All three passed geometry checks; only the
contentment vector has the real-model comparison described below.

With identical deterministic generation settings, both strength zero and strength
one answered the arithmetic control with `42`. On the held-out next-action prompt,
the neutral output proposed reflecting on outcomes and lessons; the steered output
proposed reflecting on lessons and sharing them with others. Both ended normally.
This demonstrates a working intervention and changed output on this prompt, rather
than validating contentment or an intrinsic reward. The full raw comparison is kept
in the local artifact directory. Broader behavioral calibration remains tracked.

```bash
python3 ponderer_backend/resources/affect_lab/worker.py inspect --model /path/to/model.gguf
python3 ponderer_backend/resources/affect_lab/worker.py build --model /path/to/model.gguf --concept contentment
python3 ponderer_backend/resources/affect_lab/worker.py compare --model /path/to/model.gguf --concept contentment --strengths 0,1 --max-tokens 32
```

The command-line experiments stop their model processes when finished. The desktop
supervisor embeds these same sources, so deployed binaries need no checkout to run
the lab. Serve mode requires a private token and the UI's Linux parent-pipe marker.

## Verification

```bash
python3 -m unittest discover -s ponderer_backend/tests -p test_affect_lab.py -v
cargo test --manifest-path ponderer_backend/Cargo.toml
cargo test
cargo build --bin ponderer
python3 scripts/validate_affect_ui_lifetime.py /absolute/path/to/ponderer
```

Tests cover malformed profiles, vector geometry, exact fingerprints, request-local
overrides, profile/cache isolation, comparison restoration, authentication,
streaming, cancellation, native process-group death, durable config isolation,
and the complete UI-parent-pipe shutdown chain. Mock protocol tests do not validate
an emotion construct; real-model reports support the narrower mechanics checks.
