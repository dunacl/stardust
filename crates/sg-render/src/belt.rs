//! Asteroid belts.
//!
//! Two things are drawn as a belt of small rocks:
//!
//! - a `Belt` of the snapshot (produced from M4 on), between its edges;
//! - a body of class `Asteroids` whose mass is below
//!   [`BELT_MASS_LIMIT_MEARTH`].
//!
//! The mass test keeps a heavier body that somehow carries the label a single
//! world. The Classic tier gives the label by the same limit (`sg-sim`).
//!
//! The simulation gives a belt's place and mass, not its members. The rocks
//! are a drawing: their number, sizes and orbital scatter are chosen here,
//! and their drawn size is far larger than life so they can be seen at all.

use crate::layout::{self, Layout};
use crate::scene::{self, Playback, SystemView, ViewSettings};
use crate::OrbitCamera;
use bevy::prelude::*;
use sg_core::rng::{ModuleId, RngStream};
use sg_core::units::{Au, SolarMass};
use sg_dynamics::kepler::{self, Elements};
use sg_schema::{Body, HostRef, PlanetClass, SystemSnapshot};
use std::f64::consts::TAU;

/// Above this a body is a world, not a belt, in Earth masses. A drawing choice;
/// for scale, the Solar System's main belt is about 4e-4.
pub const BELT_MASS_LIMIT_MEARTH: f64 = 1.0e-3;

/// Half-width of the belt drawn around an asteroid-class body, as a fraction
/// of its semi-major axis. A drawing convention.
const BODY_BELT_HALF_WIDTH: f64 = 0.08;
/// Largest eccentricity and inclination (radians) given to a rock.
const MAX_ROCK_ECCENTRICITY: f64 = 0.12;
const MAX_ROCK_INCLINATION: f64 = 0.10;
/// A rock's drawn radius relative to the smallest a planet is drawn.
const ROCK_APPARENT_SIZE: f32 = 0.22;

/// When a belt is selected the camera frames a stretch of it this wide, as
/// a fraction of the belt's distance from the star, rather than closing in
/// on one rock.
pub(crate) const SELECTED_VIEW_FRACTION: f32 = 0.06;

/// True if a body of this class and mass is drawn as a belt.
pub fn is_belt_sized(class: PlanetClass, mass_mearth: f64) -> bool {
    class == PlanetClass::Asteroids && mass_mearth < BELT_MASS_LIMIT_MEARTH
}

/// True if the body is drawn as a belt rather than as a lone sphere.
pub fn is_belt(body: &Body) -> bool {
    is_belt_sized(body.class, body.mass_mearth)
}

/// One drawn rock: an orbit and a shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rock {
    pub a_au: f64,
    pub e: f64,
    pub i_rad: f64,
    pub long_asc_node_rad: f64,
    pub arg_periapsis_rad: f64,
    pub mean_anomaly_rad: f64,
    /// Relative size, 0.5 to 2.
    pub size: f32,
    /// Semi-axes of the rock's ellipsoid relative to `size`.
    pub shape: [f32; 3],
    pub spin: [f32; 3],
}

/// Where the snapshot has belts to draw: `(host, seed, inner_au, outer_au)`.
pub fn belt_zones(snapshot: &SystemSnapshot) -> Vec<(HostRef, u64, f64, f64)> {
    let mut zones: Vec<(HostRef, u64, f64, f64)> = snapshot
        .bodies
        .iter()
        .filter(|b| is_belt(b))
        .map(|b| {
            (
                b.host,
                b.visual.texture_seed,
                b.orbit.a_au * (1.0 - BODY_BELT_HALF_WIDTH),
                b.orbit.a_au * (1.0 + BODY_BELT_HALF_WIDTH),
            )
        })
        .collect();
    zones.extend(
        snapshot
            .belts
            .iter()
            .enumerate()
            .map(|(index, belt)| (belt.host, index as u64 + 1, belt.inner_au, belt.outer_au)),
    );
    zones
}

/// `count` rocks with semi-major axes between `inner_au` and `outer_au`,
/// the same for the same `seed`.
pub fn rocks(seed: u64, inner_au: f64, outer_au: f64, count: usize) -> Vec<Rock> {
    let mut rng = RngStream::new(seed, ModuleId::VISUAL);
    (0..count)
        .map(|_| {
            // Mean of two draws: denser toward the middle of the belt.
            let across = 0.5 * (rng.uniform() + rng.uniform());
            // Squaring favours small values: mostly near-circular, flat orbits.
            let e = MAX_ROCK_ECCENTRICITY * rng.uniform() * rng.uniform();
            let i_rad = MAX_ROCK_INCLINATION * rng.uniform() * rng.uniform();
            // Many small rocks, few large ones.
            let size = 0.5 + 1.5 * (rng.uniform() as f32).powi(3);
            let mut axis = || 0.55 + 0.45 * rng.uniform() as f32;
            let shape = [axis(), axis(), axis()];
            let mut angle = || rng.uniform_range(0.0, TAU);
            Rock {
                a_au: inner_au + (outer_au - inner_au) * across,
                e,
                i_rad,
                long_asc_node_rad: angle(),
                arg_periapsis_rad: angle(),
                mean_anomaly_rad: angle(),
                size,
                shape,
                spin: [angle() as f32, angle() as f32, angle() as f32],
            }
        })
        .collect()
}

#[derive(Component)]
pub(crate) struct BeltRock {
    host: HostRef,
    rock: Rock,
}

#[derive(Resource, Default)]
pub(crate) struct BeltEntities {
    generation: u64,
    quality: crate::Quality,
    rocks: Vec<Entity>,
}

