#!/usr/bin/env bash
#
# Turns the frames of `sg-app --record <folder>` into a video. Needs ffmpeg;
# without it the numbered frames can be opened as an image sequence in
# Blender or most video editors.
#
# Usage: scripts/make-video.sh <folder> <output.mp4> [fps]   (default 30)

set -euo pipefail
folder="${1:?usage: scripts/make-video.sh <folder> <output.mp4> [fps]}"
output="${2:?usage: scripts/make-video.sh <folder> <output.mp4> [fps]}"
fps="${3:-30}"

if ! command -v ffmpeg >/dev/null; then
  echo "make-video: ffmpeg is not installed; the frames are in $folder." >&2
  exit 1
fi
# yuv420p for players that accept nothing else; the filter keeps sizes even,
# which that format needs.
ffmpeg -y -framerate "$fps" -i "$folder/frame-%05d.png" \
  -vf "pad=ceil(iw/2)*2:ceil(ih/2)*2" -c:v libx264 -pix_fmt yuv420p "$output"
echo "make-video: wrote $output"
