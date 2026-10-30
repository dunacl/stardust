#!/usr/bin/env bash
#
# Enforces the dependency rules of PLAN.md §6.2:
#
#   1. Simulation crates never depend (even transitively) on Bevy, UI or
#      windowing crates. Only sg-dynamics may depend on wgpu.
#   2. Only sg-sim and the front ends that need raw Classic output (sg-cli)
#      depend on sg-accrete, and sg-accrete depends only on sg-core and sg-planet.
#   3. The workspace builds without the Classic tier.

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
# shellcheck source=scripts/toolchain.sh
. scripts/toolchain.sh

SIM_CRATES="sg-core sg-stellar sg-disk sg-formation sg-dynamics sg-planet sg-accrete sg-schema sg-sim"
FORBIDDEN='^(bevy.*|egui.*|eframe|winit|wgpu.*|naga|iced.*|sdl2|glutin|openxr.*)$'

metadata="$(cargo metadata --format-version 1 --locked 2>/dev/null || cargo metadata --format-version 1)"
fail=0

# Transitive normal dependencies of a workspace crate, one name per line.
closure() {
  jq -r --arg root "$1" '
    (.packages | map({key: .id, value: .name}) | from_entries) as $names
    | (.resolve.nodes | map({key: .id,
         value: [.deps[] | select(any(.dep_kinds[]; .kind == null)) | .pkg]}) | from_entries) as $deps
    | (.packages[] | select(.name == $root and .source == null) | .id) as $start
    | {seen: [], todo: [$start]}
    | until(.todo | length == 0;
        .todo[0] as $n
        | if (.seen | index($n)) then .todo |= .[1:]
          else .seen += [$n] | .todo = (.todo[1:] + ($deps[$n] // [])) end)
    | .seen[] | select(. != $start) | $names[.]' <<<"$metadata" | sort -u
}

for crate in $SIM_CRATES; do
  deps="$(closure "$crate")"
  bad="$(grep -E "$FORBIDDEN" <<<"$deps" || true)"
  if [ "$crate" = "sg-dynamics" ]; then
    bad="$(grep -Ev '^(wgpu.*|naga)$' <<<"$bad" || true)"
  fi
  if [ -n "$bad" ]; then
    echo "check-deps: $crate depends on forbidden crates: $(tr '\n' ' ' <<<"$bad")" >&2
    fail=1
  fi
done

# Direct workspace dependents of sg-accrete.
dependents="$(jq -r '.packages[] | select(.source == null)
  | select(any(.dependencies[]; .name == "sg-accrete" and .kind == null)) | .name' <<<"$metadata" | sort | tr '\n' ' ')"
if [ "$dependents" != "sg-sim " ]; then
  echo "check-deps: only sg-sim may depend on sg-accrete; found: $dependents" >&2
  fail=1
fi

classic_deps="$(jq -r '.packages[] | select(.name == "sg-accrete" and .source == null)
  | .dependencies[] | select(.kind == null and .path != null) | .name' <<<"$metadata" | tr '\n' ' ')"
if [ "$classic_deps" != "sg-core sg-planet " ]; then
  echo "check-deps: sg-accrete may depend only on sg-core and sg-planet among workspace crates; found: $classic_deps" >&2
  fail=1
fi

if cargo tree -p sg-sim --no-default-features -e normal 2>/dev/null | grep -q 'sg-accrete'; then
  echo "check-deps: sg-sim --no-default-features still pulls in sg-accrete" >&2
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "check-deps: dependency rules hold"
