//! A small spaceship that visits every planet and moon in turn.
//!
//! This is for fun and is not simulated: nothing in the model knows about
//! it, and its speed, path and size are drawing choices. A leg takes the same
//! time however far it is, the ship circles each body once, and its size
//! follows the body's as drawn so that it can be seen beside it.
//!
//! The ship's position is kept in system coordinates, in double precision,
//! and converted with [`OrbitCamera::local`], because the renderer's origin
//! moves every frame (PLAN.md D43).

use crate::camera::OrbitCamera;
use crate::scene::{BodyPlacements, Selection, Target};
use bevy::math::DVec3;
use bevy::prelude::*;
use core::f32::consts::TAU;
use sg_schema::{BodyKind, HostRef, SystemSnapshot};

/// Seconds to fly from one body to the next.
const LEG_SECS: f32 = 4.0;
/// Seconds to circle a body.
const VISIT_SECS: f32 = 3.0;
/// Radius of the circle around a body, in the body's drawn radii.
const VISIT_RADII: f64 = 2.2;
/// Height of the circle above the body, as a share of its radius.
const VISIT_HEIGHT: f64 = 0.4;
/// The ship's length, as a share of the visited body's drawn radius.
const SHIP_SHARE: f32 = 0.3;

/// The bodies in the order the ship visits them: each planet in the
/// snapshot's order, followed by its moons.
pub fn route(snapshot: &SystemSnapshot) -> Vec<Target> {
    let mut stops = Vec::new();
    for planet in snapshot.planets() {
        stops.push(Target::Body(planet.id));
        stops.extend(
            snapshot
                .bodies
                .iter()
                .filter(|b| b.kind == BodyKind::Moon && b.host == HostRef::Body(planet.id))
                .map(|moon| Target::Body(moon.id)),
        );
    }
    stops
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The point at `angle` on the circle around a body.
fn circling(centre: DVec3, radius: f32, angle: f32) -> DVec3 {
    let r = f64::from(radius);
    let (sin, cos) = (f64::from(angle).sin(), f64::from(angle).cos());
    centre
        + DVec3::new(
            cos * VISIT_RADII * r,
            VISIT_HEIGHT * r,
            sin * VISIT_RADII * r,
        )
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    /// Flying in from a position, with the size the ship had there.
    Flying { from: DVec3, size: f32, clock: f32 },
    /// Circling, at this angle.
    Circling { angle: f32 },
}

/// Where the ship is this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fix {
    pub position: DVec3,
    /// Unit vector along the ship's motion.
    pub heading: DVec3,
    pub size: f32,
}

/// The tour in progress. A tour begins already circling its first body.
#[derive(Debug, Clone, PartialEq)]
struct Tour {
    route: Vec<Target>,
    stop: usize,
    phase: Phase,
    last: Option<Fix>,
    /// The stop has changed and has not yet been selected.
    arrived: bool,
}

impl Tour {
    fn new(route: Vec<Target>, stop: usize) -> Self {
        Self {
            route,
            stop,
            phase: Phase::Circling { angle: 0.0 },
            last: None,
            arrived: true,
        }
    }

    /// Moves on by `dt` seconds. `place` gives a body's position and drawn
    /// radius; one not there at this moment of the timeline is passed over.
    /// `None` when the tour is over.
    fn step(&mut self, dt: f32, place: impl Fn(Target) -> Option<(DVec3, f32)>) -> Option<Fix> {
        let (centre, radius) = loop {
            match place(*self.route.get(self.stop)?) {
                Some(found) => break found,
                None => self.stop += 1,
            }
        };
        let size = SHIP_SHARE * radius;
        let (position, size) = match &mut self.phase {
            Phase::Circling { angle } => {
                *angle += dt * TAU / VISIT_SECS;
                (circling(centre, radius, *angle), size)
            }
            Phase::Flying {
                from,
                size: from_size,
                clock,
            } => {
                *clock += dt;
                let s = smoothstep(*clock / LEG_SECS);
                let end = circling(centre, radius, 0.0);
                (from.lerp(end, f64::from(s)), from_size.lerp(size, s))
            }
        };
        let heading = self
            .last
            .and_then(|last| (position - last.position).try_normalize())
            .or(self.last.map(|last| last.heading))
            .unwrap_or(DVec3::X);
        let fix = Fix {
            position,
            heading,
            size,
        };
        self.last = Some(fix);

        match self.phase {
            Phase::Circling { angle } if angle >= TAU => {
                self.stop += 1;
                self.arrived = true;
                self.phase = Phase::Flying {
                    from: position,
                    size,
                    clock: 0.0,
                };
            }
            Phase::Flying { clock, .. } if clock >= LEG_SECS => {
                self.phase = Phase::Circling { angle: 0.0 };
            }
            _ => {}
        }
        Some(fix)
    }
}

/// The ship's tour, if one is under way.
#[derive(Resource, Debug, Default)]
pub struct Voyage(Option<Tour>);

