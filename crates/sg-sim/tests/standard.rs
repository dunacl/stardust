//! End-to-end tests of the Standard tier through the public API. These run
//! with or without the Classic tier.

use sg_schema::*;
use sg_sim::{preset_params, result_hash, simulate, SimulationResult, StellarModel, ALL_PRESETS};

fn sun(seed: u64) -> SimulationParams {
    SimulationParams {
        seed,
        fidelity: Fidelity::Standard,
        ..SimulationParams::default()
    }
}

fn stars(node: &HierarchyNode) -> Vec<&Star> {
    match node {
        HierarchyNode::Star(star) => vec![star],
        HierarchyNode::Pair { a, b, .. } => {
            let mut all = stars(a);
            all.extend(stars(b));
            all
        }
    }
}

/// Everything a consumer of the snapshot relies on.
fn check_consistency(result: &SimulationResult) {
    let snap = &result.snapshot;
    assert_eq!(result.diagnostics.fidelity, Fidelity::Standard);
    assert_eq!(
        result.diagnostics.stellar_model,
        StellarModel::HurleyPolsTout2000
    );
    let mut ids: Vec<BodyId> = snap.bodies.iter().map(|b| b.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), snap.bodies.len());

    for body in snap.planets() {
        let zone = snap
            .planet_zones
            .iter()
            .find(|z| z.host == body.host)
            .unwrap();
        assert!(body.orbit.a_au >= zone.inner_au * 0.999 && body.orbit.a_au <= zone.outer_au);
        assert!((0.0..0.61).contains(&body.orbit.e));
        assert!(
            body.mass_mearth >= 0.01 && body.mass_mearth < 2.0e4,
            "{}",
            body.mass_mearth
        );
        assert!(
            body.radius_km > 500.0 && body.radius_km < 80_000.0,
            "{}",
            body.radius_km
        );
        assert!(
            body.density_g_cm3 > 0.05 && body.density_g_cm3 < 30.0,
            "{}",
            body.density_g_cm3
        );
        assert!(body.equilibrium_temp_k > 0.0);
        let c = &body.composition;
        let sum = c.metal_frac + c.rock_frac + c.water_frac + c.volatiles_frac + c.h_he_frac;
        assert!((sum - 1.0).abs() < 1e-9, "{sum}");
        assert_eq!(body.atmosphere.is_some(), c.h_he_frac > 0.0);
        assert!(body.climate.is_none());
        // Mostly-gas planets have no surface to stand on.
        assert_eq!(body.surface_gravity_gees.is_none(), c.h_he_frac >= 0.1);
        assert_eq!(
            body.rotation.period_hours.is_some(),
            body.rotation.tidally_locked
        );
        let giant = matches!(body.class, PlanetClass::GasGiant | PlanetClass::HotJupiter);
        assert_eq!(giant, c.h_he_frac >= 0.5);
    }
    // Planets of one host are listed outward.
    for pair in snap.planets().collect::<Vec<_>>().windows(2) {
        if pair[0].host == pair[1].host {
            assert!(pair[0].orbit.a_au <= pair[1].orbit.a_au);
        }
    }
    for star in stars(&snap.hierarchy) {
        assert!(snap.age_gyr <= star.main_sequence_lifetime_gyr);
    }
    for disk in &snap.disks_at_dispersal {
        assert!(disk.lifetime_myr > 0.0 && disk.initial_mass_msun > 0.0);
        assert!(disk.inner_au < disk.outer_au);
        assert!(disk.final_ice_line_au >= disk.inner_au);
    }

    // The timeline is ordered and ends with exactly the finished planets.
    let t = &result.timeline;
    assert!(t.events.windows(2).all(|w| w[0].t_myr <= w[1].t_myr));
    assert!(t.keyframes.windows(2).all(|w| w[0].t_myr < w[1].t_myr));
    let last = t.keyframes.last().unwrap();
    let mut final_ids: Vec<BodyId> = last.bodies.iter().map(|b| b.id).collect();
    final_ids.sort();
    let mut planet_ids: Vec<BodyId> = snap.planets().map(|b| b.id).collect();
    planet_ids.sort();
    assert_eq!(final_ids, planet_ids);
    for body in &last.bodies {
        let planet = snap.bodies.iter().find(|b| b.id == body.id).unwrap();
        assert_eq!(body.mass_mearth, planet.mass_mearth);
        assert_eq!(body.host, planet.host);
    }
}

#[test]
fn same_params_give_identical_results() {
    let a = simulate(&sun(42)).unwrap();
    assert_eq!(a, simulate(&sun(42)).unwrap());
    assert_ne!(result_hash(&a), result_hash(&simulate(&sun(43)).unwrap()));
}

#[test]
fn sun_like_systems_are_consistent_for_many_seeds() {
    let mut with_planets = 0;
    for seed in 1..=150 {
        let result = simulate(&sun(seed)).unwrap();
        check_consistency(&result);
        assert_eq!(stars(&result.snapshot.hierarchy).len(), 1);
        assert_eq!(result.snapshot.disks_at_dispersal.len(), 1);
        if !result.snapshot.bodies.is_empty() {
            with_planets += 1;
        }
    }
    assert!(with_planets > 120, "{with_planets}");
}

#[test]
fn every_preset_runs_and_is_consistent() {
    for preset in ALL_PRESETS {
        for seed in [1, 7, 42] {
            let result = simulate(&preset_params(preset, seed, Fidelity::Standard)).unwrap();
            check_consistency(&result);
        }
    }
}

