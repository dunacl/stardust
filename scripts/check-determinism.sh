#!/usr/bin/env bash
#
# Cross-target determinism check (PLAN.md §6.4, §10): the native build and
# the wasm32 build must produce identical result hashes.
#
# Needs the wasm32-unknown-unknown target and Node.js.

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
# shellcheck source=scripts/toolchain.sh
. scripts/toolchain.sh

CASES="1:1.0 42:1.0 7:0.3 1000:0.6 31337:0.8 99:1.2 12345:1.5 2026:1.0"

command -v node >/dev/null || { echo "check-determinism: node is required" >&2; exit 1; }

cargo build -q --release -p sg-cli
cargo build -q --release -p sg-wasm --target wasm32-unknown-unknown

native="$(for c in $CASES; do
  ./target/release/sg-cli hash --seed "${c%%:*}" --mass "${c##*:}"
done)"
# shellcheck disable=SC2086
wasm="$(node scripts/check-determinism.mjs \
  target/wasm32-unknown-unknown/release/sg_wasm.wasm $CASES)"

if [ "$native" != "$wasm" ]; then
  echo "check-determinism: native and wasm32 results differ" >&2
  paste <(tr ' ' '\n' <<<"$CASES") <(echo "$native") <(echo "$wasm") >&2
  exit 1
fi
echo "check-determinism: native and wasm32 agree on $(wc -w <<<"$CASES" | tr -d ' ') runs"
paste <(tr ' ' '\n' <<<"$CASES") <(echo "$native")
