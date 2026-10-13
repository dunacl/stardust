#!/usr/bin/env bash
#
# Determinism rule (CLAUDE.md hard rule 2): simulation code calls
# transcendental functions through sg_core::math (libm), never the methods
# on f64, whose results differ between platforms. Test modules are exempt.
#
# A text search, so it can be fooled; scripts/check-determinism.sh is the
# real test. This one says where to look.

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

SIM_CRATES="sg-core sg-stellar sg-disk sg-formation sg-dynamics sg-planet sg-classic sg-schema sg-sim"
METHODS='exp|ln|powf|powi|log10|log2|log|sin|cos|tan|cbrt|exp_m1|ln_1p|atan2|asin|acos|atan|sinh|cosh|tanh|hypot|exp2'

found=0
for crate in $SIM_CRATES; do
  while IFS= read -r file; do
    # Everything before the first `#[cfg(test)]`, minus comment lines.
    hits="$(awk -v methods="$METHODS" '
      /^#\[cfg\(test\)\]/ { exit }
      /^[ \t]*\/\// { next }
      $0 ~ "\\.(" methods ")\\(" { printf "%d: %s\n", NR, $0 }
    ' "$file")"
    if [ -n "$hits" ]; then
      echo "$file" >&2
      echo "$hits" | sed 's/^/  /' >&2
      found=1
    fi
  done < <(find "crates/$crate/src" -name '*.rs' | sort)
done

if [ "$found" -ne 0 ]; then
  echo "check-math: use sg_core::math instead of f64 methods in simulation code" >&2
  exit 1
fi
echo "check-math: no platform-dependent maths in simulation code"
