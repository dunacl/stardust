//! Comets, for the Standard and High tiers.
//!
//! `sg_dynamics::comets` gives how many; this draws a few of them to show.
//! Which comets exist is a draw: the model follows no small body. The
//! reservoirs and the counts come from the system; the orbits come from the
//! definitions of the two families, so each is a plausible comet, not one
//! that the model formed.
//!
//! It runs after everything else, on its own stream, and reads the
//! finished system, so no planet, moon or belt depends on it. The Classic
//! tier has none: Dole's method has no belts for comets to come from.
//!
//! Only a system's outermost host gets long-period comets: the tides that
//! build an Oort cloud act on the whole system, and a star with a companion
//! could not hold one to itself.

use crate::names;
use sg_core::rng::{ModuleId, RngStream};
use sg_core::units::{Au, SolarMass};
use sg_dynamics::comets::{
    active_comets, feeds_oort_cloud, is_ejector, mass_between, nucleus_diameter_km,
    oort_mass_mearth, reach_au, tisserand, visible_perihelion_au, LEAST_SHEPHERD_MEARTH,
};
use sg_dynamics::kepler;
use sg_schema::{
    Comet, CometFamily, CometId, CometReservoir, HierarchyNode, HostRef, Orbit, Star,
    SystemSnapshot,
};
use std::collections::BTreeSet;

/// Most Jupiter-family comets drawn for a host.
const MAX_SHOWN: usize = 12;
/// Most long-period comets drawn for a system.
const MAX_LONG_PERIOD: usize = 4;
/// Largest aphelion of the family, as a multiple of the giant's distance:
/// 7.35 AU to Jupiter's 5.2 (Levison 1996).
const MAX_A_OVER_GIANT: f64 = 1.41;
/// Largest inclination drawn to the plane, degrees: the family is flat.
const MAX_FAMILY_INCLINATION_DEG: f64 = 30.0;
/// Attempts at an orbit inside the family before giving up on one comet.
const MAX_TRIES: usize = 200;
/// Semi-major axes of long-period comets drawn, AU.
const LONG_PERIOD_A_AU: (f64, f64) = (2.0e3, 2.0e4);
/// How close to perihelion a long-period comet is placed, years: on the
/// scale of the view it would otherwise be far outside it.
const LONG_PERIOD_WINDOW_YR: f64 = 1.0;

fn stars_of(node: &HierarchyNode, out: &mut Vec<Star>) {
    match node {
        HierarchyNode::Star(star) => out.push(star.clone()),
        HierarchyNode::Pair { a, b, .. } => {
            stars_of(a, out);
            stars_of(b, out);
        }
    }
}

fn find(node: &HierarchyNode, host: HostRef) -> Option<&HierarchyNode> {
    match (node, host) {
        (HierarchyNode::Star(star), HostRef::Star(id)) if star.id == id => Some(node),
        (HierarchyNode::Pair { id, .. }, HostRef::Barycenter(wanted)) if *id == wanted => {
            Some(node)
        }
        (HierarchyNode::Pair { a, b, .. }, _) => find(a, host).or_else(|| find(b, host)),
        _ => None,
    }
}

/// Mass, M☉, and light, L☉, of what a planet of `host` orbits.
fn host_star(snapshot: &SystemSnapshot, host: HostRef) -> Option<(f64, f64)> {
    let mut stars = Vec::new();
    stars_of(find(&snapshot.hierarchy, host)?, &mut stars);
    Some((
        stars.iter().map(|s| s.mass_msun).sum(),
        stars.iter().map(|s| s.luminosity_lsun).sum(),
    ))
}

fn is_root(snapshot: &SystemSnapshot, host: HostRef) -> bool {
    match (&snapshot.hierarchy, host) {
        (HierarchyNode::Star(star), HostRef::Star(id)) => star.id == id,
        (HierarchyNode::Pair { id, .. }, HostRef::Barycenter(wanted)) => *id == wanted,
        _ => false,
    }
}

/// One Jupiter-family orbit about a giant at `giant_au`, or `None` if the
/// tries run out. Visible means a perihelion inside `visible_au`.
fn family_orbit(
    rng: &mut RngStream,
    giant_au: f64,
    visible_au: f64,
    star_mass_msun: f64,
) -> Option<Orbit> {
    let q_most = visible_au.min(0.9 * giant_au);
    for _ in 0..MAX_TRIES {
        let q = q_most * rng.uniform_range(0.2, 1.0);
        let a = rng.uniform_range(q * 1.2, MAX_A_OVER_GIANT * giant_au);
        let e = 1.0 - q / a;
        let i_deg = MAX_FAMILY_INCLINATION_DEG * rng.uniform() * rng.uniform();
        let t = tisserand(a, e, i_deg.to_radians(), giant_au);
        if a > q && (2.0..3.0).contains(&t) {
            return Some(orbit(rng, a, e, i_deg, rng_angle, star_mass_msun));
        }
    }
    None
}

fn rng_angle(rng: &mut RngStream) -> f64 {
    rng.uniform_range(0.0, 360.0)
}

