# Stardust

A simulator of how planetary systems form: pick a kind of star, press
**Simulate**, watch planets grow in a disk of gas and dust, then explore the
finished system. It runs as a desktop app and in a browser.

Stardust was inspired by the C program
[StarGen](https://github.com/dunacl/stargen). It offers three formation models:

- **Standard**: a modern model. A gas disk drains and clears; planet cores
  grow by sweeping up planetesimals and pebbles; the heaviest pull in gas
  and become giants;
  planets drift inward as they grow; close-in planets can lose their
  atmospheres. Compared against exoplanet surveys in `validation/`.
- **Classic**: Dole's accretion model of 1969 with Fogg's extension of 1985,
  written from their papers ([`docs/accretion-model-spec.md`](docs/accretion-model-spec.md))
  and checked against the numbers they print. A historical method, not a
  description of how planets form.

- **High**: the Standard model, with the giant planets' orbits followed
  directly after the gas clears, so that giants packed too closely scatter
  one another. A few seconds where it applies. The run aims at a million
  years but its step budget usually ends it near 30,000.

All work around one, two or three stars.

| | |
|---|---|
| Why the Classic tier was written from the papers | [`docs/adr/0006-rewrite-classic-from-the-papers.md`](docs/adr/0006-rewrite-classic-from-the-papers.md) |
| How the model compares with real planets | [`validation/report-0.14.0.md`](validation/report-0.14.0.md) |
| Contributing | [`CONTRIBUTING.md`](CONTRIBUTING.md) |
| Reporting a vulnerability | [`SECURITY.md`](SECURITY.md) |

The project plan and working notes are not published. References in the code
to `PLAN.md`, its decisions (`D18`) or sections (`§6.2`), and to `CLAUDE.md`,
point at those private documents, and so do most decision records (ADRs), of
which only ADR 0006 is published. The code stands without them.

## Quick start

Needs Rust (via [rustup](https://rustup.rs); the toolchain and the
`wasm32-unknown-unknown` target are pinned in `rust-toolchain.toml`). The
first build takes several minutes.

```sh
make run                        # open the app
make help                       # everything else
```

Some things to try:

```sh
make run PRESET=hot-jupiter SEED=80       # a giant on a 2-day orbit
make run PRESET=circumbinary SCALE=schematic  # planets around two suns
make run TIER=classic                       # Dole's model of 1969
make run TIER=high SEED=412                 # giants that scatter one another
make sim SEED=7                             # one system as text
make validate                               # 3000 systems vs exoplanet surveys
make shots OFFSCREEN=1                      # reference pictures into target/shots
make bench                                  # frame rate
make ci                                     # every automated check
```

*Playback > Spaceship tour* sends a small ship to each planet and moon, for fun;
it is not simulated.

Tick *Frame rate* in *Settings* (or start with `--fps`, or `?fps=1` in a
link) to see frames per second in the corner.

In the app: drag to rotate, scroll or pinch to zoom, click or tap a star or
planet for its data card and a close look at it. The panel on the left chooses the model, the preset and its
parameters; the menu bar holds those and every view and display option; the
bar at the bottom plays the formation back. On a phone they are all in the
*Controls* drawer.

### In a browser

```sh
cargo install wasm-bindgen-cli --version 0.2.129 --locked   # once
make web      # about six minutes: a WebGPU and a WebGL2 build
make serve    # then open http://localhost:8080
```

### Release archives

```sh
make release              # macOS and web into dist/, with SHA256SUMS
make release WHAT=macos   # or web
```

macOS gives a universal `Stardust.app` (ad-hoc signed, not notarised: open
it with right-click > Open the first time). The script can also build Linux
(`WHAT=linux`) and Windows (`WHAT=windows`) in Docker, but neither build has
been completed or tested yet; they need about 8 GB of memory for Docker.

A link can point at a system: `?seed=7&preset=circumbinary&tier=classic&scale=schematic`.

*Help* (or F1) explains everything on the screen, and hovering over a control or a
data-card row says the same. Start with it open with `--help-window`, or `?help=1`
in a link.

The interface speaks English, Spanish, French and German: choose in *Settings*, or
start with `--lang es`, or `?lang=es` in a link. Body names and the program's
command line stay as they are, and numbers keep a decimal point in every language.
On a phone the same page shows a layout for small screens; `&quality=full`
or `&quality=reduced` sets how much it draws.

## What is here

```
apps/sg-app        the application (desktop and web)
crates/
  sg-core          units, deterministic maths, random streams
  sg-stellar       stellar evolution, mass function, multiplicity, habitable zone
  sg-disk          the gas disk
  sg-formation     pebble accretion, gas accretion, migration
  sg-dynamics      orbits, stability limits, late-stage mergers
  sg-planet        planet radii, atmospheric escape, classes
  sg-accrete       the Classic tier: Dole's accretion, Fogg's planets
  sg-schema        the data contract: parameters, snapshot, timeline
  sg-sim           simulate(): puts the above together
  sg-render        the 3D scene, as a Bevy plugin: planets, stars, the disk, belts, sky
  sg-cli           command line: simulate, batch, validate
  sg-wasm          a windowless WebAssembly build used to check determinism
reference/         how the stellar tables were generated
validation/        reports comparing generated populations with surveys
scripts/           the checks `make ci` runs
web/               the page that hosts the browser build
docs/              ADR 0006, the Classic model's specification
```

## Principles

- **Reproducible.** The same parameters, seed and model version give the same
  system on every platform, bit for bit. A check compares the native and
  WebAssembly builds on every run of `make ci`.
- **Cited.** Every physics function names the paper it comes from and the
  range it is valid over, and is tested against a published or analytic
  value.
- **Measured.** The modern model is compared with real exoplanet statistics,
  and the report keeps its failures in. Passing is a calibration, not a
  prediction: the disk masses and the pressure bumps of models 0.11.0 and 0.12.0
  (where the bumps sit, how many pebbles they trap) were set to fit those
  statistics, and no source predicts them. Known limits: too few rocky planets of
  1.25–1.75 Earth radii, probably too many close-in planets smaller than
  the Earth, and giant eccentricities too low (median 0.03 in the High tier
  against about 0.2). Fixing them needs a different account of how solids
  move through the disk, not a calibration. Belts grind down by a published
  law with published values, not fitted ones.
- **Honest about what is drawn.** The renderer draws what the simulation
  produced: a planet's oceans, ice and cloud cover the fractions the model
  gives, a giant's colours follow its temperature, the disk thins and
  clears as the model says. Where it shows something the model does not
  give (the sky, the rocks of a belt, the shape of a coastline, the spaceship), the code
  and the interface say so.

## Licence

Apache-2.0 ([`LICENSE`](LICENSE)). See [`NOTICE.md`](NOTICE.md).
