# Changelog

Simulation changes are tracked by model version; each has a report in
[`validation/`](validation/).

## Unreleased

- Licensed under Apache-2.0.
- Hosted CI and automated security checks.
- Bevy 0.20.1, which fixes RUSTSEC-2026-0344 in `ruzstd`.
- Release archives for macOS and the web.

## Model 0.10.0

- The Classic tier is rewritten from Dole (1969) and Fogg (1985) in `sg-accrete`,
  replacing the port of StarGen. **Every Classic seed gives a different system**,
  and Classic no longer matches StarGen. Its systems are lighter, and closer to
  Dole's published statistics.
- The Red Dwarf preset now needs the Standard tier.
- Standard and High give the same planets as model 0.9.0.

## Model 0.9.0

- Pebble supply follows the spreading gas; pebble accretion in every regime.
