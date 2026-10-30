//! What is on screen at a given moment of playback.
//!
//! Pure functions from a [`SimulationResult`] and a playback time to a list
//! of bodies to draw. No Bevy types, so this is unit-tested directly.
//!
//! Before the end of the timeline the bodies are the embryos of the nearest
//! keyframes, interpolated; at the end they are the finished planets of the
//! snapshot. The renderer invents nothing here except where the simulation
//! is silent, and those cases are marked.

use sg_core::hash::fnv1a64;
use sg_schema::{BodyId, EventKind, HostRef, Keyframe, KeyframeBody, PlanetClass, SystemSnapshot};
use sg_sim::SimulationResult;
use std::collections::BTreeMap;

const EARTH_RADIUS_KM: f64 = 6378.0;

/// A body to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayBody {
    pub id: BodyId,
    /// The star or pair it orbits.
    pub host: HostRef,
    pub a_au: f64,
    pub e: f64,
    pub mass_mearth: f64,
    pub radius_km: f64,
    /// `None` while still forming.
    pub class: Option<PlanetClass>,
    pub long_asc_node_deg: f64,
    pub arg_periapsis_deg: f64,
    /// Mean anomaly at the orbital clock's zero.
    pub mean_anomaly_deg: f64,
    /// 0 → 1 as the body appears, 1 → 0 as it is about to be absorbed.
    pub presence: f32,
}

/// Radius shown for an embryo, which the Classic model gives no radius: a
/// sphere of Earth's mean density, `R⊕·(M/M⊕)^(1/3)`. A drawing convention,
/// not a model output.
pub fn embryo_radius_km(mass_mearth: f64) -> f64 {
    EARTH_RADIUS_KM * mass_mearth.max(0.0).cbrt()
}

/// Orbital angles for an embryo that does not survive to the snapshot, so
/// has none: spread deterministically by its id.
fn angles_for(id: BodyId, snapshot: &SystemSnapshot) -> (f64, f64, f64) {
    if let Some(body) = snapshot.bodies.iter().find(|b| b.id == id) {
        return (
            body.orbit.long_asc_node_deg,
            body.orbit.arg_periapsis_deg,
            body.orbit.mean_anomaly_deg,
        );
    }
    let h = fnv1a64(&id.0.to_le_bytes());
    let angle = |shift: u32| ((h >> shift) & 0xFFFF) as f64 / 65536.0 * 360.0;
    (angle(0), angle(16), angle(32))
}

fn final_bodies(snapshot: &SystemSnapshot) -> Vec<DisplayBody> {
    let mut bodies: Vec<DisplayBody> = snapshot
        .bodies
        .iter()
        .map(|b| DisplayBody {
            id: b.id,
            host: b.host,
            a_au: b.orbit.a_au,
            e: b.orbit.e,
            mass_mearth: b.mass_mearth,
            radius_km: b.radius_km,
            class: Some(b.class),
            long_asc_node_deg: b.orbit.long_asc_node_deg,
            arg_periapsis_deg: b.orbit.arg_periapsis_deg,
            mean_anomaly_deg: b.orbit.mean_anomaly_deg,
            presence: 1.0,
        })
        .collect();
    bodies.sort_by_key(|b| b.id);
    bodies
}

/// End of the timeline, Myr: the time of the last keyframe or event.
pub fn timeline_end_myr(result: &SimulationResult) -> f64 {
    let keyframes = result.timeline.keyframes.last().map_or(0.0, |k| k.t_myr);
    let events = result.timeline.events.last().map_or(0.0, |e| e.t_myr);
    keyframes.max(events)
}

