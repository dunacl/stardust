//! End-to-end tests of the Classic tier through the public API.
#![cfg(feature = "classic")]

use proptest::prelude::*;
use sg_schema::*;
use sg_sim::{
    result_hash, simulate, simulate_with, snapshot_hash, Control, SimError, SimObserver, Stage,
    StellarModel, MODEL_VERSION,
};

fn params(seed: u64, mass: f64) -> SimulationParams {
    SimulationParams {
        seed,
        system: SystemSpec::Auto {
            primary_mass_msun: Some(mass),
            multiplicity: Some(Multiplicity::Single),
        },
        ..SimulationParams::default()
    }
}

#[test]
fn same_params_give_identical_results() {
    let a = simulate(&params(42, 1.0)).unwrap();
    let b = simulate(&params(42, 1.0)).unwrap();
    assert_eq!(a, b);
    assert_eq!(result_hash(&a), result_hash(&b));
    assert_ne!(
        snapshot_hash(&a.snapshot),
        snapshot_hash(&simulate(&params(43, 1.0)).unwrap().snapshot)
    );
}

/// The determinism contract: these hashes must be the same on every platform
/// (Linux, macOS, Windows, WASM) for this model version. If a deliberate
/// model change alters them, bump `MODEL_VERSION` and update the values; a
/// schema change (which also changes the serialised snapshot) updates the
/// values only. Last updated for schema 0.9.0 and model 0.9.0.
#[test]
fn snapshot_hashes_are_pinned() {
    assert_eq!(MODEL_VERSION, "0.12.1");
    let got: Vec<String> = PINNED
        .iter()
        .map(|(seed, mass, _)| snapshot_hash(&simulate(&params(*seed, *mass)).unwrap().snapshot))
        .collect();
    let want: Vec<String> = PINNED.iter().map(|(_, _, h)| h.to_string()).collect();
    assert_eq!(got, want);
}

const PINNED: [(u64, f64, &str); 3] = [
    (1, 1.0, "01ee622818311660"),
    (42, 1.0, "a6153511375a4eed"),
    (7, 0.3, "d2a1141a7911d713"),
];

#[test]
fn sun_like_system_is_described_sensibly() {
    let result = simulate(&params(42, 1.0)).unwrap();
    let snap = &result.snapshot;
    assert_eq!(snap.schema_version, SCHEMA_VERSION);
    assert_eq!(snap.model_version, MODEL_VERSION);

    let HierarchyNode::Star(star) = &snap.hierarchy else {
        panic!("expected a single star");
    };
    assert_eq!(star.mass_msun, 1.0);
    // The star comes from the evolution model, so it has aged.
    assert!((5500.0..6100.0).contains(&star.teff_k), "{}", star.teff_k);
    assert!(star.spectral_class.starts_with('G'));
    assert!(snap.age_gyr > 0.0 && snap.age_gyr <= star.main_sequence_lifetime_gyr);
    let hz = &snap.habitable_zones[0];
    assert!(hz.conservative_inner_au < hz.conservative_outer_au);

    // Dole's systems hold 7–12 planets; spread above that is the known
    // deviation of the specification, section 8.
    assert!((6..=16).contains(&snap.planets().count()));
    for pair in snap.planets().collect::<Vec<_>>().windows(2) {
        assert!(pair[0].orbit.a_au < pair[1].orbit.a_au);
    }
    for body in snap.planets() {
        let c = &body.composition;
        let sum = c.metal_frac + c.rock_frac + c.water_frac + c.volatiles_frac + c.h_he_frac;
        assert!((sum - 1.0).abs() < 1e-12);
        if c.h_he_frac >= 0.1 {
            assert!(body.climate.is_none());
            assert!(body.surface_gravity_gees.is_none());
        }
        let in_hz = body.orbit.a_au >= hz.conservative_inner_au
            && body.orbit.a_au <= hz.conservative_outer_au;
        assert_eq!(body.habitability.in_conservative_hz, in_hz);
    }
    assert!(result.diagnostics.bit_deterministic);
    assert!(result.diagnostics.warnings.is_empty());
}

