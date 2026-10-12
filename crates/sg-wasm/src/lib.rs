//! WebAssembly entry points.
//!
//! M1 exposes one raw C-ABI function, used by `scripts/check-determinism.sh`
//! to prove that the WASM build produces the same bits as the native one.
//! The `wasm-bindgen` API for JS/TS consumers arrives with the web build
//! (M2).

use sg_schema::{Multiplicity, SimulationParams, SystemSpec};

/// Fingerprint of a Classic run (snapshot and timeline) for a seed and
/// stellar mass, as computed by [`sg_sim::result_hash`]. Returns 0 if the
/// run fails.
pub fn classic_result_hash(seed: u64, mass_msun: f64) -> u64 {
    let params = SimulationParams {
        seed,
        system: SystemSpec::Auto {
            primary_mass_msun: Some(mass_msun),
            multiplicity: Some(Multiplicity::Single),
        },
        ..SimulationParams::default()
    };
    match sg_sim::simulate(&params) {
        Ok(result) => u64::from_str_radix(&sg_sim::result_hash(&result), 16).unwrap_or(0),
        Err(_) => 0,
    }
}

/// C-ABI export of [`classic_result_hash`]. In JavaScript the `u64`
/// arguments and result are `BigInt`s.
#[no_mangle]
pub extern "C" fn sg_classic_result_hash(seed: u64, mass_msun: f64) -> u64 {
    classic_result_hash(seed, mass_msun)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_and_nonzero() {
        let h = classic_result_hash(42, 1.0);
        assert_ne!(h, 0);
        assert_eq!(h, classic_result_hash(42, 1.0));
        assert_ne!(h, classic_result_hash(43, 1.0));
    }

    #[test]
    fn failed_runs_return_zero() {
        assert_eq!(classic_result_hash(0, 1.0), 0);
    }
}
