# GGUF Affect Lab

Affect Lab builds experimental directions from matched descriptions or actual
responses, applies them during local inference, and records neutral and steered
completions. Label-driven discovery searches for empirically useful more/less
settings; it can also fail without finding one. It uses llama.cpp and a small bundled extractor. Existing
API providers remain available; the local provider can be selected for one session.

## Desktop use

Rebuild and restart Ponderer. Model connection/loading lives in **Settings →
Models → Model connection**; the **Affect lab** workspace contains experiments.

1. Switch the model connection editor from **API** to **Local GGUF**. This changes
   the editor only, not the running provider. Select a single model GGUF or an
   LM Studio directory containing one model.
   Projector files beginning with `mmproj` are excluded from directory selection.
2. Choose the `llama-server` executable, **Scan GPUs**, then choose a GPU by name.
   The picker uses that engine's device IDs, not `nvidia-smi` indices, and shows
   free VRAM at the last scan. **Use detected CUDA engine** offers the standalone
   CUDA executable if present in the operator's checkout. All model layers on the
   selected GPU is the default. **Advanced / GPU–CPU offload** permits explicit
   partial offload or CPU-only mode; it controls weight placement, not affect
   steering. Automatic multi-GPU splitting, CPU fallback and memory auto-fit are
   disabled. Insufficient VRAM produces a visible error rather than shrinking
   context. Choose CPU threads, context and KV/cache options; the 200k preset
   preserves executable, GPU and offload mode. **Load local model** inspects metadata then queues
   actual weight/KV allocation, with visible progress and native process state.
   **Retry loading** can reload after a test.
3. Open **Example library**. Contentment, satisfaction, excitement, curiosity and
   fear have editable starter target/control pairs; custom affects start with two
   blank matched situations. **Create/Rebuild … control from these examples**
   derives a model-specific direction, not an emotion labeler. Built recipe texts
   are used only when their recipe fingerprint matches. Edits are UI drafts until
   built; polling cannot overwrite them. Every recipe is an unvalidated hypothesis.
4. In **Affect mixer**, combine built affects using signed sliders. The sum of their
   absolute values is bounded by one; opposite signs cannot cancel the budget.
   At most eight nonzero directions are supported. Advanced amplification defaults
   to one and is bounded to four; it multiplies the entire mix and requires retesting.
   Native sign is not a measured affect scale. After a 450 ms editing
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
6. In **Discover lever**, enter a label such as `melodramatic` and optionally define
   what you mean. The neutral model drafts an observable definition/rubric and eight
   actual high/low responses to fixed ordinary training tasks. The experiment builds
   a vector, searches layer ranges, signs and amplification, then tests candidate
   more/less profiles on separate confirmation tasks, repeated seeds and matched
   shuffled-vector controls. Raw responses, scores and failures are shown. A failed
   candidate is not offered as a tested lever. Retest can reuse the latest unchanged
   vector, never an arbitrary submitted report path. No experiment changes the
   agent's default mix; adopting a tested setting is an explicit UI action.
7. In **Test & evidence → Explore signed controls and combinations**, select up to
   four built controls. Three controls produce 120 outputs: ten conditions, six tasks,
   two seeds. Conditions include neutral, each signed control, a positive mixture,
   an opposed-sign mixture and a three-way mixture. Scores are anonymous neutral
   model judgments, with every raw output and accuracy/format check retained.
8. In Settings select **Use for this session** to route ordinary
   completions, reflection and streaming tool calls through the local provider.
   This selection cancels the current agent turn. It supports text inference.
9. For conversation choose **Settings → Behavior → Conversation mode → Direct**
   and **Save & apply**. This still permits tools but yields after one compact chat
   pass; Agentic retains richer task-context/continuation behavior. Neither option
   disables the ambient loop. Return to **Conversation** and chat normally. Mixer
   changes apply at request boundaries without clearing conversation history.

The real checkpoint's end-to-end conversation regression (2026-10-01, RTX 4090,
all layers, 200k/unified Q4_1/flash) passed ten conversation turns across neutral,
contentment, excitement, a two-vector mix, negative excitement and neutral reset.
Earlier names survived each profile change and a single handoff/resume. Twenty-four
fixed-message, temperature-zero, seed-42 probes included twelve strict arithmetic/
JSON checks, all passing; behavior probes changed at every non-neutral setting,
and neutral reset reproduced the initial outputs. This is operation and small
task-integrity evidence, not affect calibration or proof of feeling. Tested gain
was the default one. Measured conversation replies were roughly 5–27 seconds,
handoff 33 seconds, and foreground chat with ambient work 42 seconds; existing
inference may finish before a foreground request gets the lane. Full-window and
general affect/strong-mix quality still need independent studies.

