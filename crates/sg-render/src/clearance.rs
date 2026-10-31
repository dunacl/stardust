//! Keeps enlarged bodies off each other's orbits.
//!
//! Scale modes enlarge bodies far more than they stretch orbits, so a body's
//! drawn sphere, rings and moons can reach a neighbour's path although the
//! orbits themselves are well apart. This is a drawing choice, not a model
//! output: each body gets half the gap to its nearest neighbour, and is drawn
//! smaller if it would not fit. A moon's orbit is scaled with its planet's
//! drawn radius, so a planet's moons are compressed, keeping their order, into
//! the same room. The card still gives the true sizes and distances.
//!
//! A body is never drawn below its true size, so True scale is left alone.
//! The dots that stand for bodies seen from afar have a fixed size on screen
//! and are not limited here.

use crate::display::DisplayBody;
use crate::scale::{ScaleBlend, ScaleMode};
use crate::scene::KM_PER_AU;
use sg_schema::{BodyId, HostRef};
use std::collections::BTreeMap;

/// Share of its room a planet with moons leaves beyond itself and its rings
/// for them, as a multiple of that extent.
const MOON_ROOM: f32 = 2.0;

/// Share of a planet's room its moons' orbits may fill: the rest is for the
/// moons' own size.
const MOON_ORBITS_SHARE: f32 = 0.8;

/// How a body is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drawn {
    /// Radius in scene units.
    pub radius: f32,
    /// A moon's semi-major axis in scene units; zero for a planet.
    pub semi_major: f32,
}

/// Half the gap each span has to its neighbours, for spans of periapsis and
/// apoapsis ordered by periapsis, the first bounded on the inside by
/// `inner_edge` and the last on the outside by `outer_edge`.
fn rooms(spans: &[(f32, f32)], inner_edge: f32, outer_edge: f32) -> Vec<f32> {
    (0..spans.len())
        .map(|k| {
            let inward = spans[k].0 - if k == 0 { inner_edge } else { spans[k - 1].1 };
            let next = spans.get(k + 1).map_or(outer_edge, |s| s.0);
            let outward = next - spans[k].1;
            (0.5 * inward.min(outward)).max(0.0)
        })
        .collect()
}

/// A moon's semi-major axis in planetary radii once its planet's moons are
/// fitted within `room` radii. `extent` is what the planet and its rings
/// take up, `farthest` the largest apoapsis among the moons, both in radii.
/// Below `room` nothing changes; above it, apoapses are compressed by a
/// power law, which keeps their order and leaves `extent` where it is.
fn fitted_semi_major(a: f32, e: f32, extent: f32, farthest: f32, room: f32) -> f32 {
    if farthest <= room {
        return a;
    }
    let power = ((room / extent).ln() / (farthest / extent).ln()).clamp(0.0, 1.0);
    extent * (a * (1.0 + e) / extent).powf(power) / (1.0 + e)
}

/// The drawn size of every body. `ring_outer` gives the outer edge of a
/// planet's rings in planetary radii.
pub fn drawn_sizes(
    bodies: &[DisplayBody],
    ring_outer: &BTreeMap<BodyId, f32>,
    scale: &ScaleBlend,
) -> BTreeMap<BodyId, Drawn> {
    let mut hosts: BTreeMap<HostRef, Vec<&DisplayBody>> = BTreeMap::new();
    let mut moons: BTreeMap<BodyId, Vec<&DisplayBody>> = BTreeMap::new();
    for body in bodies {
        match body.host {
            HostRef::Body(planet) => moons.entry(planet).or_default().push(body),
            host => hosts.entry(host).or_default().push(body),
        }
    }

    let mut drawn = BTreeMap::new();
    for planets in hosts.values_mut() {
        let span = |b: &DisplayBody| {
            (
                scale.distance((b.a_au * (1.0 - b.e)) as f32),
                scale.distance((b.a_au * (1.0 + b.e)) as f32),
            )
        };
        planets.sort_by(|a, b| span(a).0.total_cmp(&span(b).0));
        let spans: Vec<_> = planets.iter().map(|b| span(b)).collect();
        // A star is never drawn beyond half the distance to its closest
        // planet (see the star's radius in `scene`).
        let inner_edge = 0.5 * spans.first().map_or(0.0, |s| s.0);
        for (planet, room) in planets.iter().zip(rooms(&spans, inner_edge, f32::INFINITY)) {
            let extent = ring_outer.get(&planet.id).copied().unwrap_or(1.0).max(1.0);
            let own_moons = moons.get(&planet.id).map_or(&[][..], Vec::as_slice);
            let reach = extent * if own_moons.is_empty() { 1.0 } else { MOON_ROOM };
            let wanted = scale.planet_radius(planet.radius_km as f32);
            let true_size = ScaleMode::True.planet_radius(planet.radius_km as f32);
            let radius = wanted.min((room / reach).max(true_size));
            drawn.insert(
                planet.id,
                Drawn {
                    radius,
                    semi_major: 0.0,
                },
            );
            place_moons(
                own_moons,
                planet,
                (radius, radius / wanted.max(f32::MIN_POSITIVE)),
                extent,
                room,
                scale,
                &mut drawn,
            );
        }
    }
    drawn
}

