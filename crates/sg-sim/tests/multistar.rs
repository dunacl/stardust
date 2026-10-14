//! End-to-end tests of multiple-star systems (M3) through the public API.
#![cfg(feature = "classic")]

use sg_schema::*;
use sg_sim::{preset_params, result_hash, simulate, SimulationResult};

fn preset(preset: PresetId, seed: u64) -> SimulationResult {
    simulate(&preset_params(preset, seed, Fidelity::Classic)).unwrap()
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

fn pair_ids(node: &HierarchyNode, out: &mut Vec<NodeId>) {
    if let HierarchyNode::Pair { id, a, b, .. } = node {
        out.push(*id);
        pair_ids(a, out);
        pair_ids(b, out);
    }
}

/// Every system must be internally consistent, whatever its shape.
fn check_consistency(result: &SimulationResult) {
    let snap = &result.snapshot;
    let star_ids: Vec<StarId> = stars(&snap.hierarchy).iter().map(|s| s.id).collect();
    let mut nodes = Vec::new();
    pair_ids(&snap.hierarchy, &mut nodes);
    let host_exists = |host: &HostRef| match host {
        HostRef::Star(id) => star_ids.contains(id),
        HostRef::Barycenter(id) => nodes.contains(id),
        HostRef::Body(_) => false,
    };

    let mut ids: Vec<BodyId> = snap.bodies.iter().map(|b| b.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), snap.bodies.len(), "body ids are unique");

    for body in snap.planets() {
        assert!(host_exists(&body.host));
        let zone = snap
            .planet_zones
            .iter()
            .find(|z| z.host == body.host)
            .expect("every planet's host has a zone");
        let a = body.orbit.a_au;
        assert!(
            a >= zone.inner_au && a <= zone.outer_au,
            "{}: {a}",
            body.name
        );
        assert!(body.orbit.period_days > 0.0 && body.mass_mearth > 0.0);
    }
    for zone in &snap.planet_zones {
        assert!(host_exists(&zone.host) && zone.inner_au < zone.outer_au);
    }
    for hz in &snap.habitable_zones {
        assert!(host_exists(&hz.host));
        assert!(hz.optimistic_inner_au < hz.conservative_inner_au);
        assert!(hz.conservative_outer_au < hz.optimistic_outer_au);
    }
    for star in stars(&snap.hierarchy) {
        assert!(snap.age_gyr <= star.main_sequence_lifetime_gyr);
        assert_eq!(star.age_gyr, snap.age_gyr);
    }
    // Every body of every keyframe has a host, and the last keyframe holds
    // exactly the finished planets.
    let last = result.timeline.keyframes.last().unwrap();
    let mut final_ids: Vec<BodyId> = last.bodies.iter().map(|b| b.id).collect();
    final_ids.sort();
    let mut planet_ids: Vec<BodyId> = snap.planets().map(|b| b.id).collect();
    planet_ids.sort();
    assert_eq!(final_ids, planet_ids);
    for keyframe in &result.timeline.keyframes {
        assert!(keyframe.bodies.iter().all(|b| host_exists(&b.host)));
    }
    assert!(result
        .timeline
        .events
        .windows(2)
        .all(|w| w[0].t_myr <= w[1].t_myr));
}

/// Kepler-16-like: planets orbit both stars, none inside the stability
/// limit near 0.65 AU, and they are named after the pair.
#[test]
fn circumbinary_preset_forms_planets_around_the_pair() {
    for seed in 1..25 {
        let result = preset(PresetId::Circumbinary, seed);
        check_consistency(&result);
        let snap = &result.snapshot;
        assert_eq!(stars(&snap.hierarchy).len(), 2);
        assert!(!snap.bodies.is_empty());
        for body in snap.planets() {
            assert_eq!(body.host, HostRef::Barycenter(NodeId(0)));
            assert!(body.orbit.a_au > 0.64, "{}", body.orbit.a_au);
        }
        let system = stars(&snap.hierarchy)[0]
            .name
            .trim_end_matches(" A")
            .to_string();
        assert_eq!(snap.bodies[0].name, format!("{system} AB b"));
        // One zone: around the pair. The stars are too close to keep
        // planets of their own.
        assert_eq!(snap.planet_zones.len(), 1);
        assert!(snap.planet_zones[0].limited_inside && !snap.planet_zones[0].limited_outside);
    }
}

