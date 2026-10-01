# token_monitor.rs

The persistent rail heading is Novelty. Metric provenance remains explicit, with
plain text/colon separators rather than slash-separated duplicate labels. The
latest/retained-generation controls and metric calculations are unchanged.

## Purpose

Native novelty instrument in the persistent workbench rail. Each generation owns
an independent deterministic center-origin walk driven by token text, lexical
novelty and optional provider logprob/entropy. It is not a hidden-state embedding,
emotion meter or evidence of subjective experience.

## State and inspection

`generation_started`, `ingest_generation` and `generation_finished` track
independent paths. Paths retain samples until operator interaction by default,
or until manual Clear when selected. Time/recency fading never deletes samples.
`latest_generation` tracks the most recently updated path even when streams
interleave; the inspector can select any retained generation/sample instead.

`latest_readout` reports selected novelty, optional logprob/entropy, generation
source and explicit provider-probability versus lexical-proxy provenance. Missing
provider probabilities remain n/a, never fabricated.

## Rendering

All scope/wire/trail/marker colors derive from `theme::Palette`. A unit sphere
fills its scope via camera-distance compensation; this changes only projection,
not trace coordinates or metrics. Sphere comes before controls so it remains
visible in compact windows.

Drag orbits, scroll zooms, double-click resets; hover shows token/metric details.
The inspector highlights a retained sample without truncating the full trail.
“Orbit while generating” rotates only while a nonempty live trace exists, with
a five-second cooldown after manual interaction. Idle/finished traces stay still
once manual inertia settles. Camera motion never adds samples.

## Verification

Tests cover separate path origins, retention/clear, recency fading, interleaved
latest readouts, manual sample inspection, unavailable probability labeling and
live-only/disableable autorotation. API token samples retain `text`, `novelty`
and optional `logprob`/`entropy`; generation event/render methods remain stable.
