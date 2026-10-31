//! Windowless WebAssembly entry point for the determinism check.
//!
//! One raw C-ABI function, used by `scripts/check-determinism.sh` to prove
//! that a wasm32 build produces the same bits as the native one. It runs in
//! Node with no GPU or canvas, which the web app (`sg-app` compiled to
//! WebAssembly, PLAN.md D29) cannot.

use sg_schema::{Fidelity, Multiplicity, SimulationParams, SystemSpec};

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

/// Fingerprint of a preset's run: `preset` is an index into
/// [`sg_sim::ALL_PRESETS`], `tier` is 0 for Classic, 1 for Standard and 2
/// for High.
/// Returns 0 if either is out of range or the run fails. This is how the
/// multiple-star path and the Standard tier are compared across targets.
pub fn preset_result_hash(seed: u64, preset: u32, tier: u32) -> u64 {
    let Some(id) = sg_sim::ALL_PRESETS.get(preset as usize) else {
        return 0;
    };
    let fidelity = match tier {
        0 => Fidelity::Classic,
        1 => Fidelity::Standard,
        2 => Fidelity::High,
        _ => return 0,
    };
    match sg_sim::simulate(&sg_sim::preset_params(*id, seed, fidelity)) {
        Ok(result) => u64::from_str_radix(&sg_sim::result_hash(&result), 16).unwrap_or(0),
        Err(_) => 0,
    }
}

/// C-ABI export of [`preset_result_hash`].
#[no_mangle]
pub extern "C" fn sg_preset_result_hash(seed: u64, preset: u32, tier: u32) -> u64 {
    preset_result_hash(seed, preset, tier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_hashes_cover_the_multi_star_path() {
        // Index 5 is the circumbinary preset.
        assert_eq!(sg_sim::ALL_PRESETS[5], sg_schema::PresetId::Circumbinary);
        let h = preset_result_hash(42, 5, 0);
        assert_eq!(format!("{h:016x}"), "eb462cb97facfb6f");
        // The same preset in the Standard tier.
        let h = preset_result_hash(42, 5, 1);
        assert_eq!(format!("{h:016x}"), "77ee60e391761dfc");
        assert_eq!(preset_result_hash(42, 99, 0), 0);
        assert_eq!(preset_result_hash(42, 5, 7), 0);
    }

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
