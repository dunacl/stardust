# StarGen-NG shortcuts. `make` or `make help` lists them.
#
# The checks themselves live in scripts/ (scripts/ci.sh is what hosted CI will
# run); these targets only wrap them, so the two cannot drift apart.

# Prefer rustup's toolchain over any other cargo on PATH (see scripts/toolchain.sh).
# The absolute path matters: GNU Make 3.81 (the one macOS ships) looks up simple
# commands with the PATH it was started with, ignoring the export below.
export PATH := $(HOME)/.cargo/bin:$(PATH)
CARGO := $(shell . scripts/toolchain.sh; command -v cargo)

# Overridable on the command line, e.g. `make run SEED=7 PRESET=red-dwarf`.
SEED   ?= 42
PRESET ?= sun-like
SCALE  ?= enlarged
MASS   ?= 1
COUNT  ?= 1000
SHOTS  ?= target/shots

SIM_CRATES := sg-core sg-stellar sg-disk sg-formation sg-dynamics sg-planet sg-classic sg-schema sg-sim
APP        := $(CARGO) run -q -p sg-app --
APP_ARGS   := --seed $(SEED) --preset $(PRESET) --scale $(SCALE)

.DEFAULT_GOAL := help

.PHONY: help
help: ## List the targets
	@/usr/bin/awk 'BEGIN { FS = ":.*## " } \
		/^##@/ { printf "\n%s\n", substr($$0, 5); next } \
		/^[a-z0-9-]+:.*## / { printf "  %-16s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)
	@printf '\nVariables: SEED=%s PRESET=%s SCALE=%s MASS=%s COUNT=%s\n' \
		'$(SEED)' '$(PRESET)' '$(SCALE)' '$(MASS)' '$(COUNT)'
	@printf '  PRESET: sun-like | red-dwarf | random    SCALE: true | enlarged | schematic\n'

##@ Everything

.PHONY: ci
ci: ## Every automated check, in order (run before committing)
	scripts/ci.sh

.PHONY: check
check: fmt-check lint test ## The fast subset: format, clippy, tests

##@ Try it by hand

.PHONY: run
run: ## Open the desktop app (SEED, PRESET, SCALE)
	$(APP) $(APP_ARGS)

.PHONY: run-release
run-release: ## Open the desktop app, optimised build
	$(CARGO) run -q --release -p sg-app -- $(APP_ARGS)

.PHONY: sim
sim: ## Print one system as text (SEED, MASS)
	$(CARGO) run -q -p sg-cli -- simulate --seed $(SEED) --mass $(MASS)

.PHONY: sim-json
sim-json: ## Print one system as JSON: snapshot, timeline, diagnostics
	$(CARGO) run -q -p sg-cli -- simulate --seed $(SEED) --mass $(MASS) --json

.PHONY: batch
batch: ## Simulate COUNT systems into target/batch.jsonl
	$(CARGO) run -q --release -p sg-cli -- batch --count $(COUNT) --mass $(MASS) --out target/batch.jsonl

##@ Visual checks (PNG files in target/shots)

.PHONY: shot
shot: ## One screenshot of the current SEED/PRESET/SCALE
	@mkdir -p $(SHOTS)
	$(APP) $(APP_ARGS) --screenshot $(SHOTS)/shot.png

.PHONY: shots
shots: ## The reference set: three scales, formation, close-ups, red dwarf
	@mkdir -p $(SHOTS)
	$(APP) --seed $(SEED) --scale enlarged  --screenshot $(SHOTS)/overview-enlarged.png
	$(APP) --seed $(SEED) --scale schematic --screenshot $(SHOTS)/overview-schematic.png
	$(APP) --seed $(SEED) --scale true      --screenshot $(SHOTS)/overview-true.png
	$(APP) --seed $(SEED) --scale schematic --time 0.1 --screenshot $(SHOTS)/forming-10.png
	$(APP) --seed $(SEED) --scale schematic --time 0.5 --screenshot $(SHOTS)/forming-50.png
	$(APP) --seed $(SEED) --select 0 --screenshot $(SHOTS)/star.png
	$(APP) --seed $(SEED) --select 3 --screenshot $(SHOTS)/planet-3.png
	$(APP) --seed $(SEED) --select 7 --screenshot $(SHOTS)/planet-7.png
	$(APP) --seed $(SEED) --preset red-dwarf --scale schematic --screenshot $(SHOTS)/red-dwarf.png
	@printf 'screenshots written to %s\n' '$(SHOTS)'

##@ Individual checks

.PHONY: test
test: ## All unit, integration and property tests
	$(CARGO) test --workspace -q

.PHONY: test-sim
test-sim: ## Tests of the simulation crates only (no Bevy; fast)
	$(CARGO) test -q $(addprefix -p ,$(SIM_CRATES))

.PHONY: test-no-classic
test-no-classic: ## sg-sim builds and passes without the Classic tier (PLAN.md D18)
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
wasm: ## Simulation crates build for wasm32-unknown-unknown
	$(CARGO) build -q --target wasm32-unknown-unknown $(addprefix -p ,$(SIM_CRATES)) -p sg-wasm

.PHONY: deps
deps: ## Dependency rules: no Bevy in sim crates, Classic separable
	scripts/check-deps.sh

.PHONY: determinism
determinism: ## Native and wasm32 give identical hashes (needs Node.js)
	scripts/check-determinism.sh

.PHONY: golden
golden: ## Classic tier vs the 450 golden systems of the C reference
	$(CARGO) run -q --release -p sg-cli -- compare-golden

.PHONY: golden-check
golden-check: ## Rebuild the C reference; confirm reference/golden is reproduced
	scripts/golden.sh --check

.PHONY: golden-regen
golden-regen: ## Rewrite reference/golden from the C reference (changes files)
	scripts/golden.sh

##@ Housekeeping

.PHONY: clean-shots
clean-shots: ## Delete target/shots
	/bin/rm -rf $(SHOTS)

.PHONY: clean
clean: ## Delete all build output (the next build takes several minutes)
	$(CARGO) clean
