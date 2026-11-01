//! Planetary rings ([`ring.wesl`](../ring.wesl)).
//!
//! The snapshot says where a planet's rings begin and end and how opaque
//! they are (`sg_planet::moons::rings`: icy rings inside the Roche limit
//! of cold giants; which planets have them is drawn). They are drawn in
//! the planet's equatorial plane, at their true proportion to the planet
//! as drawn in every scale mode. The ringlets and gaps are texture.

use crate::palette;
use crate::planet;
use crate::scene::{BodyPlacements, Playback, SystemView, Target, ViewSettings};
use crate::surface;
use crate::OrbitCamera;
use bevy::asset::embedded_asset;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use sg_schema::{Body, BodyId};
use std::collections::BTreeMap;

/// Colour of sunlit ring ice, linear RGB: slightly warm, as Saturn's rings
/// are from the dust mixed into them.
const RING_COLOR: Vec3 = Vec3::new(0.86, 0.80, 0.68);
/// Brightness of a fully lit ring before tone mapping; an exposure
/// convention matching the planets'.
const EXPOSURE: f32 = 1.0;

/// The shader's uniform; see `ring.wesl`.
#[derive(Debug, Clone, Default, ShaderType)]
pub struct RingUniform {
    pub planet: Vec4,
    pub star_dir: Vec4,
    pub color: Vec4,
    pub eye: Vec4,
    pub normal: Vec4,
    pub shape: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct RingMaterial {
    #[uniform(0)]
    pub uniform: RingUniform,
}

impl Material for RingMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://sg_render/ring.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }
}

/// Inner and outer radius of a planet's rings in planetary radii, if it
/// has any.
pub fn extent_in_radii(body: &Body) -> Option<(f32, f32)> {
    let rings = body.rings.as_ref()?;
    let radius = body.radius_km;
    (radius > 0.0 && rings.outer_radius_km > rings.inner_radius_km).then(|| {
        (
            (rings.inner_radius_km / radius) as f32,
            (rings.outer_radius_km / radius) as f32,
        )
    })
}

#[derive(Component)]
pub(crate) struct RingVisual;

struct RingFaces {
    faces: [Entity; 2],
    material: Handle<RingMaterial>,
}

#[derive(Resource, Default)]
pub(crate) struct RingEntities {
    generation: u64,
    rings: BTreeMap<BodyId, RingFaces>,
}

