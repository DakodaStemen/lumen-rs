# Changelog

## 0.1.0 — Native preview

- Native Rust desktop app using egui/eframe and libmpv, drawing video on Wayland and X11.
- Playback core: queue with shuffle, repeat, M3U import and folder continuation; resume;
  per-file audio and subtitle choices, sync offsets and bookmarks.
- Seek bar, frame stepping, chapters, A-B loop, speed, volume to 200%, track and sync controls.
- Rebindable keys, fullscreen, screenshots.
- Independent profile in `~/.config/lumen-rs`.

This is an early migration preview. See `docs/migration.md` for remaining parity work.
