//! The 3D scene: star, bodies, orbit lines and formation highlights. Belts
//! are in [`crate::belt`] and the sky in [`crate::sky`].

use crate::camera::{self, OrbitCamera, PointerCaptured};
use crate::display::{self, DisplayBody, Highlight};
use crate::palette;
use crate::scale::ScaleBlend;
use crate::{belt, sky};
use bevy::light::GlobalAmbientLight;
use bevy::prelude::*;
use sg_core::units::{Au, SolarMass};
use sg_dynamics::kepler::{self, Elements};
use sg_schema::{BodyId, HierarchyNode, PlanetClass, Star};
use sg_sim::SimulationResult;
use std::collections::BTreeMap;
use std::f64::consts::TAU;

/// Something that can be selected and followed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Target {
    Star,
    Body(BodyId),
}

/// The selected object, if any.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Selection(pub Option<Target>);

/// The simulation result on display.
#[derive(Resource, Default)]
pub struct SystemView {
    result: Option<SimulationResult>,
    generation: u64,
}

impl SystemView {
    /// Shows a new system.
    pub fn set(&mut self, result: SimulationResult) {
        self.result = Some(result);
        self.generation += 1;
    }

    pub fn result(&self) -> Option<&SimulationResult> {
        self.result.as_ref()
    }

    /// Changes every time [`Self::set`] is called.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The star of a single-star system.
    pub fn star(&self) -> Option<&Star> {
        match &self.result.as_ref()?.snapshot.hierarchy {
            HierarchyNode::Star(star) => Some(star),
            HierarchyNode::Pair { .. } => None,
        }
    }

    /// End of the formation timeline, Myr.
    pub fn timeline_end_myr(&self) -> f64 {
        self.result.as_ref().map_or(0.0, display::timeline_end_myr)
    }
}

/// Playback state. Two independent clocks: formation time, which moves
/// along the timeline, and the orbital clock, which moves bodies around
/// their orbits. (Formation takes millions of years; at that rate orbital
/// motion would be a blur, so the two are decoupled.)
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct Playback {
    /// Position on the formation timeline.
    pub t_myr: f64,
    pub playing: bool,
    pub myr_per_second: f64,
    /// Orbital clock.
    pub orbit_days: f64,
    pub orbits_running: bool,
    pub days_per_second: f64,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            t_myr: 0.0,
            playing: false,
            myr_per_second: 1.0,
            orbit_days: 0.0,
            orbits_running: true,
            days_per_second: 30.0,
        }
    }
}

/// What to draw and at what scale.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct ViewSettings {
    pub scale: ScaleBlend,
    pub show_orbits: bool,
    pub show_habitable_zone: bool,
    pub show_formation_highlights: bool,
    /// The sky behind the system ([`crate::sky`]): scenery, not simulation.
    pub show_backdrop: bool,
    /// Names beside the star and planets. Drawn by the app.
    pub show_labels: bool,
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self {
            scale: ScaleBlend::default(),
            show_orbits: true,
            show_habitable_zone: true,
            show_formation_highlights: true,
            show_backdrop: true,
            show_labels: true,
        }
    }
}

/// Where something is drawn this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub position: Vec3,
    /// Radius as drawn, including the minimum apparent size.
    pub radius: f32,
    /// Radius the scale mode assigns, before the minimum apparent size.
    pub body_radius: f32,
    /// Size of what the camera should frame when this is selected: the body
    /// itself, or for a belt a stretch of the belt.
    pub focus_radius: f32,
}

/// Scene position and drawn radius of everything selectable, refreshed
/// every frame.
#[derive(Resource, Debug, Default)]
pub struct BodyPlacements(BTreeMap<Target, Placement>);

impl BodyPlacements {
    pub fn get(&self, target: Target) -> Option<Placement> {
        self.0.get(&target).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (Target, Placement)> + '_ {
        self.0.iter().map(|(t, p)| (*t, *p))
    }
}

#[derive(Component)]
struct BodyVisual {
    class: Option<PlanetClass>,
}

#[derive(Component)]
struct StarVisual;

#[derive(Component)]
struct StarLight;

#[derive(Resource)]
struct SceneAssets {
    sphere: Handle<Mesh>,
}

#[derive(Resource, Default)]
struct SceneEntities {
    generation: u64,
    bodies: BTreeMap<BodyId, Entity>,
    star: Option<Entity>,
}

/// Bodies never shrink below this fraction of their distance from the
/// camera, so they stay visible (as dots) at any zoom. At true scale this is
/// what you see until you are very close.
const MIN_APPARENT_RADIUS: f32 = 0.0035;
const ORBIT_SEGMENTS: usize = 128;
/// Illuminance the star gives the body being followed, and the reference
/// distance of the overview. Exposure conventions, not physics.
const FOLLOWED_LUX: f32 = 20_000.0;
const OVERVIEW_LUX: f32 = 2200.0;
/// How long a formation highlight lingers, as a fraction of the timeline.
const HIGHLIGHT_LOOKBACK: f64 = 0.02;

