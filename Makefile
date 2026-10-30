# Stardust shortcuts. `make` or `make help` lists them.
#
# The checks themselves live in scripts/ (scripts/ci.sh is what hosted CI
# runs); these targets only wrap them, so the two cannot drift apart.

# Prefer rustup's toolchain over any other cargo on PATH (see scripts/toolchain.sh).
# The absolute path matters: GNU Make 3.81 (the one macOS ships) looks up simple
# commands with the PATH it was started with, ignoring the export below.
export PATH := $(HOME)/.cargo/bin:$(PATH)
CARGO := $(shell . scripts/toolchain.sh; command -v cargo)

# Overridable on the command line, e.g. `make run SEED=7 PRESET=circumbinary`.
SEED   ?= 42
PRESET ?= sun-like
TIER   ?= standard
SCALE  ?= enlarged
MASS   ?= 1
COUNT  ?= 1000
SHOTS  ?= target/shots
PORT   ?= 8080

SIM_CRATES := sg-core sg-stellar sg-disk sg-formation sg-dynamics sg-planet sg-accrete sg-schema sg-sim
APP        := $(CARGO) run -q -p sg-app --
# OFFSCREEN=1 draws screenshots into an image: no window needs to be visible.
APP        += $(if $(OFFSCREEN),--offscreen,)
APP_ARGS   := --seed $(SEED) --preset $(PRESET) --tier $(TIER) --scale $(SCALE)

.DEFAULT_GOAL := help

.PHONY: help
help: ## List the targets
	@/usr/bin/awk 'BEGIN { FS = ":.*## " } \
		/^##@/ { printf "\n%s\n", substr($$0, 5); next } \
		/^[a-z0-9-]+:.*## / { printf "  %-16s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)
	@printf '\nVariables: SEED=%s PRESET=%s SCALE=%s MASS=%s COUNT=%s PORT=%s\n' \
		'$(SEED)' '$(PRESET)' '$(SCALE)' '$(MASS)' '$(COUNT)' '$(PORT)'
	@printf '  TIER: standard | classic | high\n'
	@printf '  PRESET: sun-like | red-dwarf | hot-jupiter | metal-poor | massive-a-star |\n'
	@printf '          circumbinary | s-type-binary | hierarchical-triple | random\n'
	@printf '  SCALE: true | enlarged | schematic\n'

##@ Everything

.PHONY: ci
ci: ## Every automated check, in order (run before committing)
	scripts/ci.sh

.PHONY: check
check: fmt-check lint test ## The fast subset: format, clippy, tests

##@ Try it by hand

.PHONY: run
run: ## Open the desktop app (SEED, PRESET, TIER, SCALE)
	$(APP) $(APP_ARGS)

.PHONY: run-release
run-release: ## Open the desktop app, optimised build
	$(CARGO) run -q --release -p sg-app -- $(APP_ARGS)

.PHONY: sim
sim: ## Print one system as text (SEED, MASS)
	$(CARGO) run -q -p sg-cli -- simulate --seed $(SEED) --mass $(MASS) --fidelity $(TIER)

.PHONY: sim-json
sim-json: ## Print one system as JSON: snapshot, timeline, diagnostics
	$(CARGO) run -q -p sg-cli -- simulate --seed $(SEED) --mass $(MASS) --json

.PHONY: batch
batch: ## Simulate COUNT systems into target/batch.jsonl
	$(CARGO) run -q --release -p sg-cli -- batch --count $(COUNT) --mass $(MASS) --out target/batch.jsonl

##@ Web build (target/web)

.PHONY: web
web: ## Build the web app, WebGPU and WebGL2 (several minutes)
	scripts/build-web.sh

.PHONY: serve
serve: ## Serve target/web at http://localhost:8080, or PORT (run `make web` first)
	@test -f target/web/index.html || { echo "run 'make web' first"; exit 1; }
	@printf 'Stardust at http://localhost:%s  (add ?renderer=webgl2 to force the fallback)\n' '$(PORT)'
	/usr/bin/python3 -m http.server $(PORT) --bind 127.0.0.1 --directory target/web

