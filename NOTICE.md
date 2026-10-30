# Notice

Stardust
Copyright 2026 Jorge Valencia

Licensed under the Apache License, Version 2.0 ([`LICENSE`](LICENSE)), except
as stated below.

## What the Apache licence does not cover

These parts derive from StarGen, whose `Credits.txt` says it is "free for your
use as long as you don't sell it". That condition comes from its earlier
authors and cannot be replaced by this licence, so these parts are distributed under
StarGen's terms, not Apache-2.0:

- `crates/sg-classic/`, the Rust port of StarGen.
- `reference/golden/`, output of the StarGen program.
- `reference/harness/patches/`, a patch containing lines of StarGen's source.

The StarGen sources are not in this repository. `reference/stargen-c` is a git
submodule pointing at <https://github.com/dunacl/stargen>, under its own terms.

Do not sell any build that includes `sg-classic`. To build without it, disable
the `classic` feature of `sg-sim`; `sg-cli`, `sg-wasm` and `sg-app` enable it
and would need that changed too.

## Published data

The coefficients in `crates/sg-stellar/src/evolution/tables.rs` and the test
values in `crates/sg-stellar/tests/data/` come from the SSE package of Hurley,
Pols & Tout; see `reference/sse/README.md`.
