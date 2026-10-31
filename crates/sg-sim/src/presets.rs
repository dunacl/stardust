//! Presets: named starting configurations (PLAN.md §9).
//!
//! A preset fills in [`SimulationParams`]; the caller may then adjust any
//! field. Presets live here, not in a front end, so every app offers the same
//! ones.

use sg_schema::{
    Fidelity, Multiplicity, OrbitSpec, PresetId, SimulationParams, StarNode, StarSpec, SystemSpec,
};

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
    match (fidelity, preset) {
        (Fidelity::Standard | Fidelity::High, _) => Available,
        (Fidelity::Classic, PresetId::HotJupiter) => {
            Unavailable("needs migration: use the Standard tier")
        }
        (Fidelity::Classic, PresetId::RedDwarf) => Unavailable(
            "Classic's scalings overproduce giants at red dwarfs: use the Standard tier",
        ),
        (Fidelity::Classic, PresetId::MetalPoor) => {
            Unavailable("the Classic model ignores metallicity: use the Standard tier")
        }
        (Fidelity::Classic, _) => Available,
    }
}

/// Parameters for `preset` with the given seed and tier.
///
/// - **Sun-like**, and **Red dwarf** (0.2 M☉), which needs the Standard tier:
///   Fogg's scalings, extrapolated that far, give such a star several Jupiters.
/// - **Massive A star**: 2 M☉.
/// - **Circumbinary**: the stars of Kepler-16 (Doyle et al. 2011, Science
///   333, 1602): 0.69 and 0.20 M☉, 0.224 AU apart, e = 0.16.
/// - **S-type binary**: the stars of α Centauri AB (Pourbaix & Boffin 2016,
///   A&A 586, A90): 1.08 and 0.91 M☉, 23.5 AU apart, e = 0.52.
/// - **Hierarchical triple**: an invented, compact example, so all three
///   stars fit in one view: a 1.0 + 0.7 M☉ pair 0.25 AU apart, orbited at
///   60 AU by a 0.4 M☉ star.
/// - **Random**: primary mass from the IMF and companions from observed
///   multiplicity statistics.
///
/// The planets of the three multiple-star presets are whatever the model
/// forms around those stars; the real systems' planets are not put in.
/// - **Hot Jupiter** and **Metal-poor** (a 0.8 M☉ star with a tenth of the
///   Sun's metals) need the Standard tier, whose formation model responds
///   to metallicity and includes migration.
///
/// With the Standard tier a Sun-like or red-dwarf preset leaves the
/// metallicity and the disk to be drawn; the Classic tier ignores them.
pub fn preset_params(preset: PresetId, seed: u64, fidelity: Fidelity) -> SimulationParams {
    let single = |mass: f64| SystemSpec::Auto {
        primary_mass_msun: Some(mass),
        multiplicity: Some(Multiplicity::Single),
    };
    let star = |mass_msun: f64| Box::new(StarNode::Star(StarSpec { mass_msun }));
    let pair = |a: Box<StarNode>, b: Box<StarNode>, a_au: f64, e: f64| StarNode::Pair {
        a,
        b,
        orbit: OrbitSpec { a_au, e },
    };
    let mut params = SimulationParams {
        seed,
        fidelity,
        preset: Some(preset),
        ..SimulationParams::default()
    };
    match preset {
        PresetId::SunLike => params.system = single(1.0),
        PresetId::HotJupiter => {
            // Conditions that favour giants forming and migrating far in: a
            // metal-rich star with a massive, long-lived disk. Not every
            // seed produces a hot Jupiter.
            params.system = single(1.0);
            // Calibrated: giants move this far only through a long-lived
            // disk whose gas flows fast. About one seed in six gives a
            // giant inside 10 days and two in three one inside 100.
            params.metallicity_feh = Some(0.3);
            params.disk.mass_fraction = Some(0.03);
            params.disk.lifetime_myr = Some(8.0);
            params.disk.alpha_viscous = 1.0e-2;
            params.formation.standard.alpha_gap = 1.0e-3;
        }
        PresetId::RedDwarf => params.system = single(0.2),
        PresetId::MassiveAStar => params.system = single(2.0),
        PresetId::MetalPoor => {
            params.system = single(0.8);
            params.metallicity_feh = Some(-1.0);
        }
        PresetId::Circumbinary => {
            params.system = SystemSpec::Explicit(pair(star(0.6897), star(0.2026), 0.2243, 0.159));
        }
        PresetId::STypeBinary => {
            params.system = SystemSpec::Explicit(pair(star(1.08), star(0.91), 23.5, 0.52));
        }
        PresetId::HierarchicalTriple => {
            params.system = SystemSpec::Explicit(pair(
                Box::new(pair(star(1.0), star(0.7), 0.25, 0.1)),
                star(0.4),
                60.0,
                0.3,
            ));
        }
        PresetId::Random => {
            params.system = SystemSpec::Auto {
                primary_mass_msun: None,
                multiplicity: None,
            };
        }
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
    fn available_presets_run() {
        for preset in ALL_PRESETS {
            let params = preset_params(preset, 5, Fidelity::Classic);
            if preset_availability(preset, Fidelity::Classic) == PresetAvailability::Available {
                let result = crate::simulate(&params);
                assert!(result.is_ok(), "{preset:?}: {:?}", result.err());
            }
        }
        for preset in ALL_PRESETS {
            assert_eq!(
                preset_availability(preset, Fidelity::Standard),
                PresetAvailability::Available
            );
            let result = crate::simulate(&preset_params(preset, 5, Fidelity::Standard));
            assert!(result.is_ok(), "{preset:?}: {:?}", result.err());
        }
    }

    #[cfg(feature = "classic")]
    #[test]
    fn presets_produce_the_intended_stars() {
        use sg_schema::HierarchyNode;
        fn count(node: &HierarchyNode) -> usize {
            match node {
                HierarchyNode::Star(_) => 1,
                HierarchyNode::Pair { a, b, .. } => count(a) + count(b),
            }
        }
        let stars = |preset| {
            let result = crate::simulate(&preset_params(preset, 9, Fidelity::Classic)).unwrap();
            (count(&result.snapshot.hierarchy), result)
        };
        let (n, red_dwarf) = stars(PresetId::RedDwarf);
        assert_eq!(n, 1);
        let HierarchyNode::Star(star) = &red_dwarf.snapshot.hierarchy else {
            panic!("single star expected");
        };
        assert_eq!(star.mass_msun, 0.2);

        let (n, a_star) = stars(PresetId::MassiveAStar);
        assert_eq!(n, 1);
        assert_eq!(
            a_star.diagnostics.stellar_model,
            crate::StellarModel::HurleyPolsTout2000
        );
        let HierarchyNode::Star(star) = &a_star.snapshot.hierarchy else {
            panic!("single star expected");
        };
        assert_eq!(star.mass_msun, 2.0);
        // A-type when young; it cools to F late on the main sequence.
        assert!(
            star.spectral_class.starts_with(['A', 'F']),
            "{}",
            star.spectral_class
        );

        assert_eq!(stars(PresetId::Circumbinary).0, 2);
        assert_eq!(stars(PresetId::STypeBinary).0, 2);
        assert_eq!(stars(PresetId::HierarchicalTriple).0, 3);
    }
}
