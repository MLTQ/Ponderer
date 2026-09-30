# affect_lab.rs

Shared asynchronous model/experiment controller. Model connection controls render
inside Settings General, switching between API and local GGUF editors. The separate
lab window has Affect mixer, Example library and Test & evidence panes. Loading
chains worker initialization to a cancellable weight-allocation job, and session
provider selection is explicit. All processes remain owned by the desktop UI.

Quiet polls have separate state from action requests; epoch-tagged replies discard
polls predating mutations. Mix versions preserve slider edits made during requests.
Automatic mix updates debounce 450 ms, bound combined strength to one, and stop
retrying a failed version until explicitly retried or edited. No Apply button or
poll-triggered spinner. Headless tests assert identical mixer geometry during polls.

The example library exposes editable paired target/control texts, including verified
built recipe texts. Local drafts survive polling. The test pane compares complete
mixtures at neutral/half/full scales, pairs outputs by held-out prompt, reports
accuracy/format smoke checks and truncation, and saves separate operator affect/
quality judgments. It explicitly distinguishes interventions from calibration or
subjective experience; placebo and broader evaluation remain future work.
Inference/memory settings expose context up to 1,048,576 tokens, unified KV,
separate K/V types and flash attention. The 200k/Q4_1 preset changes these four
settings only; executable and GPU placement are explicitly chosen by the operator.

Provider selection and stop return an updated AgentConfig to the application so
Settings and Character stay synchronized. Provider-only Settings synchronization
preserves unrelated drafts; the app blocks saves during provider transitions.
Selection applies only to the current backend session. Reset restores valid layers
and neutral strengths even if draft layer bounds are invalid.

The window is opened from AgentApp's controls. It does not launch processes directly
or represent experimental steering strengths as measured emotion or experience.
