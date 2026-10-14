// Runs the WASM build of the simulator and prints one result hash per case,
// in the same format as `sg-cli hash`. Used by scripts/check-determinism.sh.
//
//   node scripts/check-determinism.mjs <sg_wasm.wasm> <seed>:<mass|preset> ...
//
// A case is a single star of a given mass (`42:1.0`), a preset by name
// (`42:circumbinary`), or a preset in the Standard tier
// (`42:circumbinary@standard`) or the High tier (`45:sun-like@high`).

import { readFile } from "node:fs/promises";

// The order of sg_sim::ALL_PRESETS.
const PRESETS = [
  "sun-like",
  "red-dwarf",
  "hot-jupiter",
  "massive-a-star",
  "metal-poor",
  "circumbinary",
  "s-type-binary",
  "hierarchical-triple",
  "random",
];

const [wasmPath, ...cases] = process.argv.slice(2);
const { instance } = await WebAssembly.instantiate(await readFile(wasmPath), {});

for (const item of cases) {
  const [seed, what] = item.split(":");
  const [name, tier] = what.split("@");
  const preset = PRESETS.indexOf(name);
  const hash =
    preset >= 0
      ? instance.exports.sg_preset_result_hash(BigInt(seed), preset, tier === "standard" ? 1 : tier === "high" ? 2 : 0)
      : instance.exports.sg_classic_result_hash(BigInt(seed), Number(what));
  console.log(BigInt.asUintN(64, hash).toString(16).padStart(16, "0"));
}