/// Kepler-16-like and α Centauri-like systems: planets form in the same
/// zones as in M3, now from a disk truncated to fit.
#[test]
fn multiple_stars_confine_their_planets() {
    for seed in 1..=20 {
        let circumbinary = simulate(&preset_params(
            PresetId::Circumbinary,
            seed,
            Fidelity::Standard,
        ))
        .unwrap();
        for body in circumbinary.snapshot.planets() {
            assert_eq!(body.host, HostRef::Barycenter(NodeId(0)));
            assert!(body.orbit.a_au > 0.64);
        }
        let binary = simulate(&preset_params(
            PresetId::STypeBinary,
            seed,
            Fidelity::Standard,
        ))
        .unwrap();
        for body in binary.snapshot.planets() {
            assert!(matches!(body.host, HostRef::Star(_)));
            assert!(body.orbit.a_au < 2.8, "{}", body.orbit.a_au);
        }
        // Each star's disk is cut off inside 3 AU.
        let around_stars = binary
            .snapshot
            .disks_at_dispersal
            .iter()
            .filter(|d| matches!(d.host, HostRef::Star(_)));
        assert!(
            around_stars.clone().count() == 2 && around_stars.clone().all(|d| d.outer_au < 3.0)
        );
    }
}

/// The disk and the star can be fixed, and the outcome responds the way
/// the physics says: a massive, long-lived, metal-rich disk builds more
/// than a light, short-lived, metal-poor one.
#[test]
fn outcome_responds_to_the_disk() {
    let total_mass = |feh: f64, fraction: f64, life: f64| {
        let mut sum = 0.0;
        for seed in 1..=40 {
            let mut params = sun(seed);
            params.metallicity_feh = Some(feh);
            params.disk.mass_fraction = Some(fraction);
            params.disk.lifetime_myr = Some(life);
            let result = simulate(&params).unwrap();
            let disk = &result.snapshot.disks_at_dispersal[0];
            assert!((disk.initial_mass_msun - fraction).abs() < 1e-12);
            assert!((disk.lifetime_myr - life).abs() < 1e-12);
            assert!((disk.dust_to_gas - 0.0149 * 10f64.powf(feh)).abs() < 1e-12);
            sum += result
                .snapshot
                .bodies
                .iter()
                .map(|b| b.mass_mearth)
                .sum::<f64>();
        }
        sum
    };
    let rich = total_mass(0.3, 0.08, 5.0);
    let poor = total_mass(-0.5, 0.01, 1.0);
    assert!(rich > 20.0 * poor, "{rich} vs {poor}");
}

#[test]
fn migration_can_be_switched_off() {
    let mut params = sun(42);
    params.toggles.migration = false;
    let fixed = simulate(&params).unwrap();
    check_consistency(&fixed);
    // Without migration nothing ends up at the disk's inner edge.
    let inner = fixed.snapshot.disks_at_dispersal[0].inner_au;
    assert!(fixed.snapshot.planets().all(|b| b.orbit.a_au > 1.9 * inner));
}

#[test]
fn invalid_parameters_are_rejected() {
    let mut params = sun(1);
    params.disk.mass_fraction = Some(0.9);
    assert!(simulate(&params).is_err());
    let mut params = sun(1);
    params.formation.standard.alpha_turbulent = 0.0;
    assert!(simulate(&params).is_err());
    let mut params = sun(1);
    params.formation.standard.planetesimal_efficiency = -0.1;
    assert!(simulate(&params).is_err());
    let mut params = sun(1);
    params.metallicity_feh = Some(f64::NAN);
    assert!(simulate(&params).is_err());
}

/// The tier must stay fast: far below a second per system even unoptimised.
#[test]
fn a_run_takes_a_bounded_number_of_steps() {
    for seed in 1..=30 {
        let result = simulate(&sun(seed)).unwrap();
        // `rng_draws` reports the integration steps of the growth model.
        assert!(
            result.diagnostics.rng_draws < 400_000,
            "{}",
            result.diagnostics.rng_draws
        );
    }
}

/// Pinned so that an accidental change fails on every platform. Update
/// together with `MODEL_VERSION`.
#[test]
fn standard_hashes_are_pinned() {
    let got = [
        result_hash(&simulate(&sun(42)).unwrap()),
        result_hash(
            &simulate(&preset_params(PresetId::HotJupiter, 42, Fidelity::Standard)).unwrap(),
        ),
        result_hash(
            &simulate(&preset_params(
                PresetId::Circumbinary,
                42,
                Fidelity::Standard,
            ))
            .unwrap(),
        ),
        result_hash(&simulate(&preset_params(PresetId::Random, 42, Fidelity::Standard)).unwrap()),
    ];
    assert_eq!(got, PINNED);
}

const PINNED: [&str; 4] = [
    "5d1ae9816fe56bce",
    "87930c5451759748",
    "a0c3356b035c02b5",
    "21c1304defff16ca",
];

#[test]
fn the_hot_jupiter_preset_makes_hot_jupiters() {
    let hot = |seed| {
        simulate(&preset_params(
            PresetId::HotJupiter,
            seed,
            Fidelity::Standard,
        ))
        .unwrap()
        .snapshot
        .planets()
        .any(|b| b.mass_mearth >= 95.0 && b.orbit.period_days < 10.0)
    };
    assert!(hot(80));
    assert!((1..=20).filter(|seed| hot(*seed)).count() >= 2);
}
