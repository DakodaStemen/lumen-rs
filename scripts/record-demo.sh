#!/bin/sh
# Records docs/demo.gif: a scripted tour of the real player in a real window.
#
# Needs ffmpeg (and gifski for the best GIF) and a desktop session. The clips it makes have no audio stream and the player runs
# muted, so the recording is silent. The window is on screen for about half a minute.
set -eu
cd "$(dirname "$0")/.."

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
media="$work/media"
mkdir -p "$media"
ff() { ffmpeg -hide_banner -loglevel error -y "$@"; }
encode="-c:v libx264 -preset veryfast -crf 23 -pix_fmt yuv420p"

cat > "$work/chapters.txt" <<'CHAPTERS'
;FFMETADATA1
[CHAPTER]
TIMEBASE=1/1000
START=0
END=8000
title=Intro
[CHAPTER]
TIMEBASE=1/1000
START=8000
END=16000
title=Zoom
[CHAPTER]
TIMEBASE=1/1000
START=16000
END=26000
title=Deep
CHAPTERS

cat > "$work/en.srt" <<'SUBS'
1
00:00:00,500 --> 00:00:07,000
A zoom into the Mandelbrot set

2
00:00:08,000 --> 00:00:15,000
Every frame is generated, never filmed

3
00:00:16,500 --> 00:00:25,000
Lumen plays what mpv plays
SUBS

cat > "$work/es.srt" <<'SUBS'
1
00:00:00,500 --> 00:00:07,000
Un acercamiento al conjunto de Mandelbrot

2
00:00:08,000 --> 00:00:15,000
Cada cuadro se genera, nunca se filma

3
00:00:16,500 --> 00:00:25,000
Lumen reproduce lo que reproduce mpv
SUBS

# Silent clips: generated pictures, no audio stream.
# shellcheck disable=SC2086
ff -f lavfi -i "mandelbrot=size=1280x720:rate=30" -i "$work/en.srt" -i "$work/es.srt" -i "$work/chapters.txt" \
   -t 26 -filter:v "hue=h=230:s=0.45,eq=brightness=-0.12:contrast=1.15:saturation=1.1" -map 0:v -map 1 -map 2 -map_metadata 3 -map_chapters 3 $encode -c:s srt \
   -metadata:s:s:0 language=eng -metadata:s:s:0 title=English \
   -metadata:s:s:1 language=spa -metadata:s:s:1 title=Español \
   "$media/Into the Fractal.mkv"
# shellcheck disable=SC2086
ff -f lavfi -i "gradients=size=1280x720:rate=30:speed=0.02:nb_colors=4:c0=0x0b1026:c1=0x6a2c70:c2=0xf0703c:c3=0x101a3a:x0=0:y0=720:x1=1280:y1=0:type=linear" -t 24 $encode "$media/Coastline at Dusk.mp4"
# shellcheck disable=SC2086
ff -f lavfi -i "gradients=size=1280x720:rate=30:speed=0.025:nb_colors=4:c0=0x04121a:c1=0x0f6b73:c2=0xd7263d:c3=0x050a14:x0=0:y0=720:x1=1280:y1=0:type=linear" -t 24 $encode "$media/Night Drive.mp4"
# shellcheck disable=SC2086
ff -f lavfi -i "gradients=size=1280x720:rate=30:speed=0.015:nb_colors=4:c0=0x08150f:c1=0x2d6a4f:c2=0xe9c46a:c3=0x0b1f17:x0=0:y0=720:x1=1280:y1=0:type=linear" -t 24 $encode "$media/The Long Way Home.mp4"

cargo run --release --example demo -- docs/demo.gif "$media"
