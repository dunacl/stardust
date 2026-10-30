#!/usr/bin/env bash
#
# Builds release archives of the desktop app (sg-app), the command line
# (sg-cli) and the web app into dist/.
#
#   macos    Stardust.app and sg-cli, universal (Apple silicon + Intel),
#            ad-hoc signed. Needs a Mac and the x86_64-apple-darwin target
#            (`rustup target add x86_64-apple-darwin`).
#   linux    x86_64 and aarch64, built in Docker (Debian bookworm, so the
#            binaries run on glibc 2.36 or newer).
#   windows  x86_64, built in Docker with MinGW (x86_64-pc-windows-gnu).
#   web      the WebGPU + WebGL2 bundles of scripts/build-web.sh.
#
# Docker Desktop's default VM (a few GB of memory) cannot compile Bevy with
# a job per core, so Docker builds use DOCKER_JOBS (default 2) jobs; raise it
# after giving Docker more memory (Settings > Resources).
#
# Usage: scripts/release.sh [macos|linux|windows|web|all]...   (default: all)
#
# The archives are named stardust-<version>-<platform>; the version is the
# workspace version in Cargo.toml. Nothing is notarised: macOS users must
# right-click > Open the first time (or run `xattr -dr com.apple.quarantine`).

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
# shellcheck source=scripts/toolchain.sh
. scripts/toolchain.sh

VERSION="$(awk -F'"' '/^\[workspace.package\]/ { s = 1 } s && /^version/ { print $2; exit }' Cargo.toml)"
DIST=dist
STAGE=target/release-stage
# The Rust image tag follows the local toolchain, so every platform is built
# by the same compiler.
RUST_VERSION="$(rustc --version | awk '{ print $2 }')"
IMAGE="stardust-release:$RUST_VERSION"

mkdir -p "$DIST"

readme() {
  cat >"$1/README.txt" <<EOF
Stardust $VERSION (model $(model_version))
Planetary-system formation simulator.

  sg-app    the desktop app (sg-app --help for options)
  sg-cli    the command line: sg-cli simulate --seed 42

Licence and sources: see the project repository.
EOF
}

model_version() {
  awk -F'"' '/pub const MODEL_VERSION/ { print $2; exit }' crates/sg-sim/src/lib.rs
}

macos() {
  echo "== macos"
  local targets=(aarch64-apple-darwin x86_64-apple-darwin)
  for t in "${targets[@]}"; do
    cargo build -q --profile dist -p sg-app -p sg-cli --target "$t"
  done
  local name="stardust-$VERSION-macos-universal"
  local out="$STAGE/$name"
  local app="$out/Stardust.app"
  rm -rf "$out"
  mkdir -p "$app/Contents/MacOS"
  for bin in sg-app sg-cli; do
    lipo -create -output "$STAGE/$bin" \
      "target/aarch64-apple-darwin/dist/$bin" \
      "target/x86_64-apple-darwin/dist/$bin"
  done
  mv "$STAGE/sg-app" "$app/Contents/MacOS/sg-app"
  mv "$STAGE/sg-cli" "$out/sg-cli"
  cat >"$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Stardust</string>
  <key>CFBundleDisplayName</key><string>Stardust</string>
  <key>CFBundleIdentifier</key><string>cl.hint.stardust</string>
  <key>CFBundleExecutable</key><string>sg-app</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF
  # Ad-hoc signatures: required on Apple silicon, not a developer identity.
  codesign --force --sign - "$out/sg-cli"
  codesign --force --deep --sign - "$app"
  readme "$out"
  rm -f "$DIST/$name.zip"
  (cd "$STAGE" && /usr/bin/ditto -c -k --keepParent "$name" "../../$DIST/$name.zip")
}

# One image with the Linux libraries Bevy needs (udev for gamepads, X11 and
# Wayland through winit) and MinGW for Windows.
docker_image() {
  if ! docker info >/dev/null 2>&1; then
    echo "release: Docker is not running (start Docker Desktop)." >&2
    exit 1
  fi
  docker build -q -t "$IMAGE" --build-arg "RUST_VERSION=$RUST_VERSION" - >/dev/null <<'EOF'
ARG RUST_VERSION
FROM rust:${RUST_VERSION}-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends \
      pkg-config libudev-dev libwayland-dev libxkbcommon-dev \
      gcc-mingw-w64-x86-64 zip \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add x86_64-pc-windows-gnu
EOF
}

