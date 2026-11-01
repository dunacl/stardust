//! Comets: a nucleus, a coma and two tails ([`tail.wesl`](../tail.wesl)).
//!
//! The snapshot gives each comet's orbit and nucleus size. From these, and
//! physically based: the coma and tails appear as the comet nears its star,
//! inside the distance water ice sublimes, and the tails point away from
//! the star, the dust tail bending back along the orbit. Drawing choices:
//! how long and bright the tails are, and their colours. Nuclei are drawn
//! far larger than they are, or they could not be seen at all.

use crate::layout::{self, Layout};
use crate::scene::{self, BodyPlacements, Placement, Playback, SystemView, Target, ViewSettings};
use crate::OrbitCamera;
use bevy::asset::embedded_asset;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use sg_core::units::Au;
use sg_dynamics::comets::visible_perihelion_au;
use sg_dynamics::kepler::{self, Elements};
#[cfg(test)]
use sg_schema::HostRef;
use sg_schema::{Comet, CometFamily, CometId};
use std::f64::consts::TAU;

/// Longest tail, as a fraction of the comet's drawn distance from its star.
const TAIL_FRACTION: f32 = 0.45;
/// Half the width of the far end of a tail, as a fraction of its length.
const TAIL_WIDTH: f32 = 0.06;
/// How wide the dust tail is, as a multiple of the ion tail.
const DUST_WIDTH: f32 = 1.6;
/// How strongly the dust tail bends back along the orbit, 0 to 1. Dust
/// feels less of the star's light than gas feels its wind.
const DUST_LAG: f32 = 0.35;
/// Radius of the coma, as a fraction of the longest tail.
const COMA_FRACTION: f32 = 0.09;
/// The ion tail: blue, from ionised carbon monoxide.
const ION_COLOR: Vec3 = Vec3::new(0.35, 0.55, 1.0);
/// The dust tail: yellowish, sunlight scattered by grains.
const DUST_COLOR: Vec3 = Vec3::new(1.0, 0.86, 0.62);
const COMA_COLOR: Vec3 = Vec3::new(0.75, 0.88, 1.0);
const ORBIT_COLOR: Color = Color::srgba(0.55, 0.75, 0.95, 0.22);
/// Size of a nucleus on the screen, as a multiple of the smallest drawn.
const NUCLEUS_APPARENT_SIZE: f32 = 0.5;
/// Points along an orbit line.
const ORBIT_POINTS: usize = 96;

/// How active a comet is at `r_au`, 0 to 1: nothing beyond twice the
/// distance at which it counts as visible (`sg_dynamics::comets`), which is
/// about 5 AU at the Sun, where water ice starts to sublime strongly
/// (Whipple 1950), rising to 1 at the star.
pub fn activity(r_au: f64, luminosity_lsun: f64) -> f32 {
    let limit = 2.0 * visible_perihelion_au(luminosity_lsun);
    let x = (1.0 - r_au / limit).clamp(0.0, 1.0) as f32;
    x * x
}

/// Direction of the ion tail: straight away from the star.
pub fn ion_direction(away_from_star: Vec3) -> Vec3 {
    away_from_star.normalize_or(Vec3::X)
}

/// Direction of the dust tail: away from the star, bent behind the comet's
/// motion, in the plane of the orbit.
pub fn dust_direction(away_from_star: Vec3, velocity: Vec3) -> Vec3 {
    let away = away_from_star.normalize_or(Vec3::X);
    let behind = -velocity.normalize_or_zero();
    (away * (1.0 - DUST_LAG) + behind * DUST_LAG).normalize_or(away)
}

fn elements(comet: &Comet, orbit_days: f64) -> Elements {
    let orbit = &comet.orbit;
    Elements {
        a: Au(orbit.a_au),
        e: orbit.e,
        i: orbit.i_deg.to_radians(),
        long_asc_node: orbit.long_asc_node_deg.to_radians(),
        arg_periapsis: orbit.arg_periapsis_deg.to_radians(),
        mean_anomaly: orbit.mean_anomaly_deg.to_radians() + TAU * (orbit_days / orbit.period_days),
    }
}

/// The shader's uniform; see `tail.wesl`.
#[derive(Debug, Clone, Default, ShaderType)]
pub struct TailUniform {
    pub origin: Vec4,
    pub axis: Vec4,
    pub side: Vec4,
    pub color: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct TailMaterial {
    #[uniform(0)]
    pub uniform: TailUniform,
}

impl Material for TailMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://sg_render/tail.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }
}

#[derive(Component)]
pub(crate) struct CometPart {
    id: CometId,
    part: Part,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Nucleus,
    Coma,
    Ion,
    Dust,
}

#[derive(Resource, Default)]
pub(crate) struct CometEntities {
    generation: u64,
    parts: Vec<(Entity, Option<Handle<TailMaterial>>)>,
}

