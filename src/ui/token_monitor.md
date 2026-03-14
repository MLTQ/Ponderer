# token_monitor.rs

## Purpose
Renders the live token-status monitor in the Mind sidebar. It turns streamed token novelty samples into a deterministic 3D random-walk trace inside a slowly rotating wireframe sphere.

## Components

### `TokenMonitorState`
- **Does**: Stores the active conversation trace, current 3D position/direction, sample counter, and latest novelty value for status text.
- **Interacts with**: `ui/app.rs` event handling for `FrontendEvent::TokenMetrics`.

### `TokenMonitorState::ingest`
- **Does**: Resets the trace when a new stream starts and appends new token samples to the rolling path state.
- **Interacts with**: backend token metric batches decoded in `api.rs`.

### `render(ui, state)`
- **Does**: Draws the pure-black backdrop, pale-green wireframe sphere, center marker, and the colored token trail. While hovered, mouse-wheel scroll adjusts zoom.
- **Interacts with**: egui painter API and `TokenMonitorState`.

### Trace helpers (`push_sample`, `hashed_direction`, `rotate`, `project`)
- **Does**: Convert novelty/logprob/entropy into 3D movement and screen-space projection.
- **Interacts with**: internal `Vec3` math and egui drawing primitives.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `app.rs` | `TokenMonitorState::new`, `ingest`, `last_novelty`, `trace_len`, and `render` remain available | Renaming state/render entry points |
| `api.rs` | `TokenMetricSample` continues to expose `text`, optional `logprob`/`entropy`, and `novelty` | Changing sample field names or semantics |

## Notes
- The walk is deterministic per token text + sample index, so similar replies create similar knot shapes.
- A mild center pull keeps boring runs near the origin while still letting higher-novelty segments escape beyond the unit sphere.
- The sphere is decorative but data-driven: color shifts from green toward red as the trail moves farther from the center.
- Zoom is local UI state on `TokenMonitorState`, so hovering and scrolling changes scale without disturbing the underlying trace.
