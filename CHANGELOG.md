# Changelog

## Unreleased

- Quality defaults: mpv `profile=gpu-hq` for the window, a hardware-decoding picker and an HDR
  tone-mapping choice; the active decoder appears in Media information. HDR is shown tone-mapped to SDR
  (the video framebuffer is 8-bit RGBA).
- Subtitle font, size, colours, outline and position; brightness, contrast, saturation, gamma and hue;
  aspect, rotate and crop in the Video menu.
- Night mode (loudness normalisation), ReplayGain and an audio output device picker.
- A Shaders menu listing files from `~/.config/lumen-rs/shaders` (none are bundled).
- Media keys and MPRIS through a worker thread.
- A `.desktop` file, AppStream metainfo and a draft Flatpak manifest.
- README rewritten without the demo GIF (removed from the repository).

## 0.3.0 — A friendlier interface

- Lumen's layout: sidebar with Open media, Network stream and a filterable queue; "Now playing"
  header with Audio & subtitles, Video and … menus; framed video; transport row.
- Welcome screen with recent files, drag-and-drop hint, spinners, and messages shown over the picture.
- Seek bar with hover time and chapter name, chapter ticks and A-B loop band; click to pause,
  double-click for fullscreen, scroll for volume, right-click for the menus.
- Native file pickers, Save queue (M3U), Clear, queue context menu.
- Preferences, with a keyboard tab that rebinds any key and refuses conflicts.
- Mini player, always on top (not on Wayland), media information, chapter, bookmark and recent menus.
- Load subtitle files, which are remembered for the file.
- Escape closes the top-most dialog before it acts as a shortcut.
- A hint when yt-dlp and Streamlink are missing for a site link.
- Interface tests that click and type through the real interface without a window.
- A demo GIF recorded from the real player, and the recorder (`scripts/record-demo.sh`).
- The welcome text no longer shows through the drop hint while files hover over the window.

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
