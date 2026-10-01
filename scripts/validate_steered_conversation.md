# validate_steered_conversation.py

An opt-in real-GGUF end-to-end regression, using the actual backend routes and
agentic conversation executor used by the desktop. Supply the built desktop
binary, model, native engine device ID and a lab directory containing existing
checkpoint-matched contentment/excitement vectors. All inputs are read-only;
configuration, chats, handoffs and copied vectors live in a temporary directory.
No external tools, Telegram, ambient cognition or services run.

The default is Direct mode for interactive conversations, with normal gain one.
`--mode agentic` exercises richer task context and continuations; `--gain 4`
opts into the strongest amplification rather than silently changing the UI
default. Both modes support actual tool execution and handoff finalization.

```bash
python3 scripts/validate_steered_conversation.py --binary /absolute/ponderer --model /absolute/model.gguf --server /absolute/llama-server --device CUDA0 --vectors /absolute/affect_lab --report /tmp/new-conversation-report.json
```

Loads with all layers on one GPU, 200k context, unified Q4_1 K/V and flash
attention. Selects the provider for the session and carries one conversation
across neutral, contentment, excitement, combined, negative excitement and back
to neutral. Every reply must retain two earlier names and finish without hidden
autonomous repetition. The test also writes one real handoff to its isolated
database and resumes from it. Tool iterations are unlimited to exercise the
independent no-progress guard. Finally enables ambient cognition, sends another
foreground message and verifies recall/responsiveness with background work
allowed (its effectful tools are disabled). Closing the UI-parent pipe must terminate the
backend and every inference descendant.

The report retains raw visible replies, timings and acknowledged/applied
profiles, plus identical-message/temperature-zero/seed-42 probes under each
profile. Arithmetic and exact-JSON probes must pass at every setting, and at
least one fixed behavior probe must change. This proves operational conversation under intervention, not a
calibrated emotion scale, subjective experience or filled-200k performance.
Consecutive replies share a changing history and production sampling, so their
differences alone are not a controlled causal estimate; use the lab's paired
same-prompt/seed comparisons and independent confirmation for that purpose.