/// Draws the rings of the finished system's planets. Like belts and
/// moons, rings belong to the finished system and are hidden during
/// formation playback.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sync_rings(
    mut commands: Commands,
    view: Res<SystemView>,
    playback: Res<Playback>,
    settings: Res<ViewSettings>,
    orbit: Res<OrbitCamera>,
    placements: Res<BodyPlacements>,
    mut entities: ResMut<RingEntities>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<RingMaterial>>,
    mut faces: Query<(&mut Transform, &mut Visibility), With<RingVisual>>,
) {
    let Some(result) = view.result() else {
        return;
    };
    if entities.generation != view.generation() {
        for (_, ring) in std::mem::take(&mut entities.rings) {
            for face in ring.faces {
                commands.entity(face).despawn();
            }
        }
        entities.generation = view.generation();
    }
    let finished = playback.t_myr >= view.timeline_end_myr();
    let _ = &settings;
    for body in result.snapshot.planets() {
        let Some((inner, outer)) = extent_in_radii(body) else {
            continue;
        };
        let placement = placements.get(Target::Body(body.id)).filter(|_| finished);
        let Some(placement) = placement else {
            if let Some(ring) = entities.rings.get(&body.id) {
                for face in ring.faces {
                    if let Ok((_, mut visibility)) = faces.get_mut(face) {
                        *visibility = Visibility::Hidden;
                    }
                }
            }
            continue;
        };
        let look = surface::appearance(body);
        let tilt = planet::axis_tilt(&look);
        let normal = tilt * Vec3::Y;
        // Lit by the nearest star.
        let star = placements
            .iter()
            .filter_map(|(target, star)| match target {
                Target::Star(id) => Some((id, star.position - placement.position)),
                Target::Body(_) | Target::Comet(_) => None,
            })
            .min_by(|a, b| a.1.length_squared().total_cmp(&b.1.length_squared()));
        let (star_dir, star_color) = star.map_or((Vec3::X, Vec3::ONE), |(id, to_star)| {
            (
                to_star.normalize_or(Vec3::X),
                view.star(id).map_or(Vec3::ONE, |s| {
                    palette::star_color(s.color_linear_rgb).to_vec3()
                }),
            )
        });
        let radius = placement.body_radius;
        let uniform = RingUniform {
            planet: placement.position.extend(radius),
            star_dir: star_dir.extend(0.0),
            color: (RING_COLOR * star_color).extend(EXPOSURE),
            eye: orbit.eye().extend(1.0),
            normal: normal.extend(0.0),
            shape: Vec4::new(
                inner * radius,
                outer * radius,
                body.rings.as_ref().map_or(0.0, |r| r.optical_depth as f32),
                look.seed,
            ),
        };
        // An annulus in the XY plane, laid in the ring plane: one face up,
        // one down.
        let placed = |turn: f32| {
            Transform::from_translation(placement.position)
                .with_rotation(tilt * Quat::from_rotation_x(turn))
                .with_scale(Vec3::splat(radius))
        };
        let (up, down) = (
            placed(-std::f32::consts::FRAC_PI_2),
            placed(std::f32::consts::FRAC_PI_2),
        );
        match entities.rings.get(&body.id) {
            Some(ring) => {
                if let Some(mut material) = materials.get_mut(&ring.material) {
                    material.uniform = uniform;
                }
                for (face, transform) in ring.faces.into_iter().zip([up, down]) {
                    if let Ok((mut current, mut visibility)) = faces.get_mut(face) {
                        *current = transform;
                        *visibility = Visibility::Visible;
                    }
                }
            }
            None => {
                let mesh = meshes.add(Annulus::new(inner, outer).mesh().resolution(128));
                let material = materials.add(RingMaterial { uniform });
                let mut spawn = |transform: Transform| {
                    commands
                        .spawn((
                            RingVisual,
                            Mesh3d(mesh.clone()),
                            MeshMaterial3d(material.clone()),
                            transform,
                            Visibility::Visible,
                        ))
                        .id()
                };
                let faces = [spawn(up), spawn(down)];
                entities
                    .rings
                    .insert(body.id, RingFaces { faces, material });
            }
        }
    }
}

pub(crate) fn build(app: &mut App) {
    embedded_asset!(app, "ring.wesl");
    app.add_plugins(MaterialPlugin::<RingMaterial>::default())
        .init_resource::<RingEntities>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use sg_schema::{Rings, SimulationParams};

    #[test]
    fn ring_extent_is_in_planetary_radii() {
        let mut snapshot = sg_sim::simulate(&SimulationParams {
            seed: 42,
            ..SimulationParams::default()
        })
        .expect("classic run")
        .snapshot;
        let body = &mut snapshot.bodies[10];
        body.rings = None;
        assert_eq!(extent_in_radii(body), None);
        body.rings = Some(Rings {
            inner_radius_km: 1.2 * body.radius_km,
            outer_radius_km: 2.3 * body.radius_km,
            optical_depth: 0.8,
        });
        let (inner, outer) = extent_in_radii(body).unwrap();
        assert!((inner - 1.2).abs() < 1e-6 && (outer - 2.3).abs() < 1e-6);
        // Nonsense rings are not drawn.
        body.rings = Some(Rings {
            inner_radius_km: 3.0 * body.radius_km,
            outer_radius_km: 2.0 * body.radius_km,
            optical_depth: 0.8,
        });
        assert_eq!(extent_in_radii(body), None);
    }

    #[test]
    fn some_classic_giants_have_rings_to_draw() {
        let ringed = (1..60u64)
            .flat_map(|seed| {
                sg_sim::simulate(&SimulationParams {
                    seed,
                    ..SimulationParams::default()
                })
                .expect("classic run")
                .snapshot
                .bodies
            })
            .filter(|body| extent_in_radii(body).is_some())
            .count();
        assert!(ringed > 0);
    }
}
