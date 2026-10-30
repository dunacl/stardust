#!/usr/bin/env bash
#
# The full check suite: run it before committing. Hosted CI runs exactly this
# script (.github/workflows/ci.yml), with the security checks as a job of
# their own (SKIP_SECURITY=1 here, .github/workflows/security.yml there).

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
# shellcheck source=scripts/toolchain.sh
. scripts/toolchain.sh

SIM_CRATES="sg-core sg-stellar sg-disk sg-formation sg-dynamics sg-planet sg-accrete sg-schema sg-sim"

step() { printf '\n== %s\n' "$*"; }

step "format"
cargo fmt --all -- --check

step "clippy"
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p sg-sim --no-default-features --all-targets -- -D warnings

step "tests"
cargo test --workspace -q

step "build and test without the Classic tier (PLAN.md D18)"
cargo test -q -p sg-sim --no-default-features

step "simulation crates build for wasm32-unknown-unknown"
# shellcheck disable=SC2086
cargo build -q --target wasm32-unknown-unknown $(printf -- '-p %s ' $SIM_CRATES) -p sg-wasm

step "the app compiles for the web, WebGL2 and WebGPU"
cargo check -q -p sg-app --target wasm32-unknown-unknown
cargo check -q -p sg-app --target wasm32-unknown-unknown --features webgpu

step "dependency rules"
scripts/check-deps.sh

step "no platform-dependent maths in simulation code"
scripts/check-math.sh

step "native / wasm32 determinism"
scripts/check-determinism.sh

if [ "${SKIP_SECURITY:-0}" != 1 ]; then
  step "security: leaks, advisories, licences, sources"
  scripts/check-security.sh
fi

printf '\nci: all checks passed\n'
