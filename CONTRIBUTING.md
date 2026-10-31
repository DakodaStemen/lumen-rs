# Contributing

Preserve the reference app and its settings. Implement changes in this repository
and test them with a separate profile (`XDG_CONFIG_HOME` points at a scratch directory).

Run formatting, Clippy and the tests before submitting changes:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The UI tests and the demo recorder (`scripts/record-demo.sh`, which writes a clip for your own
checking and is no longer part of the README) aim at widgets by name, and a test keeps those
names reporting their positions.

Changes to rendering, input or fullscreen also need a run in the real window, on
Wayland and X11 if you can. Report the checks you ran and their limitations; do not
claim feature parity or performance improvements from language choice alone.

## Where code goes

| Concern | File |
| --- | --- |
| mpv calls | `src/engine.rs` |
| Player behaviour | `src/session.rs` |
| Queue ordering | `src/queue.rs` |
| Persistence | `src/store.rs` |
| Key bindings | `src/keys.rs` |
| GL rendering | `src/video.rs` |
| Layout | `src/app.rs`, `src/ui/` |

New behaviour belongs in `Session` with a test in
`tests/playback.rs`, not in the window code.
