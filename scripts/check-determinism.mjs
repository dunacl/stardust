// Runs the WASM build of the simulator and prints one result hash per case,
// in the same format as `sg-cli hash`. Used by scripts/check-determinism.sh.
//
//   node scripts/check-determinism.mjs <sg_wasm.wasm> <seed>:<mass> ...

import { readFile } from "node:fs/promises";

const [wasmPath, ...cases] = process.argv.slice(2);
const { instance } = await WebAssembly.instantiate(await readFile(wasmPath), {});

for (const item of cases) {
  const [seed, mass] = item.split(":");
  const hash = instance.exports.sg_classic_result_hash(BigInt(seed), Number(mass));
  console.log(BigInt.asUintN(64, hash).toString(16).padStart(16, "0"));
}
