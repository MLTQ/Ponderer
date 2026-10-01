# ui_snapshot.rs

Opt-in native visual QA harness. See [workbench](../src/ui/workbench.md) for CLI
arguments and isolation guarantees. Request an eframe viewport screenshot after
six frames, save the returned pixel buffer as PNG, then close. The timeout is
bounded; synthetic chat, approval and trace fixtures are visibly labeled.

Example with a CPU-rendered isolated X11 display:

```bash
cargo build --offline --features ui-snapshot --example ui_snapshot
env -u WAYLAND_DISPLAY WINIT_UNIX_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1 xvfb-run -a target/debug/examples/ui_snapshot /tmp/ponderer-violet.png appearance B79CDC dark 1240 900
```

This example is not compiled into the default app and does not attach to the
operator's live backend, load a GGUF or write agent configuration.
