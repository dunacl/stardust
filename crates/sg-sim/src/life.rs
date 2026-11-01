//! A finished system seen at another moment of its star's life.
//!
//! Running the simulation at another age gives another system: age feeds
//! the late stage, and a different count of planets comes out. So this does
//! not run it. It keeps every body as it formed and changes only what
//! follows directly from the star: the star itself, along the same tracks
//! (`sg_stellar::evolution`), its habitable zone, the light its bodies
//! receive, and the surface minerals estimated from that.
//!
//! Left as they were: orbits, masses, atmospheres, climates, classes, belts
//! and comets. Those depend on the system's history, which this does not
//! rerun, so a scrubbed system is a view and not a simulation at that age.

use crate::SimError;
use sg_core::math::pow;
use sg_core::units::{Kelvin, SolarMass};
use sg_schema::{HierarchyNode, HostRef, Star, SystemSnapshot};
use sg_stellar::evolution::MainSequence;

/// Share of a star's main-sequence life the model goes to (`stars.rs`).
const LIFE_SHARE: f64 = 0.95;
/// Youngest age shown, Gyr: the formation stage ends about here.
pub const YOUNGEST_GYR: f64 = 0.01;

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

fn stars_mut(node: &mut HierarchyNode, visit: &mut dyn FnMut(&mut Star)) {
    match node {
        HierarchyNode::Star(star) => visit(star),
        HierarchyNode::Pair { a, b, .. } => {
            stars_mut(a, visit);
            stars_mut(b, visit);
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

fn light_of(hierarchy: &HierarchyNode, host: HostRef) -> Option<f64> {
    Some(
        stars(find(hierarchy, host)?)
            .iter()
            .map(|s| s.luminosity_lsun)
            .sum(),
    )
}

/// Oldest age the system can be shown at, Gyr: most of the shortest-lived
/// star's main sequence. Evolved stars are not modelled.
pub fn oldest_gyr(snapshot: &SystemSnapshot) -> f64 {
    stars(&snapshot.hierarchy)
        .iter()
        .map(|star| star.main_sequence_lifetime_gyr)
        .fold(f64::INFINITY, f64::min)
        * LIFE_SHARE
}

/// The system with its star at `age_gyr`, kept within what the model covers.
/// See the module's notes for what changes. A planet's equilibrium
/// temperature goes as `L^(1/4)` at a fixed distance (e.g. Seager 2010,
/// *Exoplanet Atmospheres*, eq. 3.9), valid while orbits stay as they are.
pub fn at_age(snapshot: &SystemSnapshot, age_gyr: f64) -> Result<SystemSnapshot, SimError> {
    let age_gyr = age_gyr.clamp(YOUNGEST_GYR, oldest_gyr(snapshot).max(YOUNGEST_GYR));
    let mut aged = snapshot.clone();
    let z = stars(&snapshot.hierarchy)
        .first()
        .map_or(0.02, |star| star.metal_frac);
    let tracks = MainSequence::new(z).map_err(|err| SimError::InvalidParams(err.to_string()))?;
    let mut failure = None;
    stars_mut(&mut aged.hierarchy, &mut |star| match tracks
        .state(SolarMass(star.mass_msun), age_gyr * 1000.0)
    {
        Ok(state) => {
            star.luminosity_lsun = state.luminosity.0;
            star.radius_rsun = state.radius.0;
            star.teff_k = state.teff.0;
            star.spectral_class = sg_stellar::spectral_class(state.teff).to_string();
            star.color_linear_rgb = sg_stellar::blackbody_linear_srgb(state.teff);
            star.age_gyr = age_gyr;
        }
        Err(err) => failure = Some(err.to_string()),
    });
    if let Some(reason) = failure {
        return Err(SimError::InvalidParams(reason));
    }
    aged.age_gyr = age_gyr;

    let host_of = |body: &sg_schema::Body| match body.host {
        HostRef::Body(planet) => snapshot
            .bodies
            .iter()
            .find(|other| other.id == planet)
            .map_or(body.host, |other| other.host),
        host => host,
    };
    let scaled: Vec<f64> = snapshot
        .bodies
        .iter()
        .map(|body| {
            let host = host_of(body);
            match (
                light_of(&snapshot.hierarchy, host),
                light_of(&aged.hierarchy, host),
            ) {
                (Some(before), Some(now)) if before > 0.0 => {
                    body.equilibrium_temp_k * pow(now / before, 0.25)
                }
                _ => body.equilibrium_temp_k,
            }
        })
        .collect();
    for (body, temp) in aged.bodies.iter_mut().zip(scaled) {
        body.equilibrium_temp_k = temp;
    }
    crate::assign_minerals(&mut aged);

    for zone in &mut aged.habitable_zones {
        let Some(node) = find(&aged.hierarchy, zone.host) else {
            continue;
        };
        let lights: Vec<_> = stars(node)
            .iter()
            .map(|s| {
                (
                    sg_core::units::SolarLuminosity(s.luminosity_lsun),
                    Kelvin(s.teff_k),
                )
            })
            .collect();
        if let Ok(hz) = sg_stellar::combined_habitable_zone(&lights) {
            zone.optimistic_inner_au = hz.recent_venus.0;
            zone.conservative_inner_au = hz.runaway_greenhouse.0;
            zone.conservative_outer_au = hz.maximum_greenhouse.0;
            zone.optimistic_outer_au = hz.early_mars.0;
        }
    }
    Ok(aged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sg_schema::{Fidelity, SimulationParams};

    fn system() -> SystemSnapshot {
        crate::simulate(&SimulationParams {
            seed: 42,
            fidelity: Fidelity::Standard,
            ..SimulationParams::default()
        })
        .unwrap()
        .snapshot
    }

    fn sun(snapshot: &SystemSnapshot) -> &Star {
        stars(&snapshot.hierarchy)[0]
    }

    #[test]
    fn the_bodies_stay_and_only_their_light_changes() {
        let snapshot = system();
        let aged = at_age(&snapshot, 1.0).unwrap();
        assert_eq!(aged.bodies.len(), snapshot.bodies.len());
        assert_eq!(aged.comets, snapshot.comets);
        assert_eq!(aged.belts, snapshot.belts);
        for (now, then) in aged.bodies.iter().zip(&snapshot.bodies) {
            assert_eq!(now.orbit, then.orbit);
            assert_eq!(now.mass_mearth, then.mass_mearth);
        }
    }

    /// The Sun has brightened by about 30% since it began (Gough 1981).
    #[test]
    fn a_sun_like_star_brightens_with_age() {
        let snapshot = system();
        let young = at_age(&snapshot, 0.1).unwrap();
        let old = at_age(&snapshot, 8.0).unwrap();
        assert!(sun(&old).luminosity_lsun > 1.5 * sun(&young).luminosity_lsun);
        let planet = |s: &SystemSnapshot| s.bodies[0].equilibrium_temp_k;
        let expected = pow(
            sun(&old).luminosity_lsun / sun(&young).luminosity_lsun,
            0.25,
        );
        assert!((planet(&old) / planet(&young) - expected).abs() < 1e-9);
        assert!(
            old.habitable_zones[0].conservative_inner_au
                > young.habitable_zones[0].conservative_inner_au
        );
    }

    #[test]
    fn ages_stay_within_what_the_model_covers() {
        let snapshot = system();
        let limit = oldest_gyr(&snapshot);
        assert_eq!(at_age(&snapshot, 500.0).unwrap().age_gyr, limit);
        assert_eq!(at_age(&snapshot, 0.0).unwrap().age_gyr, YOUNGEST_GYR);
    }

    #[test]
    fn a_trip_there_and_back_returns_the_same_star() {
        let snapshot = system();
        let age = snapshot.age_gyr;
        let back = at_age(&at_age(&snapshot, 2.0).unwrap(), age).unwrap();
        let ratio = sun(&back).luminosity_lsun / sun(&snapshot).luminosity_lsun;
        assert!((ratio - 1.0).abs() < 1e-9);
        let temp = back.bodies[0].equilibrium_temp_k / snapshot.bodies[0].equilibrium_temp_k;
        assert!((temp - 1.0).abs() < 1e-9);
    }
}
