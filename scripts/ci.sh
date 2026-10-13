#!/usr/bin/env bash
#
# The full local check suite. Hosted CI is not set up yet; when it is, it
# should run exactly this script.

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
# shellcheck source=scripts/toolchain.sh
. scripts/toolchain.sh

SIM_CRATES="sg-core sg-stellar sg-disk sg-formation sg-dynamics sg-planet sg-classic sg-schema sg-sim"

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

step "native / wasm32 determinism"
scripts/check-determinism.sh

step "Classic tier against the golden reference"
cargo run -q --release -p sg-cli -- compare-golden | tail -n 3

printf '\nci: all checks passed\n'
