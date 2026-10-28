# Migration coverage

The Python project is preserved at https://github.com/DakodaStemen/lumen.
The Rust project has its own Git history, executable, app ID and profile. It does
not replace the old installation or migrate settings automatically.

| Capability | Rust preview |
| --- | --- |
| Local file playback, hardware decoding | Implemented; headless playback tests, one manual run on Wayland |
| Network URLs (HTTP, RTSP, RTMP, UDP) and the open-stream dialog | Implemented; HTTP HLS tested, RTSP, RTMP and UDP passed to mpv untested |
| Live streams through Streamlink relay, stream title | Implemented; tested with a local HLS stream, and with a real Twitch channel on request |
| Twitch ad filtering | Done by Streamlink itself; Lumen does not check that ads are removed |
| Stream quality picker, low-latency mode | Implemented; the quality switch is tested on a local stream, low latency only passes Streamlink's flags |
| YouTube and other sites through yt-dlp | Implemented; tested with a fake yt-dlp and with real YouTube on request |
| SponsorBlock | Skipping tested; the lookup ran against the real service for one video, which had no segments |
| Resume, per-file audio/subtitle choice and sync | Implemented; tested |
| Recent files | Implemented; welcome screen and menu |
| Continue with next file in the folder | Implemented; tested, natural order |
| Play queue, shuffle, repeat, drag and drop, M3U import | Implemented |
| Queue filter, Save queue (M3U), Clear, context menu | Implemented; tested |
| Queue reordering by drag | Pending |
| Speed 0.25×–4× | Implemented; pitch correction is mpv's default, not separately tested |
| Seek bar, frame stepping, chapters, A-B loop | Implemented; tested except the seek-bar mouse handling |
| Bookmarks | Add, list, jump, clear; tested |
| Chapters list and jump | Implemented; tested |
| Seek bar with hover time, chapter ticks and loop band | Implemented; hover and drag not tested with a real mouse |
| Seek-bar preview frames | Pending |
| Volume to 200%, mute, audio and subtitle sync | Implemented; tested |
| Audio and subtitle track switching | Implemented; tested |
| Load subtitle file, remembered per file | Implemented; the picker dialog itself is untested |
| Online subtitle search | Pending |
| Subtitle font, size, colour and outline | Pending |
| 10-band equalizer and presets | Pending |
| Audio output device | Pending |
| Aspect, crop, zoom, rotate, flip, deinterlace, colour controls | Pending |
| Screenshots | Implemented; not covered by a test |
| Rebindable keys | Preferences tab with capture and conflict refusal; tested |
| Preferences and keyboard shortcut list | Implemented; tested |
| Media information | Implemented; tested |
| Fullscreen with auto-hiding controls | Implemented; checked manually only |
| Mini player | Implemented; the open and close logic is tested, the window resize is not |
| Always on top | Implemented; not available on Wayland, where compositors decide |
| Media keys, MPRIS, desktop media controls | Pending |
| Native Wayland video | Implemented; checked on one session |
| Packaged distribution | Pending |
| Measured speed, memory or latency improvements | Not established |

Settings are not imported: track ids come from mpv and differ from libVLC's, so remembered
tracks would not match. Remembered positions could be imported later.
