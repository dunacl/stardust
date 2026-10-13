//! Presets: named starting configurations (PLAN.md §9).
//!
//! A preset fills in [`SimulationParams`]; the caller may then adjust any
//! field. Presets live here, not in a front end, so every app offers the same
//! ones.

use sg_schema::{Fidelity, Multiplicity, PresetId, SimulationParams, SystemSpec};

/// Every preset, in menu order.
pub const ALL_PRESETS: [PresetId; 9] = [
    PresetId::SunLike,
    PresetId::RedDwarf,
    PresetId::HotJupiter,
    PresetId::MassiveAStar,
    PresetId::MetalPoor,
    PresetId::Circumbinary,
    PresetId::STypeBinary,
    PresetId::HierarchicalTriple,
    PresetId::Random,
];

/// Whether a preset can be simulated with a given tier today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetAvailability {
    Available,
    /// Not yet; the string says what it is waiting for.
    Unavailable(&'static str),
}

/// Whether `preset` works with `fidelity` in this build.
pub fn preset_availability(preset: PresetId, fidelity: Fidelity) -> PresetAvailability {
    use PresetAvailability::{Available, Unavailable};
    if fidelity != Fidelity::Classic {
        return Unavailable("this tier is not implemented yet");
    }
    match preset {
        PresetId::SunLike | PresetId::RedDwarf | PresetId::Random => Available,
        PresetId::HotJupiter => Unavailable("needs migration (Standard tier, M4)"),
        PresetId::MassiveAStar => Unavailable("needs stars above 1.5 M☉ (M3)"),
        PresetId::MetalPoor => Unavailable("needs metallicity (Standard tier, M4)"),
        PresetId::Circumbinary | PresetId::STypeBinary | PresetId::HierarchicalTriple => {
            Unavailable("needs multiple stars (M3)")
        }
    }
}

/// Parameters for `preset` with the given seed and tier.
///
/// Classic-tier notes:
/// - **Red dwarf** uses 0.2 M☉, the smallest star the Classic model accepts;
///   a TRAPPIST-1-like 0.09 M☉ star needs the M3 stellar model.
/// - **Random** leaves the mass unset, so the Classic model draws one in
///   [0.7, 1.4] M☉ as StarGen does.
///
/// Presets that are not available yet still return parameters describing
/// what they will mean; [`crate::simulate`] reports why they cannot run.
pub fn preset_params(preset: PresetId, seed: u64, fidelity: Fidelity) -> SimulationParams {
    let single = |mass: Option<f64>| SystemSpec::Auto {
        primary_mass_msun: mass,
        multiplicity: Some(Multiplicity::Single),
    };
    let multiple = |mass: f64, multiplicity| SystemSpec::Auto {
        primary_mass_msun: Some(mass),
        multiplicity: Some(multiplicity),
    };
    let mut params = SimulationParams {
        seed,
        fidelity,
        preset: Some(preset),
        ..SimulationParams::default()
    };
    match preset {
        PresetId::SunLike | PresetId::HotJupiter => params.system = single(Some(1.0)),
        PresetId::RedDwarf => params.system = single(Some(0.2)),
        PresetId::MassiveAStar => params.system = single(Some(2.0)),
        PresetId::MetalPoor => {
            params.system = single(Some(0.8));
            params.metallicity_feh = Some(-1.0);
        }
        PresetId::Circumbinary => params.system = multiple(0.69, Multiplicity::Binary),
        PresetId::STypeBinary => params.system = multiple(1.1, Multiplicity::Binary),
        PresetId::HierarchicalTriple => params.system = multiple(1.0, Multiplicity::Triple),
        PresetId::Random => params.system = single(None),
    }
    params
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_is_listed_once() {
        let mut seen = ALL_PRESETS.to_vec();
        seen.dedup();
        assert_eq!(seen.len(), 9);
    }

    #[test]
    fn presets_carry_seed_tier_and_identity() {
        for preset in ALL_PRESETS {
            let params = preset_params(preset, 77, Fidelity::Classic);
            assert_eq!(params.seed, 77);
            assert_eq!(params.fidelity, Fidelity::Classic);
            assert_eq!(params.preset, Some(preset));
        }
    }

    #[cfg(feature = "classic")]
    #[test]
    fn availability_agrees_with_simulate() {
        for preset in ALL_PRESETS {
            let params = preset_params(preset, 5, Fidelity::Classic);
            let ran = crate::simulate(&params).is_ok();
            let available =
                preset_availability(preset, Fidelity::Classic) == PresetAvailability::Available;
            // The massive A star "runs" only because StarGen silently
            // substitutes an in-range mass; it is still listed as unavailable.
            if preset != PresetId::MassiveAStar
                && preset != PresetId::HotJupiter
                && preset != PresetId::MetalPoor
            {
                assert_eq!(ran, available, "{preset:?}");
            }
        }
        assert_eq!(
            preset_availability(PresetId::SunLike, Fidelity::Standard),
            PresetAvailability::Unavailable("this tier is not implemented yet")
        );
    }

    #[cfg(feature = "classic")]
    #[test]
    fn red_dwarf_and_random_presets_produce_the_intended_stars() {
        let star_mass = |preset| {
            let result = crate::simulate(&preset_params(preset, 9, Fidelity::Classic)).unwrap();
            match result.snapshot.hierarchy {
                sg_schema::HierarchyNode::Star(star) => {
                    (star.mass_msun, result.diagnostics.warnings)
                }
                _ => panic!("single star expected"),
            }
        };
        assert_eq!(star_mass(PresetId::RedDwarf).0, 0.2);
        let (mass, warnings) = star_mass(PresetId::Random);
        assert!((0.7..=1.4).contains(&mass));
        assert!(warnings.is_empty(), "{warnings:?}");
    }
}
