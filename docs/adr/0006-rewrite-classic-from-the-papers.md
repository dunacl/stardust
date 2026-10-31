# ADR 0006: Rewrite the Classic tier from the published papers

Status: accepted, 2026-10-30, with all the answers to the specification's gaps.

## Context

`sg-classic` is a Rust port of StarGen, whose terms forbid selling it. The rest
of the project is Apache-2.0, but every app and command-line build includes the
port, so no build can be sold, and `NOTICE.md` has to carve out the port, the
golden outputs and the harness patch.

The physics itself is published. Copyright covers how the C code was written,
not the equations in Dole (1969/1970), Fogg (1985), Hart (1978) or Kothari
(1936).

## Decision

Replace `sg-classic` with a new crate, written from
[`docs/accretion-model-spec.md`](../accretion-model-spec.md), under Apache-2.0.
Then remove `sg-classic`, `reference/golden/`, `reference/harness/`, the
`stargen-c` submodule, `compare-golden`, `docs/classic-constants.md` and the
SplitMix64 exception to the determinism rule.

To keep the new code independent of the old:

- The specification cites a paper for each relation, and states what its author
  had seen of the old code.
- The implementer works in a worktree without `crates/sg-classic`,
  `reference/stargen-c` or `docs/classic-constants.md`.

## Consequences

- The tier no longer matches StarGen seed for seed. Existing Classic seeds and
  links give different systems, and the golden comparison goes with the goldens.
- Checks move to analytic values, the values printed in the papers, and Dole's
  population statistics over his 40-run sets.
- Tuning that exists only in StarGen's code is lost unless a publication
  supports it.
- One path can serve all stars, which ends the split of D31.
- The old port and StarGen's output were removed from the whole git history
  before the first public push, so they are in no build and not in this
  repository. Earlier commits no longer compile on their own.

## Alternatives considered

- **Keep the port and accept the limit.** No work, but no build can ever be sold.
- **Ask the earlier authors for permission.** Decades of authors; not practical.
- **Ship without the Classic tier.** Loses the historical model, which needs only
  a small crate to keep.
- **Rewrite from the port, changing names and structure.** Remains a derivative
  work, so it would not lift the limit.

## Limits

This is not a strict clean room. The owner maintains the fork, and earlier work
on this repository wrote the port. The independence record above reduces the risk
but does not remove it, and has not been reviewed by a lawyer. That review should
happen before anything is sold.