/// Smallest radius at which something at `position` is drawn, seen from
/// `eye`.
pub(crate) fn apparent_floor(eye: Vec3, position: Vec3) -> f32 {
    (position - eye).length() * MIN_APPARENT_RADIUS
}

/// Simulation frame (z up, AU) to scene frame (y up).
fn to_scene(p: [f64; 3]) -> Vec3 {
    Vec3::new(p[0] as f32, p[2] as f32, -p[1] as f32)
}

/// Applies the scale mode's radial mapping to a position in AU.
pub(crate) fn map_position(scale: &ScaleBlend, p: [f64; 3]) -> Vec3 {
    let v = to_scene(p);
    let r = v.length();
    if r <= f32::EPSILON {
        return Vec3::ZERO;
    }
    v * (scale.distance(r) / r)
}

fn elements(body: &DisplayBody, star_mass_msun: f64, orbit_days: f64) -> Elements {
    let period = kepler::orbital_period(Au(body.a_au), SolarMass(star_mass_msun)).0;
    Elements {
        a: Au(body.a_au),
        e: body.e,
        i: 0.0,
        long_asc_node: body.long_asc_node_deg.to_radians(),
        arg_periapsis: body.arg_periapsis_deg.to_radians(),
        mean_anomaly: body.mean_anomaly_deg.to_radians() + TAU * (orbit_days / period),
    }
}

/// Scene radius that fits the finished system: its outermost apoapsis.
pub fn system_extent(result: &SimulationResult, scale: &ScaleBlend) -> f32 {
    let outermost = result
        .snapshot
        .bodies
        .iter()
        .map(|b| b.orbit.a_au * (1.0 + b.orbit.e))
        .fold(1.0f64, f64::max);
    scale.distance(outermost as f32)
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(SceneAssets {
        sphere: meshes.add(Sphere::new(1.0).mesh().uv(48, 24)),
    });
    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: 120.0,
        ..default()
    });
    commands.spawn((
        StarLight,
        PointLight {
            range: 1.0e5,
            ..default()
        },
        Transform::default(),
    ));
}

