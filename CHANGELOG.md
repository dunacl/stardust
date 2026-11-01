# Changelog

Simulation changes are tracked by model version; each has a report in
[`validation/`](validation/).

## Unreleased

- Stars have layered convection cells, sunspots and faculae; hot stars stay smooth.
  The gas disk shows long trailing arcs, warm near the star, in place of radial
  streaks. The model's density and gaps are unchanged, and the patterns are drawn.

- *Export 3D scene (.glb)* saves the view for Blender and other 3D tools (`--export`
  from the command line, a download in a browser). Surfaces are baked into vertex
  colours. Belts, comet tails and the sky are left out. No change to the model.
- `--record <folder> --frames N --frame-rate F` plays the view with a fixed time step
  and saves numbered frames, off screen; `scripts/make-video.sh` turns them into a
  video with ffmpeg. Desktop only.

- Licensed under Apache-2.0.
- Hosted CI and automated security checks.
- Bevy 0.20.1, which fixes RUSTSEC-2026-0344 in `ruzstd`.
- Release archives for macOS and the web.
- Parameter files with an unknown or misplaced field are refused.
- Enlarged bodies, their rings and their moons no longer reach a neighbour's orbit:
  they are drawn smaller where there is no room. No change to the model.
- A menu bar (System, View, Playback, Settings, Help) holds the view and display
  options, and the control panel can be hidden. Phones keep everything in the drawer.
- A spaceship tour visits each planet and moon, for fun. It is not simulated.
- The growth-model milestone is closed at model 0.13.0, with its known limits stated
  in the README. No change to the model.

## Model 0.16.0

- Every body's card shows what it is made of, by mass, and any solid world an estimate
  of its surface minerals, labelled as one: the model follows no mineral, so the mix
  comes from its makeup and temperature by a few rules (`sg_planet::minerals`). Stars
  show hydrogen, helium and metals. The estimate tints bare rock. Schema 0.13.0. Nothing
  else changes: the validation report differs from 0.15.0 only in its timing.

## Model 0.15.0

- Comets, in the Standard and High tiers. How many a host has follows from its icy
  belts and planets, scaled from the Solar System: heavy planets on the way in eject
  most of them (Wyatt et al. 2017, MNRAS 464, 3385), and a planet of the right mass
  and age can build an Oort cloud. A few are drawn to show, with orbits from the
  Jupiter family's definition or, for long-period comets, from any direction. Schema
  0.12.0. Nothing else changes: comets come last, from their own random stream, and
  the validation report differs from 0.14.0 only in its timing.
- Comet counts are orders of magnitude. This model's outer belts are not cleared as
  the Kuiper belt was, so they run far above the Solar System's: the median system
  has about 120,000 against 300.

## Model 0.14.0

- Planets and moons have proper names, unique in their system, and keep their
  designation (`Calira b`) as a new field; belts are named. Schema 0.11.0. Nothing
  physical changes: the names are given last, from their own random stream.

## Model 0.13.0

- Belts grind themselves down in collisions over the system's age (Wyatt et al.
  2007, ApJ 658, 569), with that paper's canonical values. The limit comes to
  within a factor of two of the real asteroid belt without fitting. On 1000 High
  systems the median belt mass per star falls from 16 to 2.8 M⊕, stars above
  10 M⊕ from 66% to 13%, and belts ending inside 10 AU hold almost nothing.
  Planets and the population statistics are unchanged.

## Model 0.12.1

- The Hot Jupiter preset gives a giant inside 10 days in about one seed in six, and
  inside 100 days in two in three. It gave none in 400 seeds at 0.12.0. Other presets
  are unchanged.

## Model 0.12.0

- Standard and High: the median disk mass rises from 0.8% to 1.2% of the star's mass,
  and bumps trap 40% of the pebbles instead of 20%. All seven validation statistics
  pass, on seeds 1–3000 and 3001–6000. Both values are calibrated, not predicted.

## Model 0.11.0

- Standard and High: pressure bumps. A bump keeps part of the pebbles as a ring of
  dust and stops cores that have not opened a gap (Morbidelli 2020).
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
