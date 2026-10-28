```text
 _
| |   _   _ _ __ ___   ___ _ __
| |  | | | | '_ ` _ \ / _ \ '_ \
| |__| |_| | | | | | |  __/ | | |
|_____\__,_|_| |_| |_|\___|_| |_|   · Rust
```

**Every format. Every frame. In a better light.**

A native media player for Linux, built in Rust on libmpv.
Lumen uses egui/eframe for the interface and libmpv for playback, so video is drawn
natively on Wayland instead of through XWayland. It is a single Rust binary with no Qt,
Python or libVLC runtime.

This is an early release: playback and streams are covered, and the remaining features
are listed in [docs/migration.md](docs/migration.md). No performance claims are made.

## Status

| Area | State | Evidence |
| --- | --- | --- |
| Playback, queue, resume, streams | Working | Headless playback and stream tests |
| Quality defaults, hardware decoding, tone mapping | Working | Options reach headless mpv in tests; looks and speed not measured |
| Subtitle style, picture controls, night mode, ReplayGain, audio device | Working | Settings reach headless mpv in tests; not judged by eye or ear |
| Shader presets | Working, none bundled | Lists files you put in `~/.config/lumen-rs/shaders` |
| Media keys and MPRIS | New | Mapping logic tested; not yet run against a real desktop |
| Packaging | Draft | `.desktop` and AppStream validated; Flatpak manifest never built |
| Equalizer, online subtitles, preview frames | Not started | See [docs/migration.md](docs/migration.md) |

## How it fits together

```mermaid
flowchart LR
  UI["egui interface<br/>ui/, app.rs"] --> S["Session<br/>queue, resume, policy"]
  S --> E["Engine<br/>libmpv wrapper"]
  S --> T["Tuning<br/>settings to mpv options"]
  T --> E
  E --> V["Video<br/>GL texture"]
  V --> UI
  S --> ST["Streams<br/>Streamlink, yt-dlp"]
  UI <--> M["Media keys<br/>worker thread, MPRIS"]