.PHONY: serve-lan
serve-lan: ## Serve target/web to other devices on this network (a phone): opens PORT to the local network
	@printf 'On the phone, open  http://%s:%s\n' "$$(/usr/sbin/ipconfig getifaddr en0 || /usr/sbin/ipconfig getifaddr en1)" '$(PORT)'
	/usr/bin/python3 -m http.server $(PORT) --bind 0.0.0.0 --directory target/web

.PHONY: web-shot
web-shot: ## Screenshot the served web app in headless Firefox (needs `make serve` running)
	@mkdir -p $(SHOTS)
	node scripts/web-shot.mjs 'http://127.0.0.1:$(PORT)/?seed=$(SEED)&preset=$(PRESET)&scale=$(SCALE)&time=1' $(SHOTS)/web.png

##@ Release archives (dist/)

.PHONY: release
release: ## Archives for macOS and the web (WHAT=macos|web; linux|windows are untested and need Docker)
	scripts/release.sh $(WHAT)

##@ Visual checks (PNG files in target/shots)

.PHONY: shot
shot: ## One screenshot of the current SEED/PRESET/SCALE
	@mkdir -p $(SHOTS)
	$(APP) $(APP_ARGS) --screenshot $(SHOTS)/shot.png

.PHONY: bench
bench: ## Frame rate of the current SEED/PRESET/SCALE at 2560x1440, drawn off screen
	$(CARGO) run -q --release -p sg-app -- $(APP_ARGS) --bench 6

.PHONY: bench-sim
bench-sim: ## Time the simulation of COUNT systems in the current TIER
	$(CARGO) run -q --release -p sg-cli -- bench --count $(COUNT) --fidelity $(TIER)

.PHONY: shots
shots: ## The reference set: scales, formation, close-ups, red dwarf, binaries, triple, belt
	@mkdir -p $(SHOTS)
	$(APP) --seed $(SEED) --scale enlarged  --screenshot $(SHOTS)/overview-enlarged.png
	$(APP) --seed $(SEED) --scale schematic --screenshot $(SHOTS)/overview-schematic.png
	$(APP) --seed $(SEED) --scale true      --screenshot $(SHOTS)/overview-true.png
	$(APP) --seed $(SEED) --scale schematic --time 0.1 --screenshot $(SHOTS)/forming-10.png
	$(APP) --seed $(SEED) --scale schematic --time 0.5 --screenshot $(SHOTS)/forming-50.png
	$(APP) --seed $(SEED) --select 0 --screenshot $(SHOTS)/star.png
	$(APP) --seed $(SEED) --select 3 --screenshot $(SHOTS)/planet-3.png
	$(APP) --seed $(SEED) --select 7 --screenshot $(SHOTS)/planet-7.png
	$(APP) --seed $(SEED) --tier standard --preset red-dwarf --scale schematic --screenshot $(SHOTS)/red-dwarf.png
	$(APP) --seed $(SEED) --preset s-type-binary --screenshot $(SHOTS)/binary-s-type.png
	$(APP) --seed $(SEED) --preset circumbinary --scale schematic --screenshot $(SHOTS)/binary-circumbinary.png
	$(APP) --seed $(SEED) --preset hierarchical-triple --scale schematic --screenshot $(SHOTS)/triple.png
	$(APP) --seed $(SEED) --preset s-type-binary --select 0 --screenshot $(SHOTS)/binary-star-card.png
	$(APP) --seed 83 --tier classic --select 2 --screenshot $(SHOTS)/asteroid-belt.png
	$(APP) --seed 2 --preset hot-jupiter --select 1 --screenshot $(SHOTS)/hot-jupiter.png
	$(APP) --seed 1 --tier classic --select 3 --screenshot $(SHOTS)/terrestrial.png
	$(APP) --seed 1 --tier classic --select 5 --screenshot $(SHOTS)/jovian.png
	$(APP) --seed 1 --tier classic --select 8 --screenshot $(SHOTS)/martian.png
	$(APP) --seed 42 --tier classic --scale true --select 9 --screenshot $(SHOTS)/far-planet-true-scale.png
	$(APP) --seed 2 --preset hot-jupiter --scale schematic --time 0.25 --screenshot $(SHOTS)/disk-standard.png
	$(APP) --seed 42 --tier classic --scale schematic --time 0.4 --screenshot $(SHOTS)/disk-classic.png
	$(APP) --seed 42 --tier classic --size 390x844 --screenshot $(SHOTS)/phone.png
	$(APP) --seed 1 --tier classic --size 390x844 --select 3 --screenshot $(SHOTS)/phone-card.png
	$(APP) --seed 42 --tier classic --size 390x844 --drawer --screenshot $(SHOTS)/phone-drawer.png
	$(APP) --seed 1 --tier classic --select 3 --quality reduced --screenshot $(SHOTS)/reduced.png
	$(APP) --seed 2 --tier classic --select 8 --screenshot $(SHOTS)/rings-and-moon.png
	$(APP) --seed 42 --tier classic --select 7 --screenshot $(SHOTS)/rings-tilted.png
	$(APP) --seed 1 --tier classic --select 9 --screenshot $(SHOTS)/moons.png
	$(APP) --seed 345 --tier high --scale schematic --screenshot $(SHOTS)/high-scattered-giants.png
	@printf 'screenshots written to %s\n' '$(SHOTS)'

