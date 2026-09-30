# affect_lab.rs

An asynchronous egui window for selecting a local GGUF, building bootstrap or custom
matched-pair vectors, selecting bounded manual strengths, and inspecting controlled
comparisons. Status polling and commands use the authenticated backend API without
blocking the UI. The window shows requested and applied profiles separately.

Provider selection and stop return an updated AgentConfig to the application so
Settings and Character stay synchronized. Selection applies only to the current
backend session. Edits remain local drafts until Apply; reset sends a valid neutral
profile even if draft layer bounds are invalid.

The window is opened from AgentApp's controls. It does not launch processes directly
or represent experimental steering strengths as measured emotion or experience.
