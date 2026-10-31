//! How a finished system's planets stand for the long term (PLAN.md
//! §5.7): two analytic checks, made on the snapshot and stated as what
//! they are. Neither is an integration over the system's life, which no
//! tier can afford (PLAN.md D50).
//!
//! - **Spacing**: the closest pair of neighbours, in mutual Hill radii.
//!   Three or more planets closer than about 12 do not last for billions
//!   of years (Pu & Wu 2015); a lone pair needs 3.5 (Gladman 1993).
//! - **AMD stability** (Laskar & Petit 2017): whether the system's
//!   eccentricities and inclinations, however they are shared out in
//!   future, could ever make two neighbours' orbits cross.

use sg_dynamics::late::{self, Orbiter};
use sg_dynamics::stability::{self, PlanetOrbit};
use sg_schema::{HierarchyNode, HostRef, SystemSnapshot};

const EARTHS_PER_SUN: f64 = 332_946.0;

/// The long-term standing of the planets of one host.
#[derive(Debug, Clone, PartialEq)]
pub struct HostStability {
    pub host: HostRef,
    pub planets: usize,
    /// Smallest separation of neighbours, in mutual Hill radii; `None`
    /// with fewer than two planets.
    pub closest_spacing_hill: Option<f64>,
    /// The spacing below which this many planets do not last.
    pub spacing_needed_hill: f64,
    /// Largest AMD coefficient among neighbouring pairs: below 1, no slow
    /// exchange of angular momentum can make any pair's orbits cross.
    pub amd_coefficient: Option<f64>,
}

impl HostStability {
    /// Wide enough apart to last, by the spacing rule.
    pub fn well_spaced(&self) -> bool {
        self.closest_spacing_hill
            .map_or(true, |spacing| spacing >= self.spacing_needed_hill)
    }

    /// AMD-stable: `None` with fewer than two planets.
    pub fn amd_stable(&self) -> Option<bool> {
        self.amd_coefficient.map(|beta| beta < 1.0)
    }

    /// One sentence for a person.
    pub fn describe(&self) -> String {
        let (Some(spacing), Some(beta)) = (self.closest_spacing_hill, self.amd_coefficient) else {
            return "too few planets for neighbours to disturb one another".to_string();
        };
        let spaced = if self.well_spaced() {
            format!(
                "closest neighbours {spacing:.0} Hill radii apart ({:.0} needed to last)",
                self.spacing_needed_hill
            )
        } else {
            format!(
                "closest neighbours only {spacing:.1} Hill radii apart ({:.0} needed to last)",
                self.spacing_needed_hill
            )
        };
        let amd = if beta < 1.0 {
            "no slow exchange of angular momentum can ever make orbits cross (AMD-stable)"
        } else {
            "slow exchanges of angular momentum are not ruled out from making orbits cross \
             over billions of years (not AMD-stable; nor are the Solar System's inner planets)"
        };
        format!("{spaced}; {amd}")
    }
}

fn node_mass(node: &HierarchyNode) -> f64 {
    match node {
        HierarchyNode::Star(star) => star.mass_msun,
        HierarchyNode::Pair { a, b, .. } => node_mass(a) + node_mass(b),
    }
}

fn host_mass(node: &HierarchyNode, host: HostRef) -> Option<f64> {
    match (node, host) {
        (HierarchyNode::Star(star), HostRef::Star(id)) if star.id == id => Some(star.mass_msun),
        (HierarchyNode::Pair { id, .. }, HostRef::Barycenter(wanted)) if *id == wanted => {
            Some(node_mass(node))
        }
        (HierarchyNode::Pair { a, b, .. }, _) => host_mass(a, host).or_else(|| host_mass(b, host)),
        _ => None,
    }
}

/// The standing of every host's planets, in the order of the snapshot's
/// planet zones. Moons are not considered.
pub fn long_term_stability(snapshot: &SystemSnapshot) -> Vec<HostStability> {
    snapshot
        .planet_zones
        .iter()
        .filter_map(|zone| {
            let star = host_mass(&snapshot.hierarchy, zone.host)?;
            let mut planets: Vec<_> = snapshot.planets().filter(|b| b.host == zone.host).collect();
            planets.sort_by(|a, b| a.orbit.a_au.total_cmp(&b.orbit.a_au));
            let orbiters: Vec<Orbiter> = planets
                .iter()
                .map(|b| Orbiter {
                    id: b.id.0,
                    a_au: b.orbit.a_au,
                    mass_msun: b.mass_mearth / EARTHS_PER_SUN,
                })
                .collect();
            let closest = orbiters
                .windows(2)
                .map(|pair| late::separation_in_hill_radii(&pair[0], &pair[1], star))
                .min_by(f64::total_cmp);
            let orbits: Vec<PlanetOrbit> = planets
                .iter()
                .map(|b| PlanetOrbit {
                    mass_msun: b.mass_mearth / EARTHS_PER_SUN,
                    a_au: b.orbit.a_au,
                    e: b.orbit.e.clamp(0.0, 0.999),
                    i_rad: b.orbit.i_deg.to_radians(),
                })
                .collect();
            Some(HostStability {
                host: zone.host,
                planets: planets.len(),
                closest_spacing_hill: closest,
                spacing_needed_hill: if planets.len() > 2 {
                    late::MULTI_STABILITY_HILL
                } else {
                    late::PAIR_STABILITY_HILL
                },
                amd_coefficient: stability::amd_coefficients(&orbits)
                    .into_iter()
                    .max_by(f64::total_cmp),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulate;
    use sg_schema::{Fidelity, SimulationParams};

    fn run(fidelity: Fidelity, seed: u64) -> SystemSnapshot {
        simulate(&SimulationParams {
            seed,
            fidelity,
            ..SimulationParams::default()
        })
        .expect("run")
        .snapshot
    }

    #[test]
    fn standard_systems_are_left_well_spaced_by_their_own_rule() {
        // The Standard tier merges neighbours closer than the limit, so
        // what it leaves must pass the same test.
        for seed in 1..25 {
            let snapshot = run(Fidelity::Standard, seed);
            for host in long_term_stability(&snapshot) {
                assert!(host.well_spaced(), "seed {seed}: {}", host.describe());
                assert_eq!(
                    host.planets,
                    snapshot.planets().filter(|b| b.host == host.host).count()
                );
                assert!(host.amd_coefficient.map_or(true, |beta| beta >= 0.0));
                assert!(!host.describe().is_empty());
            }
        }
    }

    #[test]
    fn scattered_giants_are_reported_as_not_settled() {
        // High tier, seed 412: giants left on crossing, elongated orbits.
        let snapshot = run(Fidelity::High, 412);
        let host = &long_term_stability(&snapshot)[0];
        assert_eq!(host.amd_stable(), Some(false));
        assert!(host.describe().contains("not AMD-stable"));
    }

    #[test]
    fn a_lone_planet_has_nothing_to_report() {
        let mut snapshot = run(Fidelity::Standard, 3);
        let keep = snapshot.planets().next().map(|b| b.id);
        snapshot.bodies.retain(|b| Some(b.id) == keep);
        let host = &long_term_stability(&snapshot)[0];
        assert_eq!(host.planets, 1);
        assert_eq!(host.closest_spacing_hill, None);
        assert_eq!(host.amd_stable(), None);
        assert!(host.well_spaced());
        assert!(host.describe().starts_with("too few planets"));
    }
}
