# Changelog

## 0.2.0 — Streams

- Live streams through Streamlink, run as a local relay, with the stream title, a quality
  picker, low-latency mode and a "live" display with no resume position.
- YouTube and other sites through yt-dlp, with separate audio streams attached, and a
  fall-back to the link itself when yt-dlp cannot read it.
- SponsorBlock segment skipping using the privacy-preserving hash-prefix lookup.
- "Open network stream" dialog (Ctrl+L).
- Lookups run on worker threads and are cancelled or ignored when the user plays something else.

## 0.1.0 — Native preview

- Native Rust desktop app using egui/eframe and libmpv, drawing video on Wayland and X11.
- Playback core: queue with shuffle, repeat, M3U import and folder continuation; resume;
  per-file audio and subtitle choices, sync offsets and bookmarks.
- Seek bar, frame stepping, chapters, A-B loop, speed, volume to 200%, track and sync controls.
- Rebindable keys, fullscreen, screenshots.
- Independent profile in `~/.config/lumen-rs`.

This is an early migration preview. See `docs/migration.md` for remaining parity work.