fn advance(
    time: Res<Time>,
    view: Res<SystemView>,
    mut playback: ResMut<Playback>,
    mut settings: ResMut<ViewSettings>,
) {
    let dt = time.delta_secs();
    let end = view.timeline_end_myr();
    if playback.playing {
        playback.t_myr += f64::from(dt) * playback.myr_per_second;
        if playback.t_myr >= end {
            playback.t_myr = end;
            playback.playing = false;
        }
    }
    playback.t_myr = playback.t_myr.clamp(0.0, end);
    if playback.orbits_running {
        playback.orbit_days += f64::from(dt) * playback.days_per_second;
    }
    if !settings.scale.is_settled() {
        settings.scale.advance(dt);
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_scene(
    mut commands: Commands,
    view: Res<SystemView>,
    playback: Res<Playback>,
    settings: Res<ViewSettings>,
    orbit: Res<OrbitCamera>,
    assets: Res<SceneAssets>,
    mut entities: ResMut<SceneEntities>,
    mut placements: ResMut<BodyPlacements>,
    mut selection: ResMut<Selection>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut bodies: Query<
        (
            &mut Transform,
            &mut BodyVisual,
            &MeshMaterial3d<StandardMaterial>,
        ),
        Without<StarVisual>,
    >,
    mut stars: Query<&mut Transform, (With<StarVisual>, Without<BodyVisual>)>,
    mut light: Single<&mut PointLight, With<StarLight>>,
) {
    placements.0.clear();
    let (Some(result), Some(star)) = (view.result(), view.star()) else {
        return;
    };

    // A new system: start from an empty scene.
    if entities.generation != view.generation() {
        for (_, entity) in std::mem::take(&mut entities.bodies) {
            commands.entity(entity).despawn();
        }
        if let Some(entity) = entities.star.take() {
            commands.entity(entity).despawn();
        }
        entities.generation = view.generation();
    }

    let scale = &settings.scale;
    let eye = orbit.eye();
    let apparent_floor = |position: Vec3| apparent_floor(eye, position);

    // The star.
    let star_color = palette::star_color(star.color_linear_rgb);
    let star_body_radius = scale.star_radius(star.radius_rsun as f32);
    let star_radius = star_body_radius.max(apparent_floor(Vec3::ZERO) * 1.6);
    match entities.star {
        Some(entity) => {
            if let Ok(mut transform) = stars.get_mut(entity) {
                transform.scale = Vec3::splat(star_radius);
            }
        }
        None => {
            let material = materials.add(StandardMaterial {
                base_color: Color::BLACK,
                emissive: star_color * 40.0,
                ..default()
            });
            let entity = commands
                .spawn((
                    StarVisual,
                    Mesh3d(assets.sphere.clone()),
                    MeshMaterial3d(material),
                    Transform::from_scale(Vec3::splat(star_radius)),
                ))
                .id();
            entities.star = Some(entity);
        }
    }
    placements.0.insert(
        Target::Star,
        Placement {
            position: Vec3::ZERO,
            radius: star_radius,
            body_radius: star_body_radius,
            focus_radius: star_body_radius,
        },
    );

    // The bodies.
    let shown = display::bodies_at(result, playback.t_myr);
    let mut stale: Vec<BodyId> = entities.bodies.keys().copied().collect();
    for body in &shown {
        stale.retain(|id| *id != body.id);
        let position = map_position(
            scale,
            kepler::position(elements(body, star.mass_msun, playback.orbit_days)),
        );
        let presence = body.presence.clamp(0.0, 1.0);
        let body_radius = scale.planet_radius(body.radius_km as f32);
        let radius = body_radius.max(apparent_floor(position)) * presence.max(0.02);
        let transform = Transform::from_translation(position).with_scale(Vec3::splat(radius));
        let color = body
            .class
            .map_or(palette::EMBRYO_COLOR, palette::class_color);

        match entities.bodies.get(&body.id) {
            Some(&entity) => {
                if let Ok((mut current, mut visual, material)) = bodies.get_mut(entity) {
                    *current = transform;
                    if visual.class != body.class {
                        visual.class = body.class;
                        if let Some(mut material) = materials.get_mut(&material.0) {
                            material.base_color = color;
                        }
                    }
                }
            }
            None => {
                let material = materials.add(StandardMaterial {
                    base_color: color,
                    perceptual_roughness: 0.9,
                    ..default()
                });
                let entity = commands
                    .spawn((
                        BodyVisual { class: body.class },
                        Mesh3d(assets.sphere.clone()),
                        MeshMaterial3d(material),
                        transform,
                    ))
                    .id();
                entities.bodies.insert(body.id, entity);
            }
        }
        placements.0.insert(
            Target::Body(body.id),
            Placement {
                position,
                radius,
                body_radius,
                focus_radius: match body.class {
                    Some(class) if belt::is_belt_sized(class, body.mass_mearth) => {
                        position.length() * belt::SELECTED_VIEW_FRACTION
                    }
                    _ => body_radius,
                },
            },
        );
    }
    for id in stale {
        if let Some(entity) = entities.bodies.remove(&id) {
            commands.entity(entity).despawn();
        }
        // A selected planet stays selected while playback is before its
        // birth; anything not in the finished system is deselected.
        let survives = result.snapshot.bodies.iter().any(|b| b.id == id);
        if selection.0 == Some(Target::Body(id)) && !survives {
            selection.0 = None;
        }
    }

    // Light the scene so that whatever the camera is looking at is well
    // exposed (a point light of Φ lumens gives Φ/4πd² lux at distance d):
    // `FOLLOWED_LUX` at the body being followed, or `OVERVIEW_LUX` a third
    // of the way out when the whole system is in view. Real sunlight falls
    // off as 1/d²; a fixed physical brightness would leave either the inner
    // or the outer system unreadable.
    let extent = system_extent(result, scale);
    let (lux, reference) = match selection.0.and_then(|target| placements.0.get(&target)) {
        Some(followed) if followed.position.length() > star_radius => {
            (FOLLOWED_LUX, followed.position.length())
        }
        _ => (OVERVIEW_LUX, 0.3 * extent),
    };
    light.color = Color::LinearRgba(star_color);
    light.intensity = lux * 4.0 * std::f32::consts::PI * reference * reference;
}

fn ring(gizmos: &mut Gizmos, radius: f32, color: Color) {
    let points = (0..=ORBIT_SEGMENTS).map(|k| {
        let angle = std::f32::consts::TAU * k as f32 / ORBIT_SEGMENTS as f32;
        Vec3::new(radius * angle.cos(), 0.0, radius * angle.sin())
    });
    gizmos.linestrip(points, color);
}

fn draw_overlays(
    mut gizmos: Gizmos,
    view: Res<SystemView>,
    playback: Res<Playback>,
    settings: Res<ViewSettings>,
    selection: Res<Selection>,
) {
    let (Some(result), Some(star)) = (view.result(), view.star()) else {
        return;
    };
    let scale = &settings.scale;

    if settings.show_habitable_zone {
        for hz in &result.snapshot.habitable_zones {
            for (au, color) in [
                (hz.conservative_inner_au, palette::HZ_COLOR),
                (hz.conservative_outer_au, palette::HZ_COLOR),
                (hz.optimistic_inner_au, palette::HZ_OPTIMISTIC_COLOR),
                (hz.optimistic_outer_au, palette::HZ_OPTIMISTIC_COLOR),
            ] {
                ring(&mut gizmos, scale.distance(au as f32), color);
            }
        }
    }

    if settings.show_orbits {
        for body in display::bodies_at(result, playback.t_myr) {
            let el = elements(&body, star.mass_msun, playback.orbit_days);
            let selected = selection.0 == Some(Target::Body(body.id));
            let color = if selected {
                palette::ORBIT_SELECTED_COLOR
            } else {
                palette::ORBIT_COLOR.with_alpha(0.35 * body.presence)
            };
            let points = (0..=ORBIT_SEGMENTS).map(|k| {
                let big_e = TAU * k as f64 / ORBIT_SEGMENTS as f64;
                map_position(scale, kepler::point_at_eccentric_anomaly(el, big_e))
            });
            gizmos.linestrip(points, color);
        }
    }

    if settings.show_formation_highlights {
        let lookback = view.timeline_end_myr() * HIGHLIGHT_LOOKBACK;
        for highlight in display::highlights_at(result, playback.t_myr, lookback) {
            match highlight {
                Highlight::Sweep {
                    inner_au,
                    outer_au,
                    age,
                } => {
                    let color = palette::SWEEP_COLOR.with_alpha(0.6 * (1.0 - age));
                    ring(&mut gizmos, scale.distance(inner_au as f32), color);
                    ring(&mut gizmos, scale.distance(outer_au as f32), color);
                }
                Highlight::Collision { a_au, age } => {
                    // An expanding, fading pair of rings around the orbit.
                    let color = palette::COLLISION_COLOR.with_alpha(1.0 - age);
                    let r = scale.distance(a_au as f32);
                    ring(&mut gizmos, r * (1.0 + 0.04 * age), color);
                    ring(&mut gizmos, r * (1.0 - 0.04 * age), color);
                }
            }
        }
    }
}

/// Adds the StarGen-NG scene to a Bevy app.
pub struct StarGenRenderPlugin;

impl Plugin for StarGenRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SystemView>()
            .init_resource::<Playback>()
            .init_resource::<ViewSettings>()
            .init_resource::<Selection>()
            .init_resource::<BodyPlacements>()
            .init_resource::<SceneEntities>()
            .init_resource::<OrbitCamera>()
            .init_resource::<PointerCaptured>()
            .init_resource::<belt::BeltEntities>()
            .init_resource::<sky::SkyState>()
            .add_systems(Startup, (setup, camera::spawn_camera))
            .add_systems(
                Update,
                (
                    camera::control_camera,
                    advance,
                    sync_scene,
                    belt::sync_belts,
                    camera::apply_camera,
                    sky::sync_sky,
                    camera::pick,
                    draw_overlays,
                )
                    .chain(),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::ScaleMode;

    #[test]
    fn simulation_frame_maps_to_y_up() {
        // The orbital plane (z = 0) becomes the scene's horizontal plane.
        assert_eq!(to_scene([1.0, 2.0, 3.0]), Vec3::new(1.0, 3.0, -2.0));
        assert_eq!(to_scene([1.0, 2.0, 0.0]).y, 0.0);
    }

    #[test]
    fn positions_keep_direction_and_take_the_mapped_distance() {
        let scale = ScaleBlend::settled(ScaleMode::Schematic);
        let p = map_position(&scale, [3.0, 4.0, 0.0]);
        assert!((p.length() - ScaleMode::Schematic.distance(5.0)).abs() < 1e-5);
        assert!((p.normalize() - Vec3::new(0.6, 0.0, -0.8)).length() < 1e-5);
        assert_eq!(map_position(&scale, [0.0, 0.0, 0.0]), Vec3::ZERO);
    }

    #[test]
    fn orbital_clock_advances_the_mean_anomaly() {
        let body = DisplayBody {
            id: BodyId(0),
            a_au: 1.0,
            e: 0.0,
            mass_mearth: 1.0,
            radius_km: 6378.0,
            class: None,
            long_asc_node_deg: 0.0,
            arg_periapsis_deg: 0.0,
            mean_anomaly_deg: 0.0,
            presence: 1.0,
        };
        // Half a year later an Earth is on the far side of its orbit.
        let p = kepler::position(elements(&body, 1.0, 365.25 / 2.0));
        assert!((p[0] + 1.0).abs() < 1e-9 && p[1].abs() < 1e-9);
    }
}
