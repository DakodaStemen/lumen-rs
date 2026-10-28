# Architecture

Playback logic is kept apart from the window so it can be tested against real mpv
without a display.

| Module | Responsibility |
| --- | --- |
| `engine.rs` | A typed wrapper over libmpv: load, seek, tracks, delays, chapters, A-B loop. No player policy. |
| `session.rs` | Lumen's behaviour: queue playback, resume, per-file memory, auto-advance, status line. |
| `queue.rs` | Queue order, shuffle, repeat, M3U import, natural-order folder continuation. |
| `store.rs` | Settings and per-file state as JSON, written atomically. |
| `keys.rs` | Actions, default keys and user overrides. |
| `clock.rs` | Time formatting and natural sort keys. |
| `video.rs` | Renders mpv's picture into an offscreen GL texture. |
| `app.rs` | The egui window, controls, queue panel and key handling. |

## Video path

libmpv's OpenGL render API draws into a framebuffer-backed texture inside an egui paint
callback. egui then draws that texture like any other image, so controls can overlay the
picture and nothing depends on a native child window. The texture is allocated once at
3840×2160; the picture is rendered into its top-left corner at window size, and larger
windows scale it up.

The render context borrows the player, so `Video` holds its own reference to the player
and is dropped, while eframe's GL context is still current, in `App::on_exit`.

## Timing

`Session::tick` runs every 200 ms, as in the Python version. It restores per-file choices
once mpv has published a file's tracks, saves the position every five seconds, and
advances the queue when a file ends. A-B looping is done by mpv itself.

## Streams

Streamlink and yt-dlp are not used yet. When they are, they will run as subprocesses, as
in the Python version, and mpv's own `ytdl` hook stays disabled so the two never compete.
