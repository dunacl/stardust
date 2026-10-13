//! Where the stars and the centres of their orbits are (PLAN.md §8.2).
//!
//! Pure functions over the snapshot's stellar hierarchy; no Bevy types, so
//! this is unit-tested directly.
//!
//! A pair's two members move about their common centre of mass, which stays
//! where the pair's own orbit puts it. Positions are worked out from the top
//! of the hierarchy down, and the scale mode's distance mapping is applied
//! to each step separately: a companion's offset from its centre of mass,
//! then a planet's offset from its host. In the linear modes this changes
//! nothing; in the schematic mode it keeps every orbit a recognisable
//! ellipse around its own centre, at the price of distances between
//! different centres no longer being comparable.
//!
//! Positions are double precision (PLAN.md §8.1): single precision cannot
//! place a planet at 30 AU to better than a few hundred kilometres. The
//! scene subtracts the camera's origin before converting ([`crate::camera`]).

use crate::scale::ScaleBlend;
use bevy::math::DVec3;
use sg_core::units::Au;
use sg_dynamics::kepler::{self, Elements};
use sg_schema::{HierarchyNode, HostRef, NodeId, Orbit, Star, StarId, SystemSnapshot};
use std::collections::BTreeMap;
use std::f64::consts::TAU;

/// Simulation frame (z up, AU) to scene frame (y up).
pub fn to_scene(p: [f64; 3]) -> DVec3 {
    DVec3::new(p[0], p[2], -p[1])
}

/// Applies the scale mode's distance mapping to an offset in AU from the
/// centre it is measured from.
pub fn map_offset(scale: &ScaleBlend, p: [f64; 3]) -> DVec3 {
    let v = to_scene(p);
    let r = v.length();
    if r <= f64::EPSILON {
        return DVec3::ZERO;
    }
    v * (scale.distance_f64(r) / r)
}

/// Orbital elements for an orbit of the snapshot at a time on the orbital
/// clock.
pub fn elements(orbit: &Orbit, orbit_days: f64) -> Elements {
    Elements {
        a: Au(orbit.a_au),
        e: orbit.e,
        i: orbit.i_deg.to_radians(),
        long_asc_node: orbit.long_asc_node_deg.to_radians(),
        arg_periapsis: orbit.arg_periapsis_deg.to_radians(),
        mean_anomaly: orbit.mean_anomaly_deg.to_radians() + TAU * (orbit_days / orbit.period_days),
    }
}

/// Total mass of the stars under a node, M☉.
pub fn node_mass(node: &HierarchyNode) -> f64 {
    match node {
        HierarchyNode::Star(star) => star.mass_msun,
        HierarchyNode::Pair { a, b, .. } => node_mass(a) + node_mass(b),
    }
}

/// Every star of a hierarchy, in tree order.
pub fn stars(node: &HierarchyNode) -> Vec<&Star> {
    match node {
        HierarchyNode::Star(star) => vec![star],
        HierarchyNode::Pair { a, b, .. } => {
            let mut all = stars(a);
            all.extend(stars(b));
            all
        }
    }
}

/// The node a host reference points at.
pub fn find_host(node: &HierarchyNode, host: HostRef) -> Option<&HierarchyNode> {
    match (node, host) {
        (HierarchyNode::Star(star), HostRef::Star(id)) if star.id == id => Some(node),
        (HierarchyNode::Pair { id, .. }, HostRef::Barycenter(wanted)) if *id == wanted => {
            Some(node)
        }
        (HierarchyNode::Pair { a, b, .. }, _) => find_host(a, host).or_else(|| find_host(b, host)),
        _ => None,
    }
}

/// Mass a planet of `host` orbits, M☉.
pub fn host_mass(snapshot: &SystemSnapshot, host: HostRef) -> Option<f64> {
    find_host(&snapshot.hierarchy, host).map(node_mass)
}