/// Turns a unit quad so that its long side runs along `axis` from `origin`
/// for `length` and it faces the eye.
fn quad_transform(origin: Vec3, axis: Vec3, side: Vec3, length: f32, half_width: f32) -> Transform {
    let normal = side.cross(axis).normalize_or(Vec3::Z);
    Transform {
        translation: origin + axis * (0.5 * length),
        rotation: Quat::from_mat3(&Mat3::from_cols(side, axis, normal)),
        // Wide enough for the tail's soft edge.
        scale: Vec3::new(2.0 * half_width, length, 1.0),
    }
}

/// Spawns a new system's comets, moves them, and offers them for selection.
/// They belong to the finished system, like the belts.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sync_comets(
    mut commands: Commands,
    view: Res<SystemView>,
    settings: Res<ViewSettings>,
    playback: Res<Playback>,
    orbit: Res<OrbitCamera>,
    mut entities: ResMut<CometEntities>,
    mut placements: ResMut<BodyPlacements>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut tails: ResMut<Assets<TailMaterial>>,
    mut parts: Query<(&CometPart, &mut Transform, &mut Visibility)>,
) {
    placements.clear_comets();
    let Some(result) = view.result() else {
        return;
    };
    let snapshot = &result.snapshot;
    if entities.generation != view.generation() {
        for (entity, _) in std::mem::take(&mut entities.parts) {
            commands.entity(entity).despawn();
        }
        entities.generation = view.generation();
        let quad = meshes.add(Rectangle::new(1.0, 1.0));
        let ball = meshes.add(Sphere::new(1.0).mesh().ico(2).expect("a low subdivision"));
        let rock = standard.add(StandardMaterial {
            base_color: Color::srgb(0.32, 0.30, 0.28),
            perceptual_roughness: 1.0,
            ..default()
        });
        for comet in &snapshot.comets {
            let nucleus = commands
                .spawn((
                    CometPart {
                        id: comet.id,
                        part: Part::Nucleus,
                    },
                    Mesh3d(ball.clone()),
                    MeshMaterial3d(rock.clone()),
                    Transform::default(),
                    Visibility::Hidden,
                ))
                .id();
            entities.parts.push((nucleus, None));
            for part in [Part::Coma, Part::Ion, Part::Dust] {
                let material = tails.add(TailMaterial::default());
                let entity = commands
                    .spawn((
                        CometPart { id: comet.id, part },
                        Mesh3d(quad.clone()),
                        MeshMaterial3d(material.clone()),
                        Transform::default(),
                        Visibility::Hidden,
                    ))
                    .id();
                entities.parts.push((entity, Some(material)));
            }
        }
    }

    let finished = playback.t_myr >= view.timeline_end_myr() && settings.show_comets;
    let eye = orbit.eye();
    let layout = Layout::at(&snapshot.hierarchy, &settings.scale, playback.orbit_days);
    let handles: Vec<Option<Handle<TailMaterial>>> =
        entities.parts.iter().map(|(_, m)| m.clone()).collect();
    for (index, (entity, _)) in entities.parts.iter().enumerate() {
        let Ok((part, mut transform, mut visibility)) = parts.get_mut(*entity) else {
            continue;
        };
        let comet = snapshot.comets.iter().find(|c| c.id == part.id);
        let drawn = comet.filter(|_| finished).and_then(|comet| {
            let place = |days: f64| -> Option<Vec3> {
                let centre = layout.host(comet.host)?;
                let at = kepler::position(elements(comet, days));
                Some(orbit.local(centre + layout::map_offset(&settings.scale, at)))
            };
            let star = orbit.local(layout.host(comet.host)?);
            Some((
                comet,
                place(playback.orbit_days)?,
                place(playback.orbit_days + 1.0)?,
                star,
            ))
        });
        let Some((comet, position, ahead, star)) = drawn else {
            *visibility = Visibility::Hidden;
            continue;
        };
        *visibility = Visibility::Visible;
        let r_au = {
            let [x, y, z] = kepler::position(elements(comet, playback.orbit_days));
            (x * x + y * y + z * z).sqrt()
        };
        let light = layout::host_luminosity(snapshot, comet.host).unwrap_or(1.0);
        let active = activity(r_au, light);
        let away = position - star;
        let longest = away.length() * TAIL_FRACTION * active;
        let floor = scene::apparent_floor(eye, position);
        let radius = floor * NUCLEUS_APPARENT_SIZE;
        if part.part == Part::Nucleus {
            *transform = Transform {
                translation: position,
                scale: Vec3::splat(radius),
                ..default()
            };
            if placements.has_no(Target::Comet(comet.id)) {
                placements.insert(
                    Target::Comet(comet.id),
                    Placement {
                        position,
                        radius: floor,
                        body_radius: radius,
                        focus_radius: longest.max(radius * 3.0) * 0.3,
                        moon: false,
                    },
                );
            }
            continue;
        }
        if longest <= 0.0 {
            *visibility = Visibility::Hidden;
            continue;
        }
        let to_eye = (eye - position).normalize_or(Vec3::Z);
        let (axis, length, width, color, coma) = match part.part {
            Part::Coma => (to_eye, longest * COMA_FRACTION, 0.0, COMA_COLOR, true),
            Part::Ion => (
                ion_direction(away),
                longest,
                longest * TAIL_WIDTH,
                ION_COLOR,
                false,
            ),
            _ => (
                dust_direction(away, ahead - position),
                longest * 0.7,
                longest * 0.7 * TAIL_WIDTH * DUST_WIDTH,
                DUST_COLOR,
                false,
            ),
        };
        let brightness = 0.25 + 1.5 * active;
        let (side, placed) = if coma {
            let up = to_eye.any_orthonormal_vector();
            let across = to_eye.cross(up);
            (
                across,
                Transform {
                    translation: position,
                    rotation: Quat::from_mat3(&Mat3::from_cols(across, up, to_eye)),
                    scale: Vec3::new(2.0 * length, 2.0 * length, 1.0),
                },
            )
        } else {
            let side = axis.cross(to_eye).normalize_or(Vec3::X);
            (
                side,
                quad_transform(position, axis, side, length, 2.0 * width),
            )
        };
        *transform = placed;
        if let Some(handle) = &handles[index] {
            if let Some(mut material) = tails.get_mut(handle) {
                material.uniform = TailUniform {
                    origin: position.extend(length),
                    axis: axis.extend(if coma { 1.0 } else { 0.0 }),
                    side: side.extend(width.max(1e-6)),
                    color: color.extend(brightness),
                };
            }
        }
    }
}

