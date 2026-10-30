# Stardust

A simulator of how planetary systems form: pick a kind of star, press
**Simulate**, watch planets grow in a disk of gas and dust, then explore the
finished system. It runs as a desktop app and in a browser.

Stardust succeeds the C program [StarGen](https://github.com/dunacl/stargen)
and keeps a faithful port of it as one of three formation models:

- **Standard**: a modern model. A gas disk drains and clears; planet cores
  grow by sweeping up planetesimals and pebbles; the heaviest pull in gas
  and become giants;
  planets drift inward as they grow; close-in planets can lose their
  atmospheres. Compared against exoplanet surveys in `validation/`.
- **Classic**: the Dole/StarGen accretion model of 1970–2000, reproduced
  seed for seed against the original C code.

- **High**: the Standard model, with the giant planets' orbits followed
  directly for up to a million years after the gas clears, so that giants
  packed too closely scatter one another. A few seconds where it applies.

All work around one, two or three stars.

| | |
|---|---|
| Design decisions behind the architecture and the science | [`docs/adr/`](docs/adr/) |
| How the model compares with real planets | [`validation/report-0.9.0.md`](validation/report-0.9.0.md) |
| Contributing | [`CONTRIBUTING.md`](CONTRIBUTING.md) |
| Reporting a vulnerability | [`SECURITY.md`](SECURITY.md) |

The project plan and working notes are not published. References in the code
to `PLAN.md`, its decisions (`D18`) or sections (`§6.2`), and to `CLAUDE.md`,
point at those private documents; the code and the ADRs stand without them.

## Quick start

Needs Rust (via [rustup](https://rustup.rs); the toolchain and the
`wasm32-unknown-unknown` target are pinned in `rust-toolchain.toml`). The
first build takes several minutes.

```sh
git submodule update --init     # the original StarGen, used as a reference
make run                        # open the app
make help                       # everything else
```

Some things to try:

```sh
make run PRESET=hot-jupiter SEED=2          # a giant on a four-day orbit
make run PRESET=circumbinary SCALE=schematic  # planets around two suns
make run TIER=classic                       # the StarGen model
make run TIER=high SEED=345                 # giants that scatter one another
make run TIER=classic SEED=1                # click the 7th planet: rings and a moon
make sim SEED=7                             # one system as text
make validate                               # 3000 systems vs exoplanet surveys
make shots OFFSCREEN=1                      # reference pictures into target/shots
make bench                                  # frame rate
make ci                                     # every automated check
```

Tick *Frame rate* in the panel (or start with `--fps`, or `?fps=1` in a
link) to see frames per second in the corner.

In the app: drag to rotate, scroll or pinch to zoom, click or tap a star or
planet for its data card and a close look at it. The panel on the left chooses the model, the preset and its
parameters; the bar at the bottom plays the formation back.

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

A link can point at a system: `?seed=7&preset=red-dwarf&tier=classic&scale=schematic`.
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
  sg-classic       the port of StarGen (separable; see below)
  sg-schema        the data contract: parameters, snapshot, timeline
  sg-sim           simulate(): puts the above together
  sg-render        the 3D scene, as a Bevy plugin: planets, stars, the disk, belts, sky
  sg-cli           command line: simulate, batch, validate, compare with the C code
  sg-wasm          a windowless WebAssembly build used to check determinism
reference/         the original C code (submodule), its test harness, golden outputs
validation/        reports comparing generated populations with surveys
scripts/           the checks `make ci` runs
web/               the page that hosts the browser build
docs/              progress, decision records, the Classic model's constants
```

## Principles

- **Reproducible.** The same parameters, seed and model version give the same
  system on every platform, bit for bit. A check compares the native and
  WebAssembly builds on every run of `make ci`.
- **Cited.** Every physics function names the paper it comes from and the
  range it is valid over, and is tested against a published or analytic
  value.
- **Measured.** The modern model is compared with real exoplanet statistics,
  and the report keeps its failures in.
- **Honest about what is drawn.** The renderer draws what the simulation
  produced: a planet's oceans, ice and cloud cover the fractions the model
  gives, a giant's colours follow its temperature, the disk thins and
  clears as the model says. Where it shows something the model does not
  give (the sky, the rocks of a belt, the shape of a coastline), the code
  and the interface say so.

## Licence

GPL version 3 ([`LICENSE`](LICENSE)), except the parts derived from StarGen.
The original is "free for your use as long as you don't sell it"; those terms
still apply to its port, `sg-classic`, which is kept separable, and to the
golden outputs. So builds that include the Classic tier must not be sold. See
[`NOTICE.md`](NOTICE.md) for exactly what is covered.
