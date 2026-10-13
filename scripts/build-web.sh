#!/usr/bin/env bash
#
# Builds the web app into target/web/ (PLAN.md §3, M2).
#
# Bevy cannot hold WebGPU and WebGL2 in one binary, so the app is built
# twice and web/index.html loads the one the browser supports.
#
# Needs the wasm32-unknown-unknown target and wasm-bindgen-cli at the
# version of wasm-bindgen in Cargo.lock:
#   cargo install wasm-bindgen-cli --version <version> --locked
# If wasm-opt (binaryen) is on PATH it is used to shrink the result.
#
# Usage: scripts/build-web.sh [webgl2|webgpu]   (default: both)

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
# shellcheck source=scripts/toolchain.sh
. scripts/toolchain.sh

OUT=target/web
WASM=target/wasm32-unknown-unknown/web/sg-app.wasm

want="$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/[^0-9.]/, ""); print; exit }' Cargo.lock)"
have="$(wasm-bindgen --version 2>/dev/null | awk '{ print $2 }' || true)"
if [ "$have" != "$want" ]; then
  echo "build-web: need wasm-bindgen-cli $want (found: ${have:-none})." >&2
  echo "  cargo install wasm-bindgen-cli --version $want --locked" >&2
  exit 1
fi

build() {
  local renderer="$1"
  shift
  echo "== $renderer"
  cargo build -q -p sg-app --target wasm32-unknown-unknown --profile web "$@"
  rm -rf "${OUT:?}/$renderer"
  wasm-bindgen --target web --no-typescript \
    --out-dir "$OUT/$renderer" --out-name stardust "$WASM"
  if command -v wasm-opt >/dev/null; then
    wasm-opt -Os -o "$OUT/$renderer/stardust_bg.wasm" "$OUT/$renderer/stardust_bg.wasm"
  fi
}

mkdir -p "$OUT"
case "${1:-all}" in
  webgl2) build webgl2 ;;
  webgpu) build webgpu --features webgpu ;;
  all)
    build webgl2
    build webgpu --features webgpu
    ;;
  *)
    echo "usage: scripts/build-web.sh [webgl2|webgpu]" >&2
    exit 2
    ;;
esac
cp web/index.html "$OUT/index.html"

echo
du -h "$OUT"/*/stardust_bg.wasm
echo "build-web: wrote $OUT; serve it with: make serve"