##@ Individual checks

.PHONY: test
test: ## All unit, integration and property tests
	$(CARGO) test --workspace -q

.PHONY: test-sim
test-sim: ## Tests of the simulation crates only (no Bevy; fast)
	$(CARGO) test -q $(addprefix -p ,$(SIM_CRATES))

.PHONY: test-no-classic
test-no-classic: ## sg-sim builds and passes without the Classic tier 
	$(CARGO) test -q -p sg-sim --no-default-features

.PHONY: fmt
fmt: ## Format the code
	$(CARGO) fmt --all

.PHONY: fmt-check
fmt-check: ## Fail if the code is not formatted
	$(CARGO) fmt --all -- --check

.PHONY: lint
lint: ## Clippy with warnings as errors, with and without Classic
	$(CARGO) clippy --workspace --all-targets -- -D warnings
	$(CARGO) clippy -p sg-sim --no-default-features --all-targets -- -D warnings

.PHONY: wasm
wasm: ## Simulation crates and the app compile for wasm32-unknown-unknown
	$(CARGO) build -q --target wasm32-unknown-unknown $(addprefix -p ,$(SIM_CRATES)) -p sg-wasm
	$(CARGO) check -q --target wasm32-unknown-unknown -p sg-app
	$(CARGO) check -q --target wasm32-unknown-unknown -p sg-app --features webgpu

.PHONY: deps
deps: ## Dependency rules: no Bevy in sim crates, Classic separable
	scripts/check-deps.sh

.PHONY: determinism
determinism: ## Native and wasm32 give identical hashes (needs Node.js)
	scripts/check-determinism.sh

.PHONY: validate
validate: ## Standard tier: 3000 systems compared with exoplanet surveys (prints the report)
	$(CARGO) run -q --release -p sg-cli -- validate --count 3000

.PHONY: security
security: ## Private files, secrets, dependency advisories, licences, sources
	scripts/check-security.sh

.PHONY: check-math
check-math: ## No platform-dependent maths in simulation code
	scripts/check-math.sh

##@ Housekeeping

.PHONY: clean-shots
clean-shots: ## Delete target/shots
	/bin/rm -rf $(SHOTS)

.PHONY: clean
clean: ## Delete all build output (the next build takes several minutes)
	$(CARGO) clean