impl Voyage {
    /// Sets off through `snapshot`, from the stop `first` if it is on the
    /// route. A system with no planets gives no tour.
    pub fn start(&mut self, snapshot: &SystemSnapshot, first: Option<Target>) {
        let route = route(snapshot);
        let stop = first
            .and_then(|target| route.iter().position(|t| *t == target))
            .unwrap_or(0);
        self.0 = (!route.is_empty()).then(|| Tour::new(route, stop));
    }

    pub fn stop(&mut self) {
        self.0 = None;
    }

    pub fn active(&self) -> bool {
        self.0.is_some()
    }
}

#[derive(Component)]
pub(crate) struct Ship;

/// Builds the ship from plain shapes, hidden until a tour starts. It points
/// along +Y, and is one unit long.
pub(crate) fn spawn_ship(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let hull = materials.add(StandardMaterial {
        base_color: Color::srgb(0.82, 0.84, 0.88),
        metallic: 0.6,
        perceptual_roughness: 0.35,
        ..default()
    });
    let engine = materials.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive: LinearRgba::rgb(2.0, 5.0, 12.0),
        ..default()
    });
    commands
        .spawn((Ship, Transform::default(), Visibility::Hidden))
        .with_children(|ship| {
            ship.spawn((
                Mesh3d(meshes.add(Cone::new(0.18, 1.0))),
                MeshMaterial3d(hull.clone()),
            ));
            ship.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.8, 0.04, 0.22))),
                MeshMaterial3d(hull),
                Transform::from_xyz(0.0, -0.32, 0.0),
            ));
            ship.spawn((
                Mesh3d(meshes.add(Sphere::new(0.1))),
                MeshMaterial3d(engine),
                Transform::from_xyz(0.0, -0.5, 0.0),
            ));
        });
}

/// Moves the tour on and places the ship.
pub(crate) fn sync_ship(
    time: Res<Time<Real>>,
    mut voyage: ResMut<Voyage>,
    placements: Res<BodyPlacements>,
    orbit: Res<OrbitCamera>,
    mut selection: ResMut<Selection>,
    ship: Single<(&mut Transform, &mut Visibility), With<Ship>>,
) {
    let (mut transform, mut visibility) = ship.into_inner();
    let fix = voyage.0.as_mut().and_then(|tour| {
        tour.step(time.delta_secs(), |target| {
            placements
                .get(target)
                .map(|p| (orbit.origin + p.position.as_dvec3(), p.radius))
        })
    });
    let Some(fix) = fix else {
        voyage.0 = None;
        *visibility = Visibility::Hidden;
        return;
    };
    if let Some(tour) = voyage.0.as_mut().filter(|tour| tour.arrived) {
        tour.arrived = false;
        selection.0 = tour.route.get(tour.stop).copied();
    }
    *visibility = Visibility::Inherited;
    *transform = Transform {
        translation: orbit.local(fix.position),
        rotation: Quat::from_rotation_arc(Vec3::Y, fix.heading.as_vec3()),
        scale: Vec3::splat(fix.size),
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use sg_schema::BodyId;

    const A: Target = Target::Body(BodyId(0));
    const B: Target = Target::Body(BodyId(1));
    const C: Target = Target::Body(BodyId(2));

    fn at(target: Target) -> Option<(DVec3, f32)> {
        match target {
            t if t == A => Some((DVec3::ZERO, 1.0)),
            t if t == B => Some((DVec3::new(100.0, 0.0, 0.0), 2.0)),
            _ => None,
        }
    }

    #[test]
    fn a_tour_visits_each_stop_then_ends() {
        let mut tour = Tour::new(vec![A, B], 0);
        let mut seen = vec![tour.route[tour.stop]];
        let mut frames = 0;
        while tour.step(0.05, at).is_some() {
            frames += 1;
            assert!(frames < 1000, "never ended");
            if let Some(stop) = tour.route.get(tour.stop) {
                if seen.last() != Some(stop) {
                    seen.push(*stop);
                }
            }
        }
        assert_eq!(seen, [A, B]);
        // Two circles and one leg.
        let expected = (2.0 * VISIT_SECS + LEG_SECS) / 0.05;
        assert!((frames as f32 - expected).abs() < 4.0, "{frames}");
    }

    #[test]
    fn a_body_that_is_not_there_is_passed_over() {
        let mut tour = Tour::new(vec![C, B], 0);
        tour.step(0.05, at).unwrap();
        assert_eq!(tour.route[tour.stop], B);
    }

    #[test]
    fn the_ship_does_not_jump_on_arrival_or_departure() {
        let mut tour = Tour::new(vec![A, B], 0);
        let mut last = tour.step(0.01, at).unwrap().position;
        while let Some(fix) = tour.step(0.01, at) {
            let moved = (fix.position - last).length();
            assert!(moved < 1.0, "jumped {moved}");
            last = fix.position;
        }
    }

    #[test]
    fn the_ship_circles_clear_of_the_body() {
        let mut tour = Tour::new(vec![A], 0);
        while let Some(fix) = tour.step(0.1, at) {
            assert!(fix.position.length() > 2.0);
            assert_eq!(fix.size, SHIP_SHARE);
        }
    }
}