/// Places a planet's moons, given its drawn radius and how far that is below
/// the radius the scale mode wanted, which their sizes follow.
fn place_moons(
    moons: &[&DisplayBody],
    planet: &DisplayBody,
    (radius, shrink): (f32, f32),
    extent: f32,
    room: f32,
    scale: &ScaleBlend,
    drawn: &mut BTreeMap<BodyId, Drawn>,
) {
    let km_per_radius = planet.radius_km.max(f64::MIN_POSITIVE);
    let in_radii = |moon: &DisplayBody| (moon.a_au * KM_PER_AU / km_per_radius) as f32;
    let farthest = moons
        .iter()
        .map(|m| in_radii(m) * (1.0 + m.e as f32))
        .fold(0.0, f32::max);
    let room_in_radii = MOON_ORBITS_SHARE * room / radius.max(f32::MIN_POSITIVE);
    let mut placed: Vec<(&DisplayBody, f32)> = moons
        .iter()
        .map(|m| {
            let a = fitted_semi_major(in_radii(m), m.e as f32, extent, farthest, room_in_radii);
            (*m, a * radius)
        })
        .collect();
    placed.sort_by(|a, b| a.1.total_cmp(&b.1));
    let spans: Vec<_> = placed
        .iter()
        .map(|(m, a)| (a * (1.0 - m.e as f32), a * (1.0 + m.e as f32)))
        .collect();
    for ((moon, semi_major), room) in placed.iter().zip(rooms(&spans, extent * radius, room)) {
        let wanted = scale.planet_radius(moon.radius_km as f32) * shrink;
        let true_size = ScaleMode::True.planet_radius(moon.radius_km as f32);
        drawn.insert(
            moon.id,
            Drawn {
                radius: wanted.min(room.max(true_size)),
                semi_major: *semi_major,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sg_schema::StarId;

    fn body(id: u32, host: HostRef, a_au: f64, e: f64, radius_km: f64) -> DisplayBody {
        DisplayBody {
            id: BodyId(id),
            host,
            a_au,
            e,
            mass_mearth: 1.0,
            radius_km,
            class: None,
            long_asc_node_deg: 0.0,
            arg_periapsis_deg: 0.0,
            mean_anomaly_deg: 0.0,
            presence: 1.0,
        }
    }

    const STAR: HostRef = HostRef::Star(StarId(0));
    const ALL_MODES: [ScaleMode; 3] = ScaleMode::ALL;

    #[test]
    fn neighbours_never_touch_in_any_mode() {
        let bodies = vec![
            body(0, STAR, 0.05, 0.0, 70_000.0),
            body(1, STAR, 0.06, 0.05, 25_000.0),
            body(2, STAR, 0.08, 0.1, 6_378.0),
            body(3, STAR, 5.0, 0.05, 70_000.0),
        ];
        for mode in ALL_MODES {
            let scale = ScaleBlend::settled(mode);
            let sizes = drawn_sizes(&bodies, &BTreeMap::new(), &scale);
            for pair in bodies.windows(2) {
                let outer_edge = scale.distance((pair[0].a_au * (1.0 + pair[0].e)) as f32)
                    + sizes[&pair[0].id].radius;
                let inner_edge = scale.distance((pair[1].a_au * (1.0 - pair[1].e)) as f32)
                    - sizes[&pair[1].id].radius;
                assert!(outer_edge <= inner_edge, "{mode:?}: {pair:?}");
            }
        }
    }

    #[test]
    fn a_body_with_room_keeps_the_size_the_mode_gives_it() {
        let bodies = vec![body(0, STAR, 5.0, 0.0, 70_000.0)];
        let scale = ScaleBlend::settled(ScaleMode::Exaggerated);
        let sizes = drawn_sizes(&bodies, &BTreeMap::new(), &scale);
        assert_eq!(sizes[&BodyId(0)].radius, scale.planet_radius(70_000.0));
    }

    #[test]
    fn true_scale_is_left_alone() {
        // Orbits that overlap leave no room: the body keeps its true size.
        let bodies = vec![
            body(0, STAR, 1.0, 0.0, 6_378.0),
            body(1, STAR, 1.0, 0.0, 6_378.0),
        ];
        let scale = ScaleBlend::settled(ScaleMode::True);
        let sizes = drawn_sizes(&bodies, &BTreeMap::new(), &scale);
        assert_eq!(sizes[&BodyId(0)].radius, scale.planet_radius(6_378.0));
    }

    #[test]
    fn a_moon_and_rings_stay_within_the_planets_room() {
        // Earth and the Moon, with Venus 0.28 AU inside: at 60 enlarged radii
        // the Moon would be drawn 1.2 AU from Earth.
        let earth = BodyId(1);
        let bodies = vec![
            body(0, STAR, 0.72, 0.0, 6_052.0),
            body(1, STAR, 1.0, 0.0, 6_378.0),
            body(2, HostRef::Body(earth), 0.00257, 0.05, 1_737.0),
        ];
        let rings = BTreeMap::from([(earth, 2.3)]);
        let scale = ScaleBlend::settled(ScaleMode::Exaggerated);
        let sizes = drawn_sizes(&bodies, &rings, &scale);
        let planet = sizes[&earth].radius;
        let moon = sizes[&BodyId(2)];
        let reach = moon.semi_major * 1.05 + moon.radius;
        assert!(reach <= 0.14 + 1e-5, "{reach}");
        assert!(moon.semi_major * 0.95 - moon.radius >= 2.3 * planet);
    }

    #[test]
    fn moons_keep_their_order_when_compressed() {
        let order = [5.9f32, 9.4, 15.0, 26.0].map(|a| fitted_semi_major(a, 0.0, 1.0, 26.0, 4.0));
        assert!(order.windows(2).all(|p| p[0] < p[1]));
        assert!((order[3] - 4.0).abs() < 1e-4);
        assert!(order[0] > 1.0);
    }

    #[test]
    fn moons_with_room_are_not_moved() {
        assert_eq!(fitted_semi_major(9.0, 0.1, 1.0, 9.9, 20.0), 9.0);
    }
}
