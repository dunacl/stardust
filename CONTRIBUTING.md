# Contributing

Issues and pull requests are welcome. By contributing you agree that your work
is licensed under the terms in [`NOTICE.md`](NOTICE.md).

## Before you open a pull request

Install Rust through [rustup](https://rustup.rs), then:

```sh
git submodule update --init            # only for scripts/golden.sh
git config core.hooksPath .githooks    # stops secrets before a push
cargo install --locked cargo-deny      # gitleaks is also needed
make ci                                # every check; run it before committing
```

## Rules of the code

1. Simulation crates (`sg-core`, `sg-stellar`, `sg-disk`, `sg-formation`,
   `sg-dynamics`, `sg-planet`, `sg-classic`, `sg-schema`, `sg-sim`) never depend
   on Bevy or UI crates, and must build for `wasm32-unknown-unknown`.
2. Same parameters, seed and model version give identical output on every
   platform: use `sg_core` RNG streams and `sg_core::math`, never `thread_rng`,
   the system clock, `f64::exp` and the like, or `HashMap` iteration.
3. Schema fields carry their units (`a_au`, `mass_mearth`), and numbers are
   always finite: use `Option` for what does not apply.
4. Every physics function cites its source and validity range, and has a test
   against an analytic or published value.
5. A change to outputs bumps `sg_sim::MODEL_VERSION` and the pinned hashes in
   `crates/sg-sim/tests/classic.rs`. For the Standard tier, add a new
   `validation/report-<model>.md`; never edit an old report, and never widen a
   tolerance to make a metric pass.
6. A schema change bumps `schema_version`.
7. Never copy StarGen's sources into this repository, and keep `sg-classic`
   separable: see [`NOTICE.md`](NOTICE.md).

Keep commits focused, with `cargo fmt` and `cargo clippy -- -D warnings` clean.
