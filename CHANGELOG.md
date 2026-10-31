# Changelog

Simulation changes are tracked by model version; each has a report in
[`validation/`](validation/).

## Unreleased

- Licensed under Apache-2.0.
- Hosted CI and automated security checks.
- Bevy 0.20.1, which fixes RUSTSEC-2026-0344 in `ruzstd`.
- Release archives for macOS and the web.
- Parameter files with an unknown or misplaced field are refused.
- Enlarged bodies, their rings and their moons no longer reach a neighbour's orbit:
  they are drawn smaller where there is no room. No change to the model.

## Model 0.12.0

- Standard and High: the median disk mass rises from 0.8% to 1.2% of the star's mass,
  and bumps trap 40% of the pebbles instead of 20% (ADR 0008). All seven validation
  statistics pass, on seeds 1–3000 and 3001–6000.

## Model 0.11.0

- Standard and High: pressure bumps. A bump keeps part of the pebbles as a ring of
  dust and stops cores that have not opened a gap (Morbidelli 2020, ADR 0007).
- Cores can gather gas below the pebble isolation mass once few solids reach them
  (Ikoma et al. 2000). The envelope cooling time is now 3×10⁸ years.
- Jupiter analogues rise from 0.07% to 1.43%, still under the 2% floor. Planets of
  1–4 Earth radii fall from 30% to 21%. A system takes about twice as long.
- Classic is unchanged, except for hashes that include the parameters.

## Model 0.10.0

- The Classic tier is rewritten from Dole (1969) and Fogg (1985) in `sg-accrete`,
  replacing the port of StarGen. **Every Classic seed gives a different system**,
  and Classic no longer matches StarGen. Its systems are lighter, and closer to
  Dole's published statistics.
- The Red Dwarf preset now needs the Standard tier.
- Standard and High give the same planets as model 0.9.0.

## Model 0.9.0

- Pebble supply follows the spreading gas; pebble accretion in every regime.
