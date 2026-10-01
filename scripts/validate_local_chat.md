# validate_local_chat.py

Opt-in real-GGUF regression for the local provider. Explicitly supply a model,
llama-server executable and engine GPU ID; never runs in the normal test suite.
It uses temporary state, neutral steering, all-layer single-GPU inference and the
200k/unified-Q4_1/flash configuration. Insufficient VRAM fails without fallback.

Checks a non-streamed reply, a structured inert fixture tool call, an answer
using its supplied result, and streamed chat with tool definitions through the
actual Python proxy. No tool executes. Unchanged requests must reuse the engine.
`finally` closes the proxy and reaps its entire owned native chain. Model files
are read-only; live settings, conversations, mixes and workloads are untouched.

```bash
python3 scripts/validate_local_chat.py --model /absolute/model.gguf --server /absolute/llama-server --device CUDA0
```

`--context` explicitly changes the test-only context (default 200000). This is a
short protocol regression, not a filled-context benchmark or affect calibration.
