# Architecture

Playback logic is kept apart from the window so it can be tested against real mpv
without a display.

| Module | Responsibility |
| --- | --- |
| `engine.rs` | A typed wrapper over libmpv: load, seek, tracks, delays, chapters, A-B loop. No player policy. |
| `session.rs` | Lumen's behaviour: queue playback, resume, per-file memory, auto-advance, status line. |
| `queue.rs` | Queue order, shuffle, repeat, M3U import, natural-order folder continuation. |
| `streams.rs` | Streamlink relay and yt-dlp resolution as subprocesses on worker threads. |
| `sponsorblock.rs` | SponsorBlock lookup by hash prefix, and segment parsing. |
| `store.rs` | Settings and per-file state as JSON, written atomically. |
| `keys.rs` | Actions, default keys and user overrides. |
| `clock.rs` | Time formatting and natural sort keys. |
| `video.rs` | Renders mpv's picture into an offscreen GL texture. |
| `ui/` | The interface. `mod.rs` lays out the window and handles input, `menus.rs` the header and context menus, `dialogs.rs` Preferences, media information and the link dialog, `bar.rs` the seek and volume bar, `theme.rs` Lumen's colours, `picker.rs` native file dialogs. |
| `app.rs` | The eframe glue: gives the interface a GPU video surface. |

## Video path

libmpv's OpenGL render API draws into a framebuffer-backed texture inside an egui paint
callback. egui then draws that texture like any other image, so controls can overlay the
picture and nothing depends on a native child window. The texture is allocated once at
3840×2160; the picture is rendered into its top-left corner at window size, and larger
windows scale it up.

The render context borrows the player, so `Video` holds its own reference to the player
and is dropped, while eframe's GL context is still current, in `App::on_exit`.

## Interface

The interface draws through a `Surface`, which is where the video goes. The window uses a GL
surface; tests use a black rectangle, so the whole interface runs in tests with no GPU and is
driven through accessible labels (every icon button has a name for screen readers).
File dialogs run on their own thread and report through a channel, which tests can feed directly.

Fullscreen and the mini player share a compact view: the picture fills the window and one
overlay holds the seek bar and transport row, hidden after two seconds without pointer movement.

## Timing

`Session::tick` runs every 200 ms. It restores per-file choices
once mpv has published a file's tracks, saves the position every five seconds, and
advances the queue when a file ends. A-B looping is done by mpv itself.

## Streams

Streamlink and yt-dlp run as subprocesses, never as libraries, and mpv's own `ytdl` hook is
disabled so the two never compete. Streamlink only strips Twitch's embedded ad segments when
it reads the stream itself, so it runs as a private HTTP server on 127.0.0.1 and mpv plays that.

`Streams::begin` starts a job on a worker thread. The job asks Streamlink whether a site plugin
matches (`--can-handle-url-no-redirect`, which uses no network), then runs the relay or yt-dlp.
It reports through a channel; `Session::tick` reads the events, and any from a job that has
since been replaced are dropped. Every process a job starts is registered with it, so
replacing or stopping the job kills them all.

A stream served through the relay is treated as live: mpv reports the length of its buffer,
but there is nothing to seek in and no position worth saving for resume.