/// The bodies to draw at playback time `t_myr`, ordered by id.
pub fn bodies_at(result: &SimulationResult, t_myr: f64) -> Vec<DisplayBody> {
    let end = timeline_end_myr(result);
    let keyframes = &result.timeline.keyframes;
    if t_myr >= end || keyframes.is_empty() {
        return final_bodies(&result.snapshot);
    }

    let empty = Keyframe {
        t_myr: 0.0,
        bodies: Vec::new(),
    };
    let next_index = keyframes.partition_point(|k| k.t_myr <= t_myr);
    let before = if next_index == 0 {
        &empty
    } else {
        &keyframes[next_index - 1]
    };
    let after = &keyframes[next_index.min(keyframes.len() - 1)];
    let span = after.t_myr - before.t_myr;
    let frac = if span > 0.0 {
        ((t_myr - before.t_myr) / span).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let approaching_final = next_index >= keyframes.len() - 1;

    let later: BTreeMap<BodyId, &KeyframeBody> = after.bodies.iter().map(|b| (b.id, b)).collect();
    let earlier: BTreeMap<BodyId, &KeyframeBody> =
        before.bodies.iter().map(|b| (b.id, b)).collect();
    let lerp = |a: f64, b: f64| a + (b - a) * frac;

    let make = |from: &KeyframeBody, a_au: f64, e: f64, mass_mearth: f64, presence: f64| {
        let id = from.id;
        let mut radius_km = embryo_radius_km(mass_mearth);
        if approaching_final {
            // Ease into the finished planet's modelled radius.
            if let Some(body) = result.snapshot.bodies.iter().find(|b| b.id == id) {
                radius_km = lerp(radius_km, body.radius_km);
            }
        }
        let (long_asc_node_deg, arg_periapsis_deg, mean_anomaly_deg) =
            angles_for(id, &result.snapshot);
        DisplayBody {
            id,
            host: from.host,
            a_au,
            e,
            mass_mearth,
            radius_km,
            class: None,
            long_asc_node_deg,
            arg_periapsis_deg,
            mean_anomaly_deg,
            presence: presence as f32,
        }
    };

    let mut bodies = Vec::with_capacity(before.bodies.len() + after.bodies.len());
    for b in &before.bodies {
        bodies.push(match later.get(&b.id) {
            Some(n) => make(
                b,
                lerp(b.a_au, n.a_au),
                lerp(b.e, n.e),
                lerp(b.mass_mearth, n.mass_mearth),
                1.0,
            ),
            // Absorbed by another body before the next keyframe.
            None => make(b, b.a_au, b.e, b.mass_mearth, 1.0 - frac),
        });
    }
    for n in &after.bodies {
        if !earlier.contains_key(&n.id) {
            bodies.push(make(n, n.a_au, n.e, n.mass_mearth, frac));
        }
    }
    // Not yet appeared, or fully absorbed.
    bodies.retain(|b| b.presence > 0.0);
    bodies.sort_by_key(|b| b.id);
    bodies
}

/// What a body orbits, whether or not it survives to the finished system:
/// from the snapshot, else from the event that injected it, else from the
/// keyframes.
pub fn host_of(result: &SimulationResult, id: BodyId) -> Option<HostRef> {
    if let Some(body) = result.snapshot.bodies.iter().find(|b| b.id == id) {
        return Some(body.host);
    }
    let injected = result
        .timeline
        .events
        .iter()
        .find_map(|event| match event.kind {
            EventKind::EmbryoInjected { body, host, .. } if body == id => Some(host),
            _ => None,
        });
    injected.or_else(|| {
        result
            .timeline
            .keyframes
            .iter()
            .flat_map(|k| &k.bodies)
            .find(|b| b.id == id)
            .map(|b| b.host)
    })
}

/// A formation event worth flashing on screen.
#[derive(Debug, Clone, PartialEq)]
pub enum Highlight {
    /// Two bodies merged at about this orbital distance from `host`.
    Collision { host: HostRef, a_au: f64, age: f32 },
    /// A body cleared the dust between these distances from `host`.
    Sweep {
        host: HostRef,
        inner_au: f64,
        outer_au: f64,
        age: f32,
    },
}

/// Events in the `lookback_myr` before `t_myr`, with `age` running from 0
/// (just happened) to 1 (about to expire).
pub fn highlights_at(result: &SimulationResult, t_myr: f64, lookback_myr: f64) -> Vec<Highlight> {
    if lookback_myr <= 0.0 || t_myr >= timeline_end_myr(result) {
        return Vec::new();
    }
    let events = &result.timeline.events;
    // Half open, so that an event on the edge of two windows is seen once.
    let start = events.partition_point(|e| e.t_myr <= t_myr - lookback_myr);
    let end = events.partition_point(|e| e.t_myr <= t_myr);
    let bodies = bodies_at(result, t_myr);
    events[start..end]
        .iter()
        .filter_map(|event| {
            let age = ((t_myr - event.t_myr) / lookback_myr) as f32;
            match event.kind {
                EventKind::Collision {
                    result: survivor, ..
                } => bodies
                    .iter()
                    .find(|b| b.id == survivor)
                    .map(|b| Highlight::Collision {
                        host: b.host,
                        a_au: b.a_au,
                        age,
                    }),
                EventKind::DustSwept {
                    body,
                    inner_au,
                    outer_au,
                    ..
                } => Some(Highlight::Sweep {
                    host: host_of(result, body)?,
                    inner_au,
                    outer_au,
                    age,
                }),
                _ => None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sg_schema::SimulationParams;

    fn result() -> SimulationResult {
        sg_sim::simulate(&SimulationParams {
            seed: 42,
            ..SimulationParams::default()
        })
        .expect("classic run")
    }

    #[test]
    fn embryo_radius_follows_constant_density() {
        assert_eq!(embryo_radius_km(1.0), 6378.0);
        assert!((embryo_radius_km(8.0) - 12_756.0).abs() < 1e-6);
        assert_eq!(embryo_radius_km(0.0), 0.0);
    }

    #[test]
    fn end_of_timeline_shows_the_snapshot() {
        let result = result();
        let end = timeline_end_myr(&result);
        assert_eq!(end, 10.0);
        let bodies = bodies_at(&result, end);
        assert_eq!(bodies.len(), result.snapshot.bodies.len());
        for body in &result.snapshot.bodies {
            let shown = bodies.iter().find(|b| b.id == body.id).expect("shown");
            assert_eq!(shown.radius_km, body.radius_km);
            assert_eq!(shown.class, Some(body.class));
            assert_eq!(shown.presence, 1.0);
        }
        assert_eq!(bodies_at(&result, end + 5.0), bodies);
    }

    #[test]
    fn formation_starts_empty_and_grows() {
        let result = result();
        assert!(bodies_at(&result, 0.0).is_empty());
        let early = bodies_at(&result, 1.0);
        let late = bodies_at(&result, 9.0);
        assert!(!early.is_empty());
        assert!(early.iter().all(|b| b.class.is_none()));
        let mass = |bodies: &[DisplayBody]| bodies.iter().map(|b| b.mass_mearth).sum::<f64>();
        assert!(mass(&late) > mass(&early));
    }

    #[test]
    fn playback_is_continuous_into_the_final_system() {
        let result = result();
        let end = timeline_end_myr(&result);
        let just_before = bodies_at(&result, end * (1.0 - 1e-9));
        // Moons are not part of formation playback: they appear with the
        // finished system.
        let at_end: Vec<DisplayBody> = bodies_at(&result, end)
            .into_iter()
            .filter(|b| !matches!(b.host, HostRef::Body(_)))
            .collect();
        assert_eq!(just_before.len(), at_end.len());
        for (a, b) in just_before.iter().zip(&at_end) {
            assert_eq!(a.id, b.id);
            assert!((a.radius_km / b.radius_km - 1.0).abs() < 1e-6);
            assert!((a.a_au / b.a_au - 1.0).abs() < 1e-6);
            // Same angles before and after, so nothing jumps.
            assert_eq!(a.mean_anomaly_deg, b.mean_anomaly_deg);
        }
    }

    #[test]
    fn bodies_are_valid_at_every_time() {
        let result = result();
        for step in 0..=200 {
            let t = step as f64 * 0.05;
            let bodies = bodies_at(&result, t);
            for pair in bodies.windows(2) {
                assert!(pair[0].id < pair[1].id);
            }
            for b in &bodies {
                assert!(b.a_au > 0.0 && (0.0..1.0).contains(&b.e));
                assert!(b.radius_km >= 0.0 && b.radius_km.is_finite());
                assert!((0.0..=1.0).contains(&b.presence));
                assert!((0.0..360.0).contains(&b.mean_anomaly_deg));
            }
        }
    }

    #[test]
    fn highlights_appear_during_formation_only() {
        let result = result();
        let end = timeline_end_myr(&result);
        let total: usize = (0..200)
            .map(|step| highlights_at(&result, step as f64 * end / 200.0, end / 200.0).len())
            .sum();
        // Every sweep and collision is seen once when the windows tile the timeline.
        let expected = result
            .timeline
            .events
            .iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    EventKind::Collision { .. } | EventKind::DustSwept { .. }
                )
            })
            .count();
        assert!(total > 0 && total <= expected, "{total} of {expected}");
        assert!(highlights_at(&result, end, 1.0).is_empty());
        for h in highlights_at(&result, 5.0, 2.0) {
            let age = match h {
                Highlight::Collision { age, .. } | Highlight::Sweep { age, .. } => age,
            };
            assert!((0.0..=1.0).contains(&age));
        }
    }
}
