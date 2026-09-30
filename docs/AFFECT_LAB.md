# GGUF Affect Lab

Affect Lab builds experimental directions from matched descriptions of the
assistant's own state, applies them during local inference, and records neutral
and steered completions. It uses llama.cpp and a small bundled extractor. Existing
API providers remain available; the local provider can be selected for one session.

## Desktop use

Rebuild and restart Ponderer. Model connection/loading lives in **Settings →
General → Model connection**; the toolbar's **Affect Lab** contains experiments.

1. Switch the model connection editor from **API** to **Local GGUF**. This changes
   the editor only, not the running provider. Select a single model GGUF or an
   LM Studio directory containing one model.
   Projector files beginning with `mmproj` are excluded from directory selection.
2. Choose CPU threads, GPU layers, context size, KV/cache options and the
   `llama-server` executable. Zero GPU layers uses CPU; nonzero layers also need
   a GPU-capable engine. **Use detected CUDA engine** offers the standalone CUDA
   executable if present in the operator's checkout. The 200k preset does not
   change executable/GPU layers. **Load local model** inspects metadata then queues
   actual weight/KV allocation, with visible progress and native process state.
   **Load weights / retry** can reload after a test.
3. Open **Example library**. Contentment, satisfaction, excitement, curiosity and
   fear have editable starter target/control pairs; custom affects start with two
   blank matched situations. **Create/Rebuild … control from these examples**
   derives a model-specific direction, not an emotion labeler. Built recipe texts
   are used only when their recipe fingerprint matches. Edits are UI drafts until
   built; polling cannot overwrite them. Every recipe is an unvalidated hypothesis.
4. In **Affect mixer**, combine built affects using sliders. Their total is bounded
   by one; at most eight nonzero directions are supported. After a 450 ms editing
   pause the mix is sent automatically, without an Apply-state step. Acknowledged
   and currently loaded mixes are distinct. The next request may reload the engine;
   avoid frequent changes during long tasks. Reset works after invalid layer edits.
5. In **Test & evidence**, compare the mix against neutral and half strength on five
   editable held-out prompts. Conditions use identical temperature zero, seed 42
   and token budget, with independent native caches between conditions. Outputs
   are paired by prompt and labeled identical/changed. Unchanged arithmetic and
   exact-JSON starter prompts get strict automatic checks; third-person leakage
   and intended affect/choice changes need human review. Truncation is visible.
   Save separate affect/quality assessments and notes into the fingerprinted JSON
   report. Tests preserve the agent's requested mix.
6. In Settings select **Use for this session** to route ordinary
   completions, reflection and streaming tool calls through the local provider.
   This selection cancels the current agent turn. It supports text inference.

Quiet background polling never disables controls or inserts/removes a spinner.
Older poll responses cannot overwrite a newer command, and acknowledgments cannot
discard subsequent slider edits. Failed mix updates require an edit or explicit
retry, rather than retrying forever. Session-provider changes preserve unrelated
unsaved Settings drafts; saving is blocked while a provider switch is in flight.

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

## Long-context memory settings

The **200k / Q4_1 preset** selects 200,000 tokens, unified KV, `q4_1` for both K and
V, and flash attention `on`. It leaves the executable and GPU layers unchanged.
The default remains a conservative 16,384-token, F16-cache configuration.

| UI setting | Native llama.cpp flags |
| --- | --- |
| Context size | `--ctx-size 200000` |
| Unified KV cache | `--kv-unified` (or explicit `--no-kv-unified`) |
| K / V cache | `--cache-type-k q4_1 --cache-type-v q4_1` |
| Flash attention | `--flash-attn on` (`auto` and `off` are also selectable) |

These flags are described in the [llama.cpp server documentation](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md).
Context accepts 1,024..1,048,576 tokens, but that UI limit is not a promise about a
model's training window, memory capacity or long-context quality. This Qwen GGUF
declares 262,144 tokens, so 200,000 requires no extra RoPE override. The engine may
round its allocation upward; the tested 200,000 request produced a 200,192-token
slot. Context includes the prompt and generated response, not just input tokens.

Quantized V cache requires flash attention; `off` plus a quantized V type is
rejected before loading. Quantization reduces cache storage but can affect quality.
Unified KV shares a buffer across sequences; it is not a separate compression
method. This host uses one inference slot and serializes requests. Disabling
context shift makes an overfull prompt fail rather than silently dropping history.

Managed loopback GGUF calls have a bounded one-hour request deadline instead of the
ordinary two minutes, and the proxy accepts request bodies up to 16 MiB. Existing
remote providers keep their deadlines. Closing the UI still kills the entire
process chain; longer deadlines do not create detached workers. The extraction
recipe continues to use its separate short 1,024-token context and default caches,
so changing inference KV settings does not silently change how vectors are built.

On this machine, `/usr/bin/llama-server` has no GPU devices. The GPU smoke test used
`/home/m/Code/llama.cpp-cuda/build/bin/llama-server`, GPU layers `999`, and
`CUDA_VISIBLE_DEVICES=0` to select the RTX 4090. To reproduce that device isolation,
launch Ponderer with that environment variable, choose the CUDA executable in the
lab, then apply the preset. No CUDA/LM Studio installation or service was changed.
Use a compatible engine with the required kernels; another build/device may reject
the same cache type or use slower fallback operations. The CUDA build's cache says
`GGML_CUDA_FA_ALL_QUANTS=OFF`, so full-window performance of its quantized attention
path still needs measurement even though the short exact-settings test succeeded.

Both CPU and RTX 4090 runs loaded the 200k configuration and answered the short
arithmetic prompt with `42` at neutral and contentment strength 0.25. This verifies
startup, allocation, short inference and steering compatibility. It does **not**
benchmark a filled 200k prompt, task quality, long-prefill speed or worst-case VRAM.
Reports record the selected engine and memory settings alongside the fingerprints.

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

The UI distinguishes geometry/file integrity, changed output, basic task integrity
and operator judgment of the intended construct. None amounts to calibration.
Repeated unseen tasks, individual controls before mixtures, multiple strengths and
layer ranges, shuffled/placebo interventions and blinded reviews remain necessary
before claiming useful affect specificity. The lab does not yet run placebo/shuffle
controls, confidence intervals or repeated-seed evaluations. Existing local reports
remain on disk, while the UI shows the last comparison of the current worker session.

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

A later RTX 4090 smoke test combined contentment 0.15 and satisfaction 0.10 on
layers 21–42, comparing neutral/half/full mixtures with 200k context, unified KV,
Q4_1 K/V and flash attention on. All six arithmetic/exact-JSON checks passed,
outputs ended normally and the requested default remained neutral. This verifies
multi-vector operation and two basic integrity tasks, not either intended affect.
The report is `affect_lab/comparisons/mix-1790803929668583843.json` (local artifact,
not committed). This short test allocates a long context but does not fill it.

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
long-context flag forwarding, invalid cache/attention combinations, large request
bodies, and the complete UI-parent-pipe shutdown chain. Mock protocol tests do not validate
an emotion construct; real-model reports support the narrower mechanics checks.