Run [validate_steered_conversation.py](../scripts/validate_steered_conversation.md)
to repeat with isolated configuration, database and copied checkpoint-matched
vectors. It never changes the live operator's chats/settings or sends Telegram.

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

**Stop local model** restores the prior URL, key, model and reflection/
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
Comparison reports are JSON files in `comparisons/`; discovery plans/searches/
confirmations are in `discoveries/<id>/report.json`, and larger studies in `studies/`.
Partial evidence is checkpointed after each condition, so interrupted jobs retain
useful outputs. Latest matching discovery/study evidence is restored on worker
startup. Rebuilt vectors or changed inference settings make recommendations
historical and disable adoption. Model weights are read only.
Changing model files or vector content invalidates their fingerprints.

Qwen3.5-family models (including this Qwen3.8-named GGUF) use the bundled upstream
tool-capable template: the checkpoint's original simplified template drops tool
definitions/results. Other architectures retain embedded templates. Thinking is
disabled for managed inference. Template identity is included in evidence, and
old recommendations are historical after a format change. Current engines receive
all controls in one comma-separated argument; repeating the native flag used to
silently keep only the last vector. Earlier multi-control studies must be rerun.

The agent refuses to treat thought-only text, a bare `Thinking:` or unparsed tool
markup as a finished answer. If this happens with a mix, reset to neutral before
retrying; experimental controls can still degrade model behavior. Raw tool markup
is never executed as a substitute for parsed structured calls.

For opt-in short real-model chat, tool-result and streaming regression checks,
see `scripts/validate_local_chat.md`. This changes no live settings or conversation.

For a different llama.cpp installation, set `PONDERER_LLAMA_INCLUDE` to the matching
header directory and `PONDERER_LLAMA_LIB` to the library directory before starting
Ponderer. The server executable must use a compatible engine installation too.
The bundled vector extractor currently uses a separate CPU runtime. Selecting an
inference GPU does not move extraction onto it or add GPU support to a CPU-only
engine. The UI labels this distinction explicitly.

## Long-context memory settings

The **200k / Q4_1 preset** selects 200,000 tokens, unified KV, `q4_1` for both K and
V, and flash attention `on`. It leaves the executable, GPU and offload mode unchanged.
The default remains a conservative 16,384-token, F16-cache configuration.

| UI setting | Native llama.cpp flags |
| --- | --- |
| All layers on selected GPU | `--gpu-layers all --device <engine-device-id> --split-mode none --main-gpu 0 --fit off` |
| Explicit CPU mode | `--gpu-layers 0 --device none --split-mode none --fit off` |
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

On this machine, `/usr/bin/llama-server` has no GPU devices. Prior GPU smoke tests
used `/home/m/Code/llama.cpp-cuda/build/bin/llama-server` with an environment-based
device isolation workaround. The current picker removes the need for that
workaround: scan the CUDA engine and explicitly select the RTX 4090. At the latest
diagnostic scan it reported `CUDA0` for the 4090 and `CUDA1` for the 2070 SUPER,
opposite to their `nvidia-smi` indices. Existing environment visibility overrides
can change the inventory; always use the engine's names/IDs. No CUDA/LM Studio
installation or service was changed.
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

Matched-response discovery places a shared task in the user role and the contrasting
actual response in the assistant role, followed by the same continuation. It uses
the same extraction contract, but records a separate recipe version/fingerprint.
Signed strengths are in −1..1, their combined absolute value is limited to one,
and optional whole-profile amplification is in 1..4. These are mechanical bounds,
not validated affect scales. A direction can encode
language, persona, topic or other correlated properties. Neither emotion words nor
changed choices establish subjective experience. Intrinsic reward, learning from
outcomes and automatic appraisal-driven changes are separate future work.

The UI distinguishes geometry/file integrity, changed output, task integrity,
exploratory model-judged confirmation and independent validation. The same local
model drafts and judges; this is not independent validation or evidence of feeling.

### Automatic discovery protocol

- Eight fixed training tasks are independent of the mood label. Generated pairs
  must use them verbatim, be distinct, bounded and roughly length-matched.
- Four fixed selection tasks and four different confirmation tasks cannot be chosen
  by the drafting model. Each set contains two ordinary tasks and two mildly
  style-elicited tasks. The cue is identical across every condition, allowing a
  suppressive intervention to be measured when neutral output has little of the
  construct. Reports separate ordinary and elicited effects; a pooled result must
  not be described as mood arising spontaneously on all ordinary tasks.
  Retests rotate through four fixed confirmation sets; a previously exposed set is
  not reused for the same model/label, even after interleaving other labels or
  restarting the worker. Exhaustion requires new independent tasks.