fn orbit(
    rng: &mut RngStream,
    a_au: f64,
    e: f64,
    i_deg: f64,
    mean_anomaly: fn(&mut RngStream) -> f64,
    star_mass_msun: f64,
) -> Orbit {
    Orbit {
        a_au,
        e,
        i_deg,
        long_asc_node_deg: rng_angle(rng),
        arg_periapsis_deg: rng_angle(rng),
        mean_anomaly_deg: mean_anomaly(rng),
        period_days: kepler::orbital_period(Au(a_au), SolarMass(star_mass_msun)).0,
    }
}

/// One long-period orbit: any tilt, long and thin, near its perihelion now.
fn long_period_orbit(rng: &mut RngStream, visible_au: f64, star_mass_msun: f64) -> Orbit {
    let a_au = rng.uniform_range(LONG_PERIOD_A_AU.0, LONG_PERIOD_A_AU.1);
    let q = visible_au * rng.uniform_range(0.1, 1.0);
    // A uniform cosine: the directions Oort-cloud comets arrive from are
    // near isotropic (Oort 1950).
    let i_deg = sg_core::math::acos(rng.uniform_range(-1.0, 1.0)).to_degrees();
    let mut drawn = orbit(rng, a_au, 1.0 - q / a_au, i_deg, |_| 0.0, star_mass_msun);
    let window = LONG_PERIOD_WINDOW_YR * 365.25 / drawn.period_days * 360.0;
    let m = rng.uniform_range(-window, window);
    drawn.mean_anomaly_deg = if m < 0.0 { m + 360.0 } else { m };
    drawn
}

/// What the system's belts and planets give one host.
fn reservoir(snapshot: &SystemSnapshot, host: HostRef) -> Option<CometReservoir> {
    let (star_mass, light) = host_star(snapshot, host)?;
    let ice_line = snapshot
        .disks_at_dispersal
        .iter()
        .find(|disk| disk.host == host)
        .map(|disk| disk.final_ice_line_au)?;
    // The last planet that matters, and how far it reaches.
    let shepherd = snapshot
        .planets()
        .filter(|p| p.host == host && p.mass_mearth >= LEAST_SHEPHERD_MEARTH)
        .map(|p| p.orbit.a_au)
        .fold(f64::NEG_INFINITY, f64::max);
    if !shepherd.is_finite() {
        return None;
    }
    let segments: Vec<(f64, f64, f64)> = snapshot
        .belts
        .iter()
        .filter(|belt| belt.host == host)
        .flat_map(|belt| &belt.segments)
        .filter(|segment| segment.inner_au >= ice_line)
        .map(|segment| (segment.inner_au, segment.outer_au, segment.mass_mearth))
        .collect();
    let mass = mass_between(&segments, shepherd, reach_au(shepherd));
    if mass <= 0.0 {
        return None;
    }
    let visible = visible_perihelion_au(light);
    let ejectors = snapshot
        .planets()
        .filter(|p| p.host == host && p.orbit.a_au > visible && p.orbit.a_au <= shepherd)
        .filter(|p| is_ejector(p.mass_mearth, p.orbit.a_au, star_mass))
        .count() as u32;
    let feeds = snapshot.planets().any(|p| {
        p.host == host && feeds_oort_cloud(p.mass_mearth, p.orbit.a_au, star_mass, snapshot.age_gyr)
    });
    Some(CometReservoir {
        host,
        belt_mass_mearth: mass,
        ejectors,
        active_comets: active_comets(mass, ejectors),
        oort_mass_mearth: (feeds && is_root(snapshot, host)).then(|| oort_mass_mearth(mass)),
    })
}

/// The distance of a host's heaviest planet, which shapes the family.
fn giant_of(snapshot: &SystemSnapshot, host: HostRef) -> Option<f64> {
    snapshot
        .planets()
        .filter(|p| p.host == host)
        .max_by(|a, b| a.mass_mearth.total_cmp(&b.mass_mearth))
        .map(|p| p.orbit.a_au)
}

/// How many of the family to show: a logarithm, four for the Solar System's
/// 300 and two more for each tenfold, because counts span orders of
/// magnitude. A choice for the picture.
fn family_shown(active: f64) -> usize {
    if active < 1.0 {
        return 0;
    }
    ((2.0 * sg_core::math::log10(active) - 1.0).round().max(1.0) as usize).min(MAX_SHOWN)
}

/// How many long-period comets to show: one more for each tenfold of Oort
/// mass above about 0.1 M⊕. A choice for the picture.
fn long_period_shown(oort_mearth: f64) -> usize {
    ((sg_core::math::log10(oort_mearth) + 2.0).round().max(1.0) as usize).min(MAX_LONG_PERIOD)
}

