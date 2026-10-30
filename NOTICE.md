# Licence notice

Copyright (C) 2026 Jorge Valencia

Stardust is free software: you can redistribute it and/or modify it under the
terms of the GNU General Public License as published by the Free Software
Foundation, version 3 ([`LICENSE`](LICENSE)). It comes with no warranty.

## What the GPL does not cover

These parts derive from StarGen, whose `Credits.txt` says it is "free for your
use as long as you don't sell it". That condition comes from its earlier
authors and cannot be replaced by the GPL here, so these parts are distributed
under StarGen's terms, not the GPL:

- `crates/sg-classic/`, the Rust port of StarGen.
- `reference/golden/`, output of the StarGen program.
- `reference/harness/patches/`, a patch containing lines of StarGen's source.

The StarGen sources are not in this repository. `reference/stargen-c` is a git
submodule pointing at <https://github.com/dunacl/stargen>, under its own terms.

Do not sell any build that includes `sg-classic`. To build without it, disable
the `classic` feature of `sg-sim`; `sg-cli`, `sg-wasm` and `sg-app` enable it
and would need that changed too.

## Additional permission under GPL version 3, section 7

If you modify this program, or any covered work, by linking or combining it
with `sg-classic`, the copyright holder of the GPL-covered parts grants you
permission to convey the resulting work. This does not change the terms of
`sg-classic` itself, and you may remove this permission from your own versions.

## Published data

The coefficients in `crates/sg-stellar/src/evolution/tables.rs` and the test
values in `crates/sg-stellar/tests/data/` come from the SSE package of Hurley,
Pols & Tout; see `reference/sse/README.md`.