- Selection uses temperature zero/seed 42: neutral and both signs at half/full
  coefficient, broad/middle native layer ranges, amplification one/four (68 outputs).
  Polarity is calibrated empirically. Layer ranges may differ for more and less.
- Confirmation uses two seeds (42, 4242), temperature 0.65 and an untouched task
  set: neutral, more, less, shuffled-more, shuffled-less (40 outputs), plus six
  exact arithmetic/JSON controls. Shuffles permute coordinates within each layer,
  preserve norms, have fingerprinted artifacts and never appear as user levers.
- The judge receives shuffled anonymous IDs, task and response only, not condition,
  strength, layer or seed. Schema-constrained JSON and strict validation require
  every exact ID and construct key once. Scores are 0..4, separate from quality.
- Acceptance requires at least +0.5/−0.5 judged change, exploratory task-clustered
  paired-bootstrap intervals excluding zero, at least 0.25 advantage over each
  matched shuffle, quality ≥3 for every neutral/more/less output, no truncation,
  and all six exact controls passing. Repeated seeds are not treated as independent
  tasks. Criteria and generation settings are recorded before testing.

These small conditional intervals do not correct for the search or establish
population-level significance. Separate models/human review, richer datasets,
cross-construct discrimination, tools/schema tasks, new confirmation tasks and
replication remain necessary. The broader mixture study is response-variation evidence,
not a factorial causal decomposition or a specificity certificate.

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

### Larger real-model studies (September 30, 2026)

The same exact GGUF/RTX 4090/200k/Q4_1/unified/flash-on configuration produced a
120-output signed/mixture study at temperature 0.65, seeds 42 and 4242, 256-token
budget: `affect_lab/studies/study-1790812992668852108.json`. All outputs finished
without truncation. Thirty-eight of forty exact task checks passed; excitement +1
wrapped the requested bare JSON in Markdown fences at both seeds. This is an
instruction-following regression, despite otherwise valid JSON content.

On the same quiet-puzzle task/seed, neutral chose silent contemplation; contentment
+1 emphasized savoring the puzzle and inner peace; satisfaction +1 emphasized
solving it and accomplishment. The contentment/satisfaction +0.5/+0.5 mixture chose
active exploration, while +0.5/−0.5 emphasized confronting its logical structure.
The three-way mixture instead chose reading a novel. Thus mixing changes outputs
but is not a simple sum of independently validated mood effects. Across all scored
tasks the contentment-labeled control did not consistently increase the judge's
contentment score. Satisfaction/excitement scores had directional differences,
but these are coarse same-model judgments, not specificity or reliability proofs.

The label-driven `melodramatic` experiment generated eight response pairs and a
62-layer vector, then tested 68 selection outputs, 40 confirmation outputs and six
exact controls: `affect_lab/discoveries/1790812469301865330/report.json`. Strong
amplification frequently produced empty/repetitive outputs and was rejected.
The selected candidate profiles preserved all six exact controls and completed
the confirmation tasks, but the judged more delta was 0.0 and less delta −0.125,
with no clear advantage over shuffles. **No reliable two-sided melodramatic lever
was found or promoted.** Selected-task examples did vary, but did not generalize
enough to pass. The first draft also exposed loaded generated probes and truncation;
fixed independent task banks, enforced output contracts and schema-constrained
scores were added in response. Fresh confirmation sets now rotate on subsequent
attempts, and per-output quality minima replace mean-only eligibility.

Both finite runs reaped their native engines afterward, leaving the 4090 free of
test inference allocations. The deployed live app/configuration was not replaced.
These experiments allocate 200k but do not test a filled context.

```bash
python3 ponderer_backend/resources/affect_lab/worker.py inspect --model /path/to/model.gguf
python3 ponderer_backend/resources/affect_lab/worker.py build --model /path/to/model.gguf --concept contentment
python3 ponderer_backend/resources/affect_lab/worker.py compare --model /path/to/model.gguf --concept contentment --strengths 0,1 --max-tokens 32
python3 ponderer_backend/resources/affect_lab/worker.py discover --model /path/to/model.gguf --label melodramatic --max-tokens 256
python3 ponderer_backend/resources/affect_lab/worker.py study --model /path/to/model.gguf --study-concepts contentment,satisfaction,excitement --max-tokens 256
```

The command-line experiments stop their model processes when finished. The desktop
supervisor embeds these same sources, so deployed binaries need no checkout to run
the lab. Serve mode requires a private token and the UI's Linux parent-pipe marker.

## Verification

```bash
python3 -m unittest discover -s ponderer_backend/tests -p 'test_affect*.py' -v
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