/// Draws the orbits of the short-period comets, and of a selected comet.
pub(crate) fn draw_comet_orbits(
    mut gizmos: Gizmos,
    view: Res<SystemView>,
    settings: Res<ViewSettings>,
    playback: Res<Playback>,
    orbit: Res<OrbitCamera>,
    selection: Res<scene::Selection>,
) {
    let Some(result) = view.result() else {
        return;
    };
    if !settings.show_comets || !settings.show_orbits || playback.t_myr < view.timeline_end_myr() {
        return;
    }
    let snapshot = &result.snapshot;
    let layout = Layout::at(&snapshot.hierarchy, &settings.scale, playback.orbit_days);
    for comet in &snapshot.comets {
        let selected = selection.0 == Some(Target::Comet(comet.id));
        if comet.family == CometFamily::LongPeriod && !selected {
            continue;
        }
        let Some(centre) = layout.host(comet.host) else {
            continue;
        };
        let el = elements(comet, 0.0);
        let points = (0..=ORBIT_POINTS).map(|i| {
            let big_e = TAU * i as f64 / ORBIT_POINTS as f64;
            let at = kepler::point_at_eccentric_anomaly(el, big_e);
            orbit.local(centre + layout::map_offset(&settings.scale, at))
        });
        gizmos.linestrip(points, ORBIT_COLOR);
    }
}

pub(crate) fn build(app: &mut App) {
    embedded_asset!(app, "tail.wesl");
    app.add_plugins(MaterialPlugin::<TailMaterial>::default())
        .init_resource::<CometEntities>();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comets_wake_near_their_star() {
        assert_eq!(activity(6.0, 1.0), 0.0);
        assert!(activity(1.0, 1.0) > activity(3.0, 1.0));
        assert!(activity(3.0, 1.0) > 0.0);
        assert!(activity(0.0, 1.0) <= 1.0);
        // A brighter star wakes them farther out.
        assert!(activity(6.0, 4.0) > 0.0);
    }

    #[test]
    fn tails_point_away_from_the_star() {
        let away = Vec3::new(3.0, 0.0, 0.0);
        assert_eq!(ion_direction(away), Vec3::X);
        let dust = dust_direction(away, Vec3::Z);
        assert!(dust.dot(Vec3::X) > 0.8);
        assert!(dust.z < 0.0, "behind the motion: {dust}");
        assert!((dust.length() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn a_quad_runs_along_its_axis_from_the_origin() {
        let t = quad_transform(Vec3::ONE, Vec3::Y, Vec3::X, 4.0, 0.5);
        let far = t.transform_point(Vec3::new(0.0, 0.5, 0.0));
        let near = t.transform_point(Vec3::new(0.0, -0.5, 0.0));
        assert!((far - Vec3::new(1.0, 5.0, 1.0)).length() < 1e-5);
        assert!((near - Vec3::ONE).length() < 1e-5);
    }

    #[test]
    fn long_period_orbits_stay_finite_next_to_perihelion() {
        let comet = Comet {
            id: CometId(0),
            name: "x".into(),
            designation: "C/x".into(),
            host: HostRef::Star(sg_schema::StarId(0)),
            family: CometFamily::LongPeriod,
            orbit: sg_schema::Orbit {
                a_au: 20_000.0,
                e: 1.0 - 1.0 / 20_000.0,
                i_deg: 120.0,
                long_asc_node_deg: 10.0,
                arg_periapsis_deg: 20.0,
                mean_anomaly_deg: 1e-5,
                period_days: 365.25 * 2.8e6,
            },
            diameter_km: 5.0,
        };
        let [x, y, z] = kepler::position(elements(&comet, 0.0));
        let r = (x * x + y * y + z * z).sqrt();
        assert!((0.99..1000.0).contains(&r), "{r}");
    }
}