#[test]
fn timeline_is_ordered_and_ends_at_the_final_system() {
    let result = simulate(&params(42, 1.0)).unwrap();
    let timeline = &result.timeline;
    let window = SimulationParams::default().timeline.classic_window_myr;

    assert!(matches!(
        timeline.events.first().unwrap().kind,
        EventKind::DiskFormed { .. }
    ));
    // The dust is gone at the end; moons, which are added to the model's
    // planets, are recorded after it.
    let last_of_formation = timeline
        .events
        .iter()
        .rev()
        .find(|e| !matches!(e.kind, EventKind::MoonFormed { .. }))
        .unwrap();
    assert!(matches!(
        last_of_formation.kind,
        EventKind::DiskDispersed { .. }
    ));
    for pair in timeline.events.windows(2) {
        assert!(pair[0].t_myr <= pair[1].t_myr);
    }
    assert_eq!(timeline.events.last().unwrap().t_myr, window);

    assert_eq!(timeline.keyframes.len(), 32);
    for pair in timeline.keyframes.windows(2) {
        assert!(pair[0].t_myr < pair[1].t_myr);
    }
    // The last keyframe holds exactly the bodies of the snapshot.
    let last = timeline.keyframes.last().unwrap();
    assert_eq!(last.t_myr, window);
    let mut final_ids: Vec<BodyId> = result.snapshot.planets().map(|b| b.id).collect();
    final_ids.sort();
    let keyframe_ids: Vec<BodyId> = last.bodies.iter().map(|b| b.id).collect();
    assert_eq!(keyframe_ids, final_ids);

    // Every collision removes one embryo: injected - collisions = planets.
    let count = |f: fn(&EventKind) -> bool| timeline.events.iter().filter(|e| f(&e.kind)).count();
    let injected = count(|k| matches!(k, EventKind::EmbryoInjected { .. }));
    let collisions = count(|k| matches!(k, EventKind::Collision { .. }));
    assert_eq!(injected - collisions, result.snapshot.planets().count());
}

#[test]
fn events_can_be_switched_off() {
    let mut p = params(42, 1.0);
    p.timeline.record_events = false;
    p.timeline.keyframe_count = 4;
    let result = simulate(&p).unwrap();
    assert!(result.timeline.events.is_empty());
    assert_eq!(result.timeline.keyframes.len(), 4);
}

#[test]
fn atmospheres_toggle_only_removes_gas_lists() {
    let with = simulate(&params(42, 1.0)).unwrap();
    let mut p = params(42, 1.0);
    p.toggles.atmospheres = false;
    let without = simulate(&p).unwrap();
    for (a, b) in with.snapshot.bodies.iter().zip(&without.snapshot.bodies) {
        assert_eq!(a.mass_mearth, b.mass_mearth);
        assert_eq!(a.class, b.class);
        assert!(b
            .atmosphere
            .as_ref()
            .map_or(true, |atm| atm.gases.is_empty()));
    }
}

#[test]
fn age_can_be_fixed() {
    let mut p = params(42, 1.0);
    p.age_gyr = Some(4.6);
    assert_eq!(simulate(&p).unwrap().snapshot.age_gyr, 4.6);
}

#[test]
fn invalid_requests_are_reported() {
    let mut p = params(0, 1.0);
    assert!(matches!(simulate(&p), Err(SimError::InvalidParams(_))));
    p.seed = 1;
    p.formation.classic.gas_dust_ratio = -1.0;
    assert!(matches!(simulate(&p), Err(SimError::InvalidParams(_))));
}