/// Spawns the rocks of a new system and moves them along their orbits.
/// Belts belong to the finished system, so they are hidden during formation
/// playback.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sync_belts(
    mut commands: Commands,
    view: Res<SystemView>,
    playback: Res<Playback>,
    settings: Res<ViewSettings>,
    orbit: Res<OrbitCamera>,
    mut entities: ResMut<BeltEntities>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut rocks_query: Query<(&BeltRock, &mut Transform, &mut Visibility)>,
) {
    let Some(result) = view.result() else {
        return;
    };
    let snapshot = &result.snapshot;

    if entities.generation != view.generation() || entities.quality != settings.quality {
        entities.quality = settings.quality;
        for entity in std::mem::take(&mut entities.rocks) {
            commands.entity(entity).despawn();
        }
        entities.generation = view.generation();
        let zones = belt_zones(&result.snapshot);
        if !zones.is_empty() {
            let mesh = meshes.add(Sphere::new(1.0).mesh().uv(8, 6));
            let material = materials.add(StandardMaterial {
                base_color: crate::palette::BELT_ROCK_COLOR,
                perceptual_roughness: 1.0,
                ..default()
            });
            for (host, seed, inner_au, outer_au) in zones {
                for rock in rocks(seed, inner_au, outer_au, settings.quality.rocks_per_belt()) {
                    let entity = commands
                        .spawn((
                            BeltRock { host, rock },
                            Mesh3d(mesh.clone()),
                            MeshMaterial3d(material.clone()),
                            Transform::default(),
                            Visibility::Hidden,
                        ))
                        .id();
                    entities.rocks.push(entity);
                }
            }
        }
    }

    let finished = playback.t_myr >= view.timeline_end_myr();
    let eye = orbit.eye();
    let layout = Layout::at(&snapshot.hierarchy, &settings.scale, playback.orbit_days);
    for (belt_rock, mut transform, mut visibility) in &mut rocks_query {
        let wanted = if finished {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
        if !finished {
            continue;
        }
        let rock = belt_rock.rock;
        let (Some(centre), Some(host_mass)) = (
            layout.host(belt_rock.host),
            layout::host_mass(snapshot, belt_rock.host),
        ) else {
            continue;
        };
        let period = kepler::orbital_period(Au(rock.a_au), SolarMass(host_mass)).0;
        let position = orbit.local(
            centre
                + layout::map_offset(
                    &settings.scale,
                    kepler::position(Elements {
                        a: Au(rock.a_au),
                        e: rock.e,
                        i: rock.i_rad,
                        long_asc_node: rock.long_asc_node_rad,
                        arg_periapsis: rock.arg_periapsis_rad,
                        mean_anomaly: rock.mean_anomaly_rad + TAU * (playback.orbit_days / period),
                    }),
                ),
        );
        let radius = scene::apparent_floor(eye, position) * ROCK_APPARENT_SIZE * rock.size;
        *transform = Transform {
            translation: position,
            rotation: Quat::from_euler(EulerRot::XYZ, rock.spin[0], rock.spin[1], rock.spin[2]),
            scale: Vec3::from(rock.shape) * radius,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sg_schema::{Belt, HostRef, SimulationParams, StarId};

    fn system(seed: u64) -> sg_sim::SimulationResult {
        sg_sim::simulate(&SimulationParams {
            seed,
            ..SimulationParams::default()
        })
        .unwrap()
    }

    #[test]
    fn rocks_repeat_and_stay_inside_the_belt() {
        let a = rocks(7, 2.1, 3.3, 500);
        assert_eq!(a, rocks(7, 2.1, 3.3, 500));
        assert_ne!(a, rocks(8, 2.1, 3.3, 500));
        for rock in &a {
            assert!((2.1..=3.3).contains(&rock.a_au));
            assert!((0.0..=MAX_ROCK_ECCENTRICITY).contains(&rock.e));
            assert!((0.0..=MAX_ROCK_INCLINATION).contains(&rock.i_rad));
            assert!((0.5..=2.0).contains(&rock.size));
        }
        // Spread around the star, not bunched: every quadrant is populated.
        for quadrant in 0..4 {
            let lo = f64::from(quadrant) * TAU / 4.0;
            let n = a
                .iter()
                .filter(|r| (lo..lo + TAU / 4.0).contains(&r.mean_anomaly_rad))
                .count();
            assert!(n > 80, "quadrant {quadrant} has {n}");
        }
    }

    /// A heavy body under the label is a planet; a light one is a belt.
    #[test]
    fn only_light_asteroid_bodies_become_belts() {
        let mut result = system(42);
        result.snapshot.bodies[0].class = PlanetClass::Asteroids;
        result.snapshot.bodies[0].mass_mearth = 0.1;
        assert!(belt_zones(&result.snapshot).is_empty());

        result.snapshot.bodies[0].mass_mearth = 3e-4;
        let zones = belt_zones(&result.snapshot);
        assert_eq!(zones.len(), 1);
        let body = &result.snapshot.bodies[0];
        assert!(is_belt(body) && body.mass_mearth < BELT_MASS_LIMIT_MEARTH);
        let (_, _, inner, outer) = zones[0];
        assert!(inner < body.orbit.a_au && body.orbit.a_au < outer);
        assert!((outer - inner) / body.orbit.a_au < 0.2);
    }

    #[test]
    fn snapshot_belts_are_drawn_between_their_edges() {
        let mut result = system(42);
        result.snapshot.belts.push(Belt {
            host: HostRef::Star(StarId(0)),
            inner_au: 2.1,
            outer_au: 3.3,
            mass_mearth: 4.0e-4,
        });
        let zones = belt_zones(&result.snapshot);
        assert_eq!(zones.len(), 1);
        assert_eq!((zones[0].2, zones[0].3), (2.1, 3.3));
    }
}
