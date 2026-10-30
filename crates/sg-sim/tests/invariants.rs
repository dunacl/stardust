//! What every tier must satisfy, for every preset it offers. Rules particular
//! to one tier are in that tier's own tests; these are the promises the
//! renderer, the UI and the schema rely on, whichever model made the system.

use sg_schema::*;

use sg_sim::{
    preset_availability, preset_params, simulate, snapshot_hash, PresetAvailability,
    SimulationResult, ALL_PRESETS,
};

fn tiers() -> Vec<Fidelity> {
    let mut tiers = vec![Fidelity::Standard, Fidelity::High];
    if cfg!(feature = "classic") {
        tiers.insert(0, Fidelity::Classic);
    }
    tiers
}

fn seeds(fidelity: Fidelity) -> &'static [u64] {
    match fidelity {
        Fidelity::High => &[1],
        _ => &[1, 2, 3, 42, 345],
    }
}

fn stars<'a>(node: &'a HierarchyNode, out: &mut Vec<&'a Star>) {
    match node {
        HierarchyNode::Star(s) => out.push(s),
        HierarchyNode::Pair { a, b, .. } => {
            stars(a, out);
            stars(b, out);
        }
    }
}

fn nodes(node: &HierarchyNode, out: &mut Vec<NodeId>) {
    if let HierarchyNode::Pair { id, a, b, .. } = node {
        out.push(*id);
        nodes(a, out);
        nodes(b, out);
    }
}

fn check(label: &str, result: &SimulationResult) {
    let snap = &result.snapshot;
    let finite = |name: &str, v: f64| assert!(v.is_finite(), "{label}: {name} = {v}");
    let mut all_stars = Vec::new();
    stars(&snap.hierarchy, &mut all_stars);
    assert!(!all_stars.is_empty(), "{label}: no star");
    let mut star_ids: Vec<StarId> = all_stars.iter().map(|s| s.id).collect();
    star_ids.sort();
    star_ids.dedup();
    assert_eq!(star_ids.len(), all_stars.len(), "{label}: star ids repeat");
    let mut node_ids = Vec::new();
    nodes(&snap.hierarchy, &mut node_ids);
    finite("age", snap.age_gyr);
    assert!(snap.age_gyr > 0.0, "{label}: age");

    for s in &all_stars {
        for (name, v) in [
            ("star mass", s.mass_msun),
            ("luminosity", s.luminosity_lsun),
            ("star radius", s.radius_rsun),
            ("teff", s.teff_k),
        ] {
            assert!(v.is_finite() && v > 0.0, "{label}: {name} = {v}");
        }
        assert!(!s.name.is_empty(), "{label}: unnamed star");
    }

    let mut ids: Vec<BodyId> = snap.bodies.iter().map(|b| b.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), snap.bodies.len(), "{label}: body ids repeat");
    let mut seen_moon = false;
    for b in &snap.bodies {
        let who = format!("{label}: {}", b.name);
        assert!(!b.name.is_empty(), "{label}: unnamed body");
        for (name, v) in [
            ("a", b.orbit.a_au),
            ("period", b.orbit.period_days),
            ("mass", b.mass_mearth),
            ("radius", b.radius_km),
            ("density", b.density_g_cm3),
            ("escape", b.escape_velocity_km_s),
        ] {
            assert!(v.is_finite() && v > 0.0, "{who}: {name} = {v}");
        }
        finite("albedo", b.albedo);
        finite("temperature", b.equilibrium_temp_k);
        assert!(
            (0.0..=1.0).contains(&b.albedo),
            "{who}: albedo {}",
            b.albedo
        );
        assert!(b.equilibrium_temp_k >= 0.0, "{who}: temperature");
        assert!((0.0..1.0).contains(&b.orbit.e), "{who}: e {}", b.orbit.e);
        for (name, v) in [
            ("inclination", b.orbit.i_deg),
            ("node", b.orbit.long_asc_node_deg),
            ("periapsis", b.orbit.arg_periapsis_deg),
            ("anomaly", b.orbit.mean_anomaly_deg),
        ] {
            finite(name, v);
        }
        let c = &b.composition;
        let sum = c.metal_frac + c.rock_frac + c.water_frac + c.volatiles_frac + c.h_he_frac;
        assert!((sum - 1.0).abs() < 1e-6, "{who}: composition sums to {sum}");
        match b.host {
            HostRef::Star(id) => assert!(star_ids.contains(&id), "{who}: host star"),
            HostRef::Barycenter(id) => assert!(node_ids.contains(&id), "{who}: host pair"),
            HostRef::Body(id) => assert!(ids.contains(&id), "{who}: host body"),
        }
        // Moons come after all the planets.
        let moon = matches!(b.kind, BodyKind::Moon);
        assert!(moon || !seen_moon, "{who}: planet after a moon");
        seen_moon |= moon;
    }
    for pair in snap.planets().collect::<Vec<_>>().windows(2) {
        if pair[0].host == pair[1].host {
            assert!(pair[0].orbit.a_au <= pair[1].orbit.a_au, "{label}: order");
        }
    }

    let t = &result.timeline;
    assert!(
        t.events.windows(2).all(|w| w[0].t_myr <= w[1].t_myr),
        "{label}: events out of order"
    );
    assert!(
        t.keyframes.windows(2).all(|w| w[0].t_myr < w[1].t_myr),
        "{label}: keyframes out of order"
    );
    let last = t.keyframes.last().expect("a timeline ends in a keyframe");
    let mut final_ids: Vec<BodyId> = last.bodies.iter().map(|b| b.id).collect();
    final_ids.sort();
    let mut planet_ids: Vec<BodyId> = snap.planets().map(|b| b.id).collect();
    planet_ids.sort();
    assert_eq!(final_ids, planet_ids, "{label}: timeline ends elsewhere");
}

/// The whole matrix, each case run twice so a difference between runs fails.
#[test]
fn every_tier_and_preset_keeps_its_promises() {
    let mut cases = 0;
    for fidelity in tiers() {
        for preset in ALL_PRESETS {
            if preset_availability(preset, fidelity) != PresetAvailability::Available {
                continue;
            }
            for &seed in seeds(fidelity) {
                let label = format!("{fidelity:?} {preset:?} seed {seed}");
                let params = preset_params(preset, seed, fidelity);
                let result = simulate(&params).unwrap_or_else(|e| panic!("{label}: {e}"));
                check(&label, &result);
                let again = simulate(&params).unwrap();
                assert_eq!(
                    snapshot_hash(&result.snapshot),
                    snapshot_hash(&again.snapshot),
                    "{label}: not repeatable"
                );
                cases += 1;
            }
        }
    }
    assert!(cases > 40, "only {cases} cases ran");
}

/// Different seeds give different systems in every tier.
#[test]
fn seeds_matter_in_every_tier() {
    for fidelity in tiers() {
        let hash = |seed| {
            let params = preset_params(PresetId::SunLike, seed, fidelity);
            snapshot_hash(&simulate(&params).unwrap().snapshot)
        };
        assert_ne!(hash(1), hash(2), "{fidelity:?}");
    }
}

/// The checks themselves work: a damaged system is refused.
#[test]
#[should_panic(expected = "mass = NaN")]
fn a_damaged_system_is_refused() {
    let params = preset_params(PresetId::SunLike, 1, Fidelity::Standard);
    let mut result = simulate(&params).unwrap();
    result.snapshot.bodies[0].mass_mearth = f64::NAN;
    check("damaged", &result);
}