/// Inside Fogg's mass range nothing is said; outside it, the model warns that
/// it is extrapolating. Stars come from the evolution model either way.
#[test]
fn stellar_model_follows_the_request() {
    let inside = simulate(&params(5, 1.2)).unwrap();
    assert_eq!(
        inside.diagnostics.stellar_model,
        StellarModel::HurleyPolsTout2000
    );
    assert!(inside.diagnostics.warnings.is_empty());

    let outside = simulate(&params(5, 3.0)).unwrap();
    assert_eq!(
        outside.diagnostics.stellar_model,
        StellarModel::HurleyPolsTout2000
    );
    let HierarchyNode::Star(star) = &outside.snapshot.hierarchy else {
        panic!("single star expected");
    };
    assert_eq!(star.mass_msun, 3.0);
    // A 3 M☉ star lives about 0.35 Gyr on the main sequence.
    assert!(outside.snapshot.age_gyr < 0.4);
    assert!(outside
        .diagnostics
        .warnings
        .iter()
        .any(|w| w.contains("written for single stars of 0.6–1.3")));
}

/// Below Fogg's range the model is extrapolating badly, and says so.
#[test]
fn red_dwarfs_are_steered_to_the_standard_tier() {
    use sg_sim::{preset_availability, PresetAvailability};
    assert!(matches!(
        preset_availability(PresetId::RedDwarf, Fidelity::Classic),
        PresetAvailability::Unavailable(_)
    ));
    assert_eq!(
        preset_availability(PresetId::RedDwarf, Fidelity::Standard),
        PresetAvailability::Available
    );
    let warnings = simulate(&params(5, 0.3)).unwrap().diagnostics.warnings;
    assert!(warnings.iter().any(|w| w.contains("real red dwarfs")));
    let warnings = simulate(&params(5, 1.0)).unwrap().diagnostics.warnings;
    assert!(warnings.is_empty());
}

#[test]
fn hot_stars_have_no_habitable_zone() {
    // A 3 M☉ star is far above Kopparapu's 7200 K.
    let result = simulate(&params(5, 3.0)).unwrap();
    assert!(result.snapshot.habitable_zones.is_empty());
    assert!(result
        .diagnostics
        .warnings
        .iter()
        .any(|w| w.contains("habitable zone")));
}

struct CancelAt(Stage);

impl SimObserver for CancelAt {
    fn progress(&mut self, stage: Stage, _fraction: f64) -> Control {
        if stage == self.0 {
            Control::Cancel
        } else {
            Control::Continue
        }
    }
}

#[test]
fn observer_can_cancel() {
    for stage in [
        Stage::Stellar,
        Stage::Formation,
        Stage::Planets,
        Stage::Assembly,
    ] {
        let result = simulate_with(&params(42, 1.0), &mut CancelAt(stage));
        assert_eq!(result, Err(SimError::Cancelled));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// JSON round-trips exactly, which also proves every number is finite
    /// (serde_json writes non-finite floats as null, which would not parse
    /// back into an f64).
    #[test]
    fn results_round_trip_through_json(seed in 1u64..100_000, mass in 0.2f64..1.5) {
        let result = simulate(&params(seed, mass)).unwrap();
        let snap_json = serde_json::to_string(&result.snapshot).unwrap();
        let snap: SystemSnapshot = serde_json::from_str(&snap_json).unwrap();
        prop_assert_eq!(&snap, &result.snapshot);
        let time_json = serde_json::to_string(&result.timeline).unwrap();
        let time: EventTimeline = serde_json::from_str(&time_json).unwrap();
        prop_assert_eq!(&time, &result.timeline);
    }

    #[test]
    fn timelines_are_monotonic_and_consistent(seed in 1u64..100_000, mass in 0.2f64..1.5) {
        let result = simulate(&params(seed, mass)).unwrap();
        for pair in result.timeline.events.windows(2) {
            prop_assert!(pair[0].t_myr <= pair[1].t_myr);
        }
        let last = result.timeline.keyframes.last().unwrap();
        prop_assert_eq!(last.bodies.len(), result.snapshot.planets().count());
        for pair in result.snapshot.planets().collect::<Vec<_>>().windows(2) {
            prop_assert!(pair[0].orbit.a_au <= pair[1].orbit.a_au);
        }
    }
}