/// The pair a star or sub-pair belongs to, with its sibling: `(pair id,
/// orbit, own mass, sibling mass)`.
pub fn parent_orbit(node: &HierarchyNode, host: HostRef) -> Option<(NodeId, &Orbit, f64, f64)> {
    let HierarchyNode::Pair { id, a, b, orbit } = node else {
        return None;
    };
    let is = |child: &HierarchyNode| match (child, host) {
        (HierarchyNode::Star(star), HostRef::Star(wanted)) => star.id == wanted,
        (HierarchyNode::Pair { id, .. }, HostRef::Barycenter(wanted)) => *id == wanted,
        _ => false,
    };
    if is(a) {
        Some((*id, orbit, node_mass(a), node_mass(b)))
    } else if is(b) {
        Some((*id, orbit, node_mass(b), node_mass(a)))
    } else {
        parent_orbit(a, host).or_else(|| parent_orbit(b, host))
    }
}

/// Scene positions of every star and every pair's centre of mass.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    pub stars: BTreeMap<StarId, DVec3>,
    pub centres: BTreeMap<NodeId, DVec3>,
}

impl Layout {
    /// Where the stars are at `orbit_days` on the orbital clock.
    pub fn at(hierarchy: &HierarchyNode, scale: &ScaleBlend, orbit_days: f64) -> Self {
        let mut layout = Self::default();
        layout.place(hierarchy, scale, orbit_days, DVec3::ZERO);
        layout
    }

    fn place(&mut self, node: &HierarchyNode, scale: &ScaleBlend, orbit_days: f64, origin: DVec3) {
        match node {
            HierarchyNode::Star(star) => {
                self.stars.insert(star.id, origin);
            }
            HierarchyNode::Pair { id, a, b, orbit } => {
                self.centres.insert(*id, origin);
                let relative = kepler::position(elements(orbit, orbit_days));
                let (mass_a, mass_b) = (node_mass(a), node_mass(b));
                let total = mass_a + mass_b;
                // `relative` is b as seen from a; each sits on its own side
                // of the centre of mass, the heavier one closer.
                let scaled = |factor: f64| {
                    map_offset(
                        scale,
                        [
                            relative[0] * factor,
                            relative[1] * factor,
                            relative[2] * factor,
                        ],
                    )
                };
                self.place(a, scale, orbit_days, origin + scaled(-mass_b / total));
                self.place(b, scale, orbit_days, origin + scaled(mass_a / total));
            }
        }
    }

    /// Scene position of the centre a planet of `host` orbits.
    pub fn host(&self, host: HostRef) -> Option<DVec3> {
        match host {
            HostRef::Star(id) => self.stars.get(&id).copied(),
            HostRef::Barycenter(id) => self.centres.get(&id).copied(),
            HostRef::Body(_) => None,
        }
    }
}

/// Points along the path a member of a pair traces around the pair's
/// centre of mass, relative to that centre: `fraction` is the member's
/// share of the separation (`-m_b/M` for a, `+m_a/M` for b).
pub fn member_path(
    orbit: &Orbit,
    fraction: f64,
    scale: &ScaleBlend,
    segments: usize,
) -> Vec<DVec3> {
    let el = elements(orbit, 0.0);
    (0..=segments)
        .map(|k| {
            let p = kepler::point_at_eccentric_anomaly(el, TAU * k as f64 / segments as f64);
            map_offset(scale, [p[0] * fraction, p[1] * fraction, p[2] * fraction])
        })
        .collect()
}

/// Farthest any host's centre gets from the system's origin, for each host,
/// in scene units: an upper bound over the stars' orbits.
fn reach(node: &HierarchyNode, scale: &ScaleBlend, so_far: f32, out: &mut BTreeMap<HostRef, f32>) {
    match node {
        HierarchyNode::Star(star) => {
            out.insert(HostRef::Star(star.id), so_far);
        }
        HierarchyNode::Pair { id, a, b, orbit } => {
            out.insert(HostRef::Barycenter(*id), so_far);
            let (mass_a, mass_b) = (node_mass(a), node_mass(b));
            let apoapsis = orbit.a_au * (1.0 + orbit.e) / (mass_a + mass_b);
            reach(
                a,
                scale,
                so_far + scale.distance((apoapsis * mass_b) as f32),
                out,
            );
            reach(
                b,
                scale,
                so_far + scale.distance((apoapsis * mass_a) as f32),
                out,
            );
        }
    }
}

