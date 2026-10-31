# Feature status

Lumen has its own executable, app ID and profile (`~/.config/lumen-rs`).

| Capability | Status |
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
| Subtitle font, size, colour, outline width and position | Implemented in Preferences → Picture & sound; the mapping to mpv options and headless mpv acceptance are tested, how it looks on screen is not |
| 10-band equalizer and presets | Pending |
| Audio output device, night mode (loudnorm), ReplayGain | Implemented in Preferences → Picture & sound; options reach headless mpv in tests, the audible result is not tested |
| Aspect, crop, rotate | Implemented in the Video menu; crop needs mpv 0.41 or newer; set through mpv in tests, not checked visually |
| Brightness, contrast, saturation, gamma, hue | Implemented in Preferences → Picture & sound; tested against headless mpv |
| Zoom, flip, deinterlace | Pending |
| Hardware decoding picker, HDR tone-mapping choice | Implemented; the active decoder shows in Media information. HDR is shown tone-mapped to SDR, because video is rendered into an 8-bit RGBA framebuffer; there is no HDR output |
| Default quality profile (`gpu-hq`) | Set at start; skipped silently when the mpv build lacks it. Speed and quality cost are not measured |
| User shaders (`~/.config/lumen-rs/shaders`) | Menu lists `.glsl`, `.hook` and `.frag` files and can switch several on; none are bundled; shader output is not verified |
| Screenshots | Implemented; not covered by a test |
| Rebindable keys | Preferences tab with capture and conflict refusal; tested |
| Preferences and keyboard shortcut list | Implemented; tested |
| Media information | Implemented; tested |
| Fullscreen with auto-hiding controls | Implemented; checked manually only |
| Mini player | Implemented; the open and close logic is tested, the window resize is not |
| Always on top | Implemented; not available on Wayland, where compositors decide |
| Media keys, MPRIS, desktop media controls | Implemented through `souvlaki` on a worker thread: play, pause, toggle, stop, next, previous and the title and position are published. Mapping and update logic are tested; no run against a real desktop is recorded yet. Seek and volume from the desktop are not wired |
| Native Wayland video | Implemented; checked on one session |
| `.desktop` file and AppStream metainfo | Present in `packaging/linux` and checked with `desktop-file-validate` and `appstreamcli validate` |
| Flatpak | Draft manifest in `packaging/flatpak`, never built; it lacks vendored cargo sources and libmpv's dependency modules |
| Installable packages (deb, rpm, AppImage), release pipeline | Pending |
| Measured speed, memory or latency improvements | Not established |

Settings from other players are not imported, because track ids are assigned by mpv.