# Runs cargo in the image for one Docker platform. Each platform keeps its
# own target directory and registry cache so repeated releases are quick.
in_docker() {
  local platform="$1"
  shift
  local cache="target/docker-${platform//\//-}"
  mkdir -p "$cache/registry" "$cache/target"
  # RUSTUP_TOOLCHAIN: the image's toolchain, not rust-toolchain.toml's
  # channel, which rustup would otherwise download into the container.
  docker run --rm --platform "$platform" \
    -v "$PWD:/src:ro" \
    -v "$PWD/$cache/registry:/usr/local/cargo/registry" \
    -v "$PWD/$cache/target:/target" \
    -e CARGO_TARGET_DIR=/target \
    -e RUSTUP_TOOLCHAIN="$RUST_VERSION" \
    -e CARGO_BUILD_JOBS="${DOCKER_JOBS:-2}" \
    -e CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
    -w /src "$IMAGE" "$@"
}

linux() {
  docker_image
  local platform arch
  for platform in linux/amd64 linux/arm64; do
    arch="${platform#linux/}"
    [ "$arch" = amd64 ] && arch=x86_64
    [ "$arch" = arm64 ] && arch=aarch64
    echo "== linux $arch"
    in_docker "$platform" cargo build -q --profile dist --locked -p sg-app -p sg-cli
    local name="stardust-$VERSION-linux-$arch"
    local out="$STAGE/$name"
    rm -rf "$out"
    mkdir -p "$out"
    cp "target/docker-linux-${platform#linux/}/target/dist/sg-app" \
      "target/docker-linux-${platform#linux/}/target/dist/sg-cli" "$out/"
    readme "$out"
    tar -C "$STAGE" -czf "$DIST/$name.tar.gz" "$name"
  done
}

windows() {
  docker_image
  echo "== windows x86_64"
  # Cross-compiled from the native platform: no emulation needed.
  local platform
  platform="linux/$(docker version -f '{{.Server.Arch}}')"
  in_docker "$platform" cargo build -q --profile dist --locked -p sg-app -p sg-cli \
    --target x86_64-pc-windows-gnu
  local name="stardust-$VERSION-windows-x86_64"
  local out="$STAGE/$name"
  local built="target/docker-${platform//\//-}/target/x86_64-pc-windows-gnu/dist"
  rm -rf "$out"
  mkdir -p "$out"
  cp "$built/sg-app.exe" "$built/sg-cli.exe" "$out/"
  readme "$out"
  rm -f "$DIST/$name.zip"
  (cd "$STAGE" && /usr/bin/zip -qr "../../$DIST/$name.zip" "$name")
}

web() {
  echo "== web"
  scripts/build-web.sh
  local name="stardust-$VERSION-web"
  rm -f "$DIST/$name.zip"
  rm -rf "${STAGE:?}/$name"
  mkdir -p "$STAGE/$name"
  # Only the page and its two bundles: target/web is also a cargo profile
  # directory and can hold build files.
  cp -R target/web/index.html target/web/webgl2 target/web/webgpu "$STAGE/$name/"
  (cd "$STAGE" && /usr/bin/zip -qr "../../$DIST/$name.zip" "$name")
}

[ $# -eq 0 ] && set -- all
for what in "$@"; do
  case "$what" in
    macos | linux | windows | web) "$what" ;;
    # Linux and Windows are left out of `all` for now: their Docker builds
    # have not yet run to completion. Ask for them by name.
    all)
      macos
      web
      ;;
    *)
      echo "usage: scripts/release.sh [macos|linux|windows|web|all]..." >&2
      exit 2
      ;;
  esac
done

(cd "$DIST" && shasum -a 256 stardust-"$VERSION"-* | grep -v SHA256SUMS >"SHA256SUMS-$VERSION.txt")
echo
ls -lh "$DIST"
echo "release: wrote $DIST/ (checksums in $DIST/SHA256SUMS-$VERSION.txt)"