/// α Centauri-like: each star has its own planets, none beyond about 3 AU.
#[test]
fn s_type_preset_forms_planets_around_each_star() {
    for seed in 1..25 {
        let result = preset(PresetId::STypeBinary, seed);
        check_consistency(&result);
        let snap = &result.snapshot;
        let around = |host: HostRef| snap.planets().filter(move |b| b.host == host);
        assert!(around(HostRef::Star(StarId(0))).count() > 0);
        assert!(around(HostRef::Star(StarId(1))).count() > 0);
        assert!(snap.planets().all(|b| b.orbit.a_au < 3.0));
        assert_eq!(snap.habitable_zones.len(), 2);
        let first_of_b = around(HostRef::Star(StarId(1))).next().unwrap();
        assert!(first_of_b.name.ends_with(" B b"), "{}", first_of_b.name);
    }
}

#[test]
fn triple_preset_forms_planets_in_nested_zones() {
    let mut circumbinary = 0;
    let mut around_third = 0;
    for seed in 1..25 {
        let result = preset(PresetId::HierarchicalTriple, seed);
        check_consistency(&result);
        let snap = &result.snapshot;
        assert_eq!(stars(&snap.hierarchy).len(), 3);
        for body in snap.planets() {
            match body.host {
                // Around the inner pair: between its own limit and the one
                // set by the third star.
                HostRef::Barycenter(NodeId(1)) => {
                    circumbinary += 1;
                    assert!(body.orbit.a_au > 0.5 && body.orbit.a_au < 15.5);
                }
                HostRef::Star(StarId(2)) => {
                    around_third += 1;
                    assert!(body.orbit.a_au < 7.0);
                }
                other => panic!("unexpected host {other:?}"),
            }
        }
    }
    assert!(circumbinary > 24 && around_third > 24);
}

#[test]
fn random_preset_is_valid_and_repeatable_for_many_seeds() {
    let mut multiples = 0;
    for seed in 1..=300 {
        let result = preset(PresetId::Random, seed);
        check_consistency(&result);
        if stars(&result.snapshot.hierarchy).len() > 1 {
            multiples += 1;
        }
        if seed % 50 == 0 {
            assert_eq!(
                result_hash(&result),
                result_hash(&preset(PresetId::Random, seed))
            );
        }
    }
    assert!((40..140).contains(&multiples), "{multiples}");
}

#[test]
fn sampled_binaries_and_triples_run() {
    for multiplicity in [Multiplicity::Binary, Multiplicity::Triple] {
        for seed in 1..=60 {
            let params = SimulationParams {
                seed,
                system: SystemSpec::Auto {
                    primary_mass_msun: Some(1.0),
                    multiplicity: Some(multiplicity),
                },
                ..SimulationParams::default()
            };
            check_consistency(&simulate(&params).unwrap());
        }
    }
}

/// Pinned so that an accidental change to the multi-star path fails on
/// every platform. Update together with `MODEL_VERSION`.
#[test]
fn multi_star_hashes_are_pinned() {
    let got = [
        result_hash(&preset(PresetId::Circumbinary, 42)),
        result_hash(&preset(PresetId::STypeBinary, 42)),
        result_hash(&preset(PresetId::HierarchicalTriple, 42)),
        result_hash(&preset(PresetId::Random, 42)),
    ];
    assert_eq!(got, PINNED);
}

const PINNED: [&str; 4] = [
    "67f09ce86f1833d8",
    "3b74004e3f53eb00",
    "cf8c829ab0923e29",
    "1d22067c7fd0f96f",
];
