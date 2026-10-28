# Migration coverage

The Python project is preserved at https://github.com/DakodaStemen/lumen.
The Rust project has its own Git history, executable, app ID and profile. It does
not replace the old installation or migrate settings automatically.

| Capability | Rust preview |
| --- | --- |
| Local file playback, hardware decoding | Implemented; headless playback tests, one manual run on Wayland |
| Network URLs (HTTP, RTSP, RTMP, UDP) | Passed to mpv; not tested |
| Live streams through Streamlink, Twitch ad filtering, quality picker | Pending |
| YouTube and other sites through yt-dlp | Pending |
| SponsorBlock | Pending |
| Resume, per-file audio/subtitle choice and sync | Implemented; tested |
| Recent files | Recorded; no menu yet |
| Continue with next file in the folder | Implemented; tested, natural order |
| Play queue, shuffle, repeat, drag and drop, M3U import | Implemented |
| Queue filter, reordering, M3U export | Pending |
| Speed 0.25×–4× | Implemented; pitch correction is mpv's default, not separately tested |
| Seek bar, frame stepping, chapters, A-B loop | Implemented; tested except the seek-bar mouse handling |
| Bookmarks | Added and stored; no list or jump UI yet |
| Seek-bar preview frames | Pending |
| Volume to 200%, mute, audio and subtitle sync | Implemented; tested |
| Audio and subtitle track switching | Implemented; tested |
| Remembered external subtitles | Re-attached when present; no UI to add them yet |
| Online subtitle search | Pending |
| Subtitle font, size, colour and outline | Pending |
| 10-band equalizer and presets | Pending |
| Audio output device | Pending |
| Aspect, crop, zoom, rotate, flip, deinterlace, colour controls | Pending |
| Screenshots | Implemented; not covered by a test |
| Rebindable keys | Read from the settings file; no editor UI |
| Fullscreen | Implemented; checked manually only |
| Mini player, always on top | Pending |
| Media keys, MPRIS, desktop media controls | Pending |
| Native Wayland video | Implemented; checked on one session |
| Packaged distribution | Pending |
| Measured speed, memory or latency improvements | Not established |

Settings are not imported: track ids come from mpv and differ from libVLC's, so remembered
tracks would not match. Remembered positions could be imported later.
