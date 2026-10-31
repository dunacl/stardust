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

# seed:mass for a single star, seed:preset for a preset, and
# seed:preset@standard or @high for a preset in the Standard or High tier.
# Together they cover stellar evolution, sampling, the multiple-star path, the
# modern formation model and the N-body integration (seeds 45, 274 and 412
# have giants that scatter).
CASES="1:1.0 42:1.0 7:0.3 1000:0.6 31337:0.8 99:1.2 12345:1.5 2026:1.0
  42:massive-a-star 42:circumbinary 42:s-type-binary 42:hierarchical-triple
  1:random 2:random 3:random 7:random 11:random 42:random 1000:random 31337:random
  1:sun-like@standard 42:sun-like@standard 7:red-dwarf@standard 42:hot-jupiter@standard
  42:metal-poor@standard 42:massive-a-star@standard 42:circumbinary@standard
  42:s-type-binary@standard 42:hierarchical-triple@standard
  1:random@standard 2:random@standard 3:random@standard 42:random@standard
  45:sun-like@high 274:sun-like@high 412:sun-like@high 42:hot-jupiter@high"

command -v node >/dev/null || { echo "check-determinism: node is required" >&2; exit 1; }

cargo build -q --release -p sg-cli
cargo build -q --release -p sg-wasm --target wasm32-unknown-unknown

native="$(for c in $CASES; do
  if [[ "${c##*:}" == [0-9]* ]]; then
    ./target/release/sg-cli hash --seed "${c%%:*}" --mass "${c##*:}"
  elif [[ "${c##*:}" == *@* ]]; then
    preset="${c##*:}"
    ./target/release/sg-cli hash --seed "${c%%:*}" --preset "${preset%@*}" --fidelity "${preset##*@}"
  else
    ./target/release/sg-cli hash --seed "${c%%:*}" --preset "${c##*:}"
  fi
done)"
# shellcheck disable=SC2086
wasm="$(node scripts/check-determinism.mjs \
  target/wasm32-unknown-unknown/release/sg_wasm.wasm $CASES)"

if [ "$native" != "$wasm" ]; then
  echo "check-determinism: native and wasm32 results differ" >&2
  paste <(tr -s ' \n' '\n' <<<"$CASES" | sed '/^$/d') <(echo "$native") <(echo "$wasm") >&2
  exit 1
fi
echo "check-determinism: native and wasm32 agree on $(wc -w <<<"$CASES" | tr -d ' ') runs"
paste <(tr -s ' \n' '\n' <<<"$CASES" | sed '/^$/d') <(echo "$native")