/// Adds the system's comets. Runs after names, so that its names are new.
pub fn assign(snapshot: &mut SystemSnapshot, seed: u64) {
    let mut rng = RngStream::new(seed, ModuleId::COMETS);
    let hosts: BTreeSet<HostRef> = snapshot.belts.iter().map(|b| b.host).collect();
    let mut taken: BTreeSet<String> = snapshot.bodies.iter().map(|b| b.name.clone()).collect();
    let mut comets = Vec::new();
    let mut reservoirs = Vec::new();
    for host in hosts {
        let Some(found) = reservoir(snapshot, host) else {
            continue;
        };
        let Some((star_mass, light)) = host_star(snapshot, host) else {
            continue;
        };
        let visible = visible_perihelion_au(light);
        let shown = family_shown(found.active_comets);
        if let Some(giant_au) = giant_of(snapshot, host) {
            for _ in 0..shown {
                if let Some(orbit) = family_orbit(&mut rng, giant_au, visible, star_mass) {
                    comets.push((host, CometFamily::JupiterFamily, orbit));
                }
            }
        }
        if let Some(oort) = found.oort_mass_mearth {
            for _ in 0..long_period_shown(oort) {
                let orbit = long_period_orbit(&mut rng, visible, star_mass);
                comets.push((host, CometFamily::LongPeriod, orbit));
            }
        }
        reservoirs.push(found);
    }
    let mut numbered = 0;
    snapshot.comets = comets
        .into_iter()
        .enumerate()
        .map(|(index, (host, family, orbit))| {
            let name = names::unique(&mut rng, &mut taken);
            let designation = match family {
                CometFamily::JupiterFamily => {
                    numbered += 1;
                    format!("{numbered}P/{name}")
                }
                CometFamily::LongPeriod => format!("C/{name}"),
            };
            Comet {
                id: CometId(index as u32),
                name,
                designation,
                host,
                family,
                orbit,
                diameter_km: nucleus_diameter_km(1.0 - rng.uniform()),
            }
        })
        .collect();
    snapshot.comet_reservoirs = reservoirs;
}

#[cfg(test)]
mod tests {
    use super::*;
    use sg_schema::{Fidelity, SimulationParams};

    fn run(seed: u64, fidelity: Fidelity) -> SystemSnapshot {
        crate::simulate(&SimulationParams {
            seed,
            fidelity,
            ..SimulationParams::default()
        })
        .unwrap()
        .snapshot
    }

    #[test]
    fn the_number_shown_follows_the_order_of_magnitude() {
        assert_eq!(family_shown(0.0), 0);
        assert_eq!(family_shown(300.0), 4);
        assert_eq!(family_shown(1.0e5), 9);
        assert_eq!(family_shown(1.0e12), MAX_SHOWN);
        assert_eq!(long_period_shown(0.01), 1);
        assert_eq!(long_period_shown(10.0), 3);
        assert_eq!(long_period_shown(1.0e4), MAX_LONG_PERIOD);
    }

    #[test]
    fn the_family_stays_in_its_tisserand_range() {
        let mut rng = RngStream::new(1, ModuleId::COMETS);
        for _ in 0..500 {
            let orbit = family_orbit(&mut rng, 5.2, 2.5, 1.0).expect("an orbit");
            let t = tisserand(orbit.a_au, orbit.e, orbit.i_deg.to_radians(), 5.2);
            assert!((2.0..3.0).contains(&t), "{t}");
            assert!(orbit.a_au * (1.0 - orbit.e) <= 2.5);
            assert!(orbit.a_au <= 1.41 * 5.2);
        }
    }

    #[test]
    fn long_period_comets_arrive_from_every_side_near_perihelion() {
        let mut rng = RngStream::new(2, ModuleId::COMETS);
        let orbits: Vec<_> = (0..400)
            .map(|_| long_period_orbit(&mut rng, 2.5, 1.0))
            .collect();
        let retrograde = orbits.iter().filter(|o| o.i_deg > 90.0).count();
        assert!((140..260).contains(&retrograde), "{retrograde}");
        for orbit in &orbits {
            assert!(orbit.e < 1.0 && orbit.a_au * (1.0 - orbit.e) <= 2.5);
            assert!(orbit.mean_anomaly_deg.is_finite());
        }
    }

    #[test]
    fn modern_systems_have_comets() {
        let any = (1..=20).any(|seed| !run(seed, Fidelity::Standard).comets.is_empty());
        assert!(any, "no comets in twenty systems");
    }

    #[cfg(feature = "classic")]
    #[test]
    fn the_classic_tier_has_none() {
        for seed in 1..=5 {
            let snapshot = run(seed, Fidelity::Classic);
            assert!(snapshot.comets.is_empty() && snapshot.comet_reservoirs.is_empty());
        }
    }

    #[test]
    fn comets_have_names_of_their_own_and_stay_in_range() {
        for seed in 1..=20 {
            let snapshot = run(seed, Fidelity::Standard);
            let mut names: BTreeSet<&str> =
                snapshot.bodies.iter().map(|b| b.name.as_str()).collect();
            for comet in &snapshot.comets {
                assert!(names.insert(&comet.name), "{}", comet.name);
                assert!(comet.diameter_km >= 2.3 && comet.diameter_km <= 60.0);
                assert!(comet.orbit.e >= 0.0 && comet.orbit.e < 1.0);
                assert!(comet.orbit.period_days.is_finite());
            }
        }
    }
}
