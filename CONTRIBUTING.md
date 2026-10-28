# Contributing

Preserve the reference app and its settings. Implement changes in this repository
and test them with a separate profile (`XDG_CONFIG_HOME` points at a scratch directory).

Run formatting, Clippy and the tests before submitting changes:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Changes to rendering, input or fullscreen also need a run in the real window, on
Wayland and X11 if you can. Report the checks you ran and their limitations; do not
claim feature parity or performance improvements from language choice alone.

Keep mpv calls in `engine.rs`, player behaviour in `session.rs`, queue ordering in
`queue.rs`, persistence in `store.rs`, key bindings in `keys.rs`, GL rendering in
`video.rs`, and layout in `app.rs`. New behaviour belongs in `Session` with a test in
`tests/playback.rs`, not in the window code.