/// Scene radius that fits the finished system: its stars' orbits and its
/// outermost planets, wherever on their orbits they are.
pub fn system_extent(snapshot: &SystemSnapshot, scale: &ScaleBlend) -> f32 {
    let mut hosts = BTreeMap::new();
    reach(&snapshot.hierarchy, scale, 0.0, &mut hosts);
    let stars_reach = hosts.values().copied().fold(0.0f32, f32::max);
    let planets_reach = snapshot
        .bodies
        .iter()
        .filter_map(|body| {
            let centre = hosts.get(&body.host)?;
            let apoapsis = body.orbit.a_au * (1.0 + body.orbit.e);
            Some(centre + scale.distance(apoapsis as f32))
        })
        .fold(0.0f32, f32::max);
    // A lone star without planets still needs a sensible frame.
    stars_reach.max(planets_reach).max(scale.distance(1.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::ScaleMode;
    use sg_schema::{Fidelity, PresetId};

    fn preset(preset: PresetId) -> SystemSnapshot {
        sg_sim::simulate(&sg_sim::preset_params(preset, 42, Fidelity::Classic))
            .unwrap()
            .snapshot
    }

    fn linear() -> ScaleBlend {
        ScaleBlend::settled(ScaleMode::True)
    }

    #[test]
    fn simulation_frame_maps_to_y_up() {
        // The orbital plane (z = 0) becomes the scene's horizontal plane.
        assert_eq!(to_scene([1.0, 2.0, 3.0]), DVec3::new(1.0, 3.0, -2.0));
        assert_eq!(to_scene([1.0, 2.0, 0.0]).y, 0.0);
    }

    #[test]
    fn offsets_keep_direction_and_take_the_mapped_distance() {
        let scale = ScaleBlend::settled(ScaleMode::Schematic);
        let p = map_offset(&scale, [3.0, 4.0, 0.0]);
        assert!((p.length() - f64::from(ScaleMode::Schematic.distance(5.0))).abs() < 1e-5);
        assert!((p.length() - ScaleMode::Schematic.distance_f64(5.0)).abs() < 1e-12);
        assert!((p.normalize() - DVec3::new(0.6, 0.0, -0.8)).length() < 1e-5);
        assert_eq!(map_offset(&scale, [0.0, 0.0, 0.0]), DVec3::ZERO);
    }

    #[test]
    fn a_single_star_sits_at_the_origin() {
        let snapshot = preset(PresetId::SunLike);
        let layout = Layout::at(&snapshot.hierarchy, &linear(), 123.0);
        assert_eq!(layout.stars.len(), 1);
        assert_eq!(layout.host(HostRef::Star(StarId(0))), Some(DVec3::ZERO));
        assert!(layout.centres.is_empty());
        assert!(parent_orbit(&snapshot.hierarchy, HostRef::Star(StarId(0))).is_none());
    }

    /// The centre of mass of a binary stays at the origin, the stars stay on
    /// opposite sides of it, and their separation stays within the orbit's
    /// periastron and apastron.
    #[test]
    fn binary_stars_balance_about_their_centre_of_mass() {
        let snapshot = preset(PresetId::STypeBinary);
        let HierarchyNode::Pair { orbit, .. } = &snapshot.hierarchy else {
            panic!("pair expected");
        };
        let all = stars(&snapshot.hierarchy);
        let (m0, m1) = (all[0].mass_msun, all[1].mass_msun);
        for step in 0..200 {
            let days = orbit.period_days * f64::from(step) / 200.0;
            let layout = Layout::at(&snapshot.hierarchy, &linear(), days);
            let (p0, p1) = (layout.stars[&StarId(0)], layout.stars[&StarId(1)]);
            let centre = (p0 * m0 + p1 * m1) / (m0 + m1);
            assert!(centre.length() < 1e-4, "{centre}");
            let separation = (p1 - p0).length();
            assert!(separation >= orbit.a_au * (1.0 - orbit.e) - 1e-3);
            assert!(separation <= orbit.a_au * (1.0 + orbit.e) + 1e-3);
            assert_eq!(
                layout.host(HostRef::Barycenter(NodeId(0))),
                Some(DVec3::ZERO)
            );
        }
        // One period later everything is back where it started.
        let start = Layout::at(&snapshot.hierarchy, &linear(), 0.0);
        let later = Layout::at(&snapshot.hierarchy, &linear(), orbit.period_days);
        assert!((start.stars[&StarId(0)] - later.stars[&StarId(0)]).length() < 1e-3);
    }

    #[test]
    fn triple_nests_the_inner_pair_inside_the_outer_orbit() {
        let snapshot = preset(PresetId::HierarchicalTriple);
        let layout = Layout::at(&snapshot.hierarchy, &linear(), 50.0);
        assert_eq!((layout.stars.len(), layout.centres.len()), (3, 2));
        let inner_centre = layout.centres[&NodeId(1)];
        // The inner pair's stars are within 0.25·1.1 AU of their own centre.
        for id in [StarId(0), StarId(1)] {
            assert!((layout.stars[&id] - inner_centre).length() < 0.28);
        }
        // The third star is tens of AU from that centre.
        let third = (layout.stars[&StarId(2)] - inner_centre).length();
        assert!((42.0..78.0).contains(&third), "{third}");
        assert_eq!(
            host_mass(&snapshot, HostRef::Barycenter(NodeId(1))),
            Some(1.7)
        );
        let (pair, _, own, sibling) =
            parent_orbit(&snapshot.hierarchy, HostRef::Star(StarId(2))).unwrap();
        assert_eq!((pair, own, sibling), (NodeId(0), 0.4, 1.7));
    }

    #[test]
    fn member_paths_close_and_scale_with_the_mass_share() {
        let snapshot = preset(PresetId::STypeBinary);
        let HierarchyNode::Pair { orbit, .. } = &snapshot.hierarchy else {
            panic!("pair expected");
        };
        let path = member_path(orbit, 0.5, &linear(), 64);
        assert_eq!(path.len(), 65);
        assert!((path[0] - path[64]).length() < 1e-4);
        let widest = path.iter().map(|p| p.length()).fold(0.0, f64::max);
        assert!((widest - 0.5 * orbit.a_au * (1.0 + orbit.e)).abs() < 0.05);
    }

    /// The extent covers every star and planet at every phase of the stars'
    /// orbits.
    #[test]
    fn extent_contains_the_whole_system_at_all_times() {
        for id in [
            PresetId::SunLike,
            PresetId::Circumbinary,
            PresetId::STypeBinary,
            PresetId::HierarchicalTriple,
        ] {
            let snapshot = preset(id);
            for mode in ScaleMode::ALL {
                let scale = ScaleBlend::settled(mode);
                let extent = system_extent(&snapshot, &scale);
                for step in 0..40 {
                    let layout = Layout::at(&snapshot.hierarchy, &scale, f64::from(step) * 977.0);
                    for position in layout.stars.values() {
                        assert!(position.length() as f32 <= extent + 1e-3, "{id:?} {mode:?}");
                    }
                    for body in &snapshot.bodies {
                        let centre = layout.host(body.host).unwrap();
                        let apoapsis =
                            scale.distance((body.orbit.a_au * (1.0 + body.orbit.e)) as f32);
                        assert!(
                            centre.length() as f32 + apoapsis <= extent + 1e-3,
                            "{id:?} {mode:?}"
                        );
                    }
                }
            }
        }
    }
}