```

## Feature matrix

| Feature | Lumen | Notes |
| --- | --- | --- |
| Local files and network URLs | Yes | Through libmpv |
| Hardware decoding | Yes | Picker: automatic, VA-API, NVDEC, off |
| HDR video | Tone-mapped to SDR | Video renders into an 8-bit RGBA framebuffer; there is no HDR output |
| User shaders | Yes | From `~/.config/lumen-rs/shaders`; none included |
| Subtitle styling | Font, size, colours, outline, position | Applies to text subtitles |
| Picture controls | Brightness, contrast, saturation, gamma, hue; aspect, rotate, crop | Crop needs mpv 0.41 or newer |
| Audio | Night mode, ReplayGain, output device | Night mode is a loudnorm filter |
| Twitch, Kick, YouTube | Yes | Needs Streamlink or yt-dlp installed |
| Media keys / MPRIS | Yes | Play, pause, stop, next, previous; seek and volume from the desktop are not wired |
| Equalizer, online subtitle search | No | Pending |
| Packages | No | Draft Flatpak and `.desktop` files in `packaging/` |

## Working today

- Playback of local files and URLs through libmpv, with hardware decoding when available.
- Play queue with shuffle, repeat, M3U import, drag and drop, and automatic
  continuation with the next file in the folder (`ep2` is followed by `ep10`).
- Resume where you left off, plus per-file memory of audio track, subtitle track,
  audio and subtitle sync, and bookmarks.
- Seek bar, frame stepping in both directions, chapters, A-B loop, speed from 0.25× to 4×,
  and volume up to 200%.
- Audio and subtitle track switching, and audio and subtitle delay in 50 ms steps.
- Screenshots, fullscreen with auto-hiding controls, and a status line.
- Lumen's own layout: a sidebar with **Open media**, **Network stream** and a filterable queue
  (right-click for play, copy location and remove; **Save queue** writes an M3U), a "Now playing"
  header with **Audio & subtitles**, **Video** and **…** menus, and a welcome screen with your
  recent files. The same menus open on right-click over the picture.
- A seek bar with a time (and chapter name) under the pointer, chapter ticks and the A-B loop
  shown on it; click the picture to pause, double-click for fullscreen, scroll for volume.
- Native file pickers for media and subtitle files, drag and drop with a drop hint, and
  spinners while a link is being opened.
- A mini player, and always on top where the desktop allows it.
- Preferences with every setting and a keyboard tab to rebind any key, with conflicts refused.
- Media information, chapters, bookmarks and recent files in the menus.
- Live streams from Twitch, Kick and other sites through [Streamlink](https://streamlink.github.io),
  run as a private relay on 127.0.0.1 so Streamlink's own ad handling applies, with a quality
  picker and the stream's title.
- YouTube and other video sites through [yt-dlp](https://github.com/yt-dlp/yt-dlp), including
  sites that serve video and audio separately.
- [SponsorBlock](https://sponsor.ajay.app): sponsor reads, self-promotion and subscribe
  reminders in YouTube videos are skipped (intros and outros optionally).
- An "Open network stream" dialog (Ctrl+L) for HTTP, HTTPS, RTSP, RTMP, UDP and RTP links.
- Quality defaults (`profile=gpu-hq`), a hardware-decoding picker and a tone-mapping choice, under
  **Preferences → Picture & sound**, together with subtitle style, picture controls, night mode,
  ReplayGain and the audio output device.
- Aspect, rotate and crop in the Video menu, and a Shaders menu for your own files in
  `~/.config/lumen-rs/shaders` (Lumen ships none).
- Media keys and MPRIS on a worker thread, so the desktop's play, pause, stop, next and previous work.
- Its own profile in `~/.config/lumen-rs`.

## Not yet

Online subtitle search, the equalizer, seek-bar preview frames, zoom, flip and deinterlace,
installable packages (a draft Flatpak manifest and `.desktop` files are in `packaging/`), and HDR
output: HDR is shown tone-mapped to SDR because the video framebuffer is 8-bit RGBA.
`docs/migration.md` tracks each item.

## Build and run

Linux is the current target. Install a stable Rust toolchain, a C linker, the usual
X11/Wayland and OpenGL runtime libraries, and mpv's library.

```sh
sudo pacman -S mpv            # Arch; Debian and Ubuntu: libmpv-dev
cargo run --release -- movie.mkv
```

Streams need two optional programs on the `PATH`. Without them Lumen still plays files and
direct links, but a Twitch or YouTube page will not open.

```sh
sudo pacman -S streamlink yt-dlp
```

The tests also need `ffmpeg` on the `PATH`; they generate their own media and are
skipped when it is missing. The Streamlink relay test is skipped without `streamlink`,
and tests against real Twitch and YouTube run only with `LUMEN_NETWORK_TESTS=1`.

```sh
cargo test
LUMEN_NETWORK_TESTS=1 cargo test --test streams   # optional, needs internet
```

## Keys

| Key | Action | Key | Action |
| --- | --- | --- | --- |
| Space | Play / pause | `[` `]` `=` | Slower / faster / normal speed |
| ← → | Seek 10 s | ↑ ↓ | Volume |
| Shift+← → | Seek 1 min | M | Mute |
| Home | Restart file | J K | Audio delay −/+ 50 ms |
| N P | Next / previous | G H | Subtitle delay −/+ 50 ms |
| `.` `,` | Next / previous frame | B V | Cycle audio / subtitle track |
| PgDn PgUp | Next / previous chapter | L | A-B loop: set A, set B, clear |
| S | Stop | Ctrl+B | Bookmark |
| F, Esc | Fullscreen, leave | Ctrl+Shift+S | Screenshot |
| Q | Show / hide queue | Delete | Remove queue item |
| Ctrl+L | Open network stream | Ctrl+O | Open media |
| Ctrl+M | Mini player | Ctrl+T | Always on top |
| Ctrl+, | Preferences | | |

To rebind a key, open **… → Keyboard shortcuts** (or Preferences, Ctrl+,), click a shortcut and
press the new keys. Escape cancels, and a key another action uses is refused. The choices are
stored under `keys` in `~/.config/lumen-rs/settings.json`.

## Streams and privacy

Lumen goes online only when you ask it to: to play a link, or for SponsorBlock on a
YouTube video. Which program handles a link:

- Plain media and playlist links (`.mp4`, `.m3u8`, …), RTSP, RTMP and UDP go straight to mpv.
- A link Streamlink has a site plugin for, such as Twitch or Kick, plays through Streamlink.
  If Streamlink fails for any reason except "no playable streams" (an offline channel), Lumen
  tries yt-dlp.
- Anything else, and all YouTube links, is resolved by yt-dlp, and played directly if yt-dlp
  cannot read it.

SponsorBlock lookups send only a four-character prefix of a hash of the video id, so the
service does not learn which video is playing. Segment data belongs to the SponsorBlock
contributors (CC BY-NC-SA 4.0).

Settings in `settings.json`: `stream_quality` (`"best"`, or a name such as `"720p60"`),
`low_latency` (Twitch and Kick), `sponsorblock`, and `sponsorblock_intros`.

## Project layout

See `docs/architecture.md`. Playback policy lives in `session.rs`, mpv calls in
`engine.rs`, and the window in `app.rs`.

## License

MIT. Playback uses mpv through its dynamic library; see `THIRD_PARTY_NOTICES.md`.
