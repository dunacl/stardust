//! The 3D scene: star, bodies, orbit lines and formation highlights. Belts
//! are in [`crate::belt`] and the sky in [`crate::sky`].

use crate::camera::{self, OrbitCamera, PointerCaptured};
use crate::display::{self, DisplayBody, Highlight};
use crate::layout::{self, Layout};
use crate::palette;
use crate::planet::{self, PlanetMaterial, StarLighting};
use crate::scale::{ScaleBlend, ScaleMode};
use crate::star::StarMaterial;
use crate::surface::{self, Appearance};
use crate::{belt, sky};
use bevy::light::GlobalAmbientLight;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use sg_core::units::{Au, SolarMass};
use sg_dynamics::kepler::{self, Elements};
use sg_schema::{BodyId, HierarchyNode, HostRef, PlanetClass, Star, StarId};
use sg_sim::SimulationResult;
use std::collections::BTreeMap;
use std::f64::consts::TAU;

/// Something that can be selected and followed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Target {
    Star(StarId),
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

    /// Every star of the system, in the hierarchy's order.
    pub fn stars(&self) -> Vec<&Star> {
        self.result
            .as_ref()
            .map_or_else(Vec::new, |r| layout::stars(&r.snapshot.hierarchy))
    }

    pub fn star(&self, id: StarId) -> Option<&Star> {
        self.stars().into_iter().find(|star| star.id == id)
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
    /// Rings where a companion star's pull ends the zone open to planets.
    pub show_stability_limits: bool,
    /// The sky behind the system ([`crate::sky`]): scenery, not simulation.
    pub show_backdrop: bool,
    /// Names beside the star and planets. Drawn by the app.
    pub show_labels: bool,
    /// The disk of gas and dust during formation playback.
    pub show_disk: bool,
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self {
            scale: ScaleBlend::default(),
            show_orbits: true,
            show_habitable_zone: true,
            show_formation_highlights: true,
            show_stability_limits: true,
            show_backdrop: true,
            show_labels: true,
            show_disk: true,
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
    look: Appearance,
}

/// The shell of air drawn around a body.
#[derive(Component)]
struct AirVisual;

#[derive(Component)]
struct StarVisual;

#[derive(Component)]
struct StarLight;

#[derive(Resource)]
struct SceneAssets {
    /// Fine enough for a body seen close up.
    globe: Handle<Mesh>,
}

/// The sphere and the light of one star.
struct StarEntities {
    sphere: Entity,
    light: Entity,
}

#[derive(Resource, Default)]
struct SceneEntities {
    generation: u64,
    bodies: BTreeMap<BodyId, Entity>,
    /// The air of the bodies that have any.
    air: BTreeMap<BodyId, Entity>,
    stars: BTreeMap<StarId, StarEntities>,
}

/// Bodies never shrink below this fraction of their distance from the
/// camera, so they stay visible (as dots) at any zoom. At true scale this is
/// what you see until you are very close.
const MIN_APPARENT_RADIUS: f32 = 0.0035;
const ORBIT_SEGMENTS: usize = 128;
/// Illuminance the stars together give the body being followed, and the
/// reference distance of the overview. Exposure conventions, not physics.
const FOLLOWED_LUX: f32 = 20_000.0;
const OVERVIEW_LUX: f32 = 2200.0;
/// Days of the orbital clock in which a body is drawn turning once. Real
/// days are far shorter than years: at a playback rate that shows orbits,
/// true rotation would be a blur, so every unlocked body is drawn turning
/// at this one slow rate. A drawing convention.
const DRAWN_ROTATION_DAYS: f64 = 1500.0;
/// Vertical field of view of the camera (Bevy's default), radians.
const FIELD_OF_VIEW: f32 = std::f32::consts::FRAC_PI_4;
/// How long a formation highlight lingers, as a fraction of the timeline.
const HIGHLIGHT_LOOKBACK: f64 = 0.02;
/// A star is never drawn wider than this fraction of the distance to its
/// nearest companion (or than half the distance of its closest planet), so
/// the enlarged scale modes cannot make close pairs overlap or swallow a
/// hot Jupiter. A drawing convention.
const MAX_STAR_RADIUS_IN_SEPARATIONS: f32 = 0.3;

/// Smallest radius at which something at `position` is drawn, seen from
/// `eye`.
pub(crate) fn apparent_floor(eye: Vec3, position: Vec3) -> f32 {
    (position - eye).length() * MIN_APPARENT_RADIUS
}

/// Orbit of a body about its host. The period follows from the host's
/// mass, so it is right for embryos too, which the snapshot does not list.
fn elements(body: &DisplayBody, host_mass_msun: f64, orbit_days: f64) -> Elements {
    let period = kepler::orbital_period(Au(body.a_au), SolarMass(host_mass_msun)).0;
    Elements {
        a: Au(body.a_au),
        e: body.e,
        i: 0.0,
        long_asc_node: body.long_asc_node_deg.to_radians(),
        arg_periapsis: body.arg_periapsis_deg.to_radians(),
        mean_anomaly: body.mean_anomaly_deg.to_radians() + TAU * (orbit_days / period),
    }
}

/// Scene radius that fits the finished system: see [`layout::system_extent`].
pub fn system_extent(result: &SimulationResult, scale: &ScaleBlend) -> f32 {
    layout::system_extent(&result.snapshot, scale)
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(SceneAssets {
        globe: meshes.add(Sphere::new(1.0).mesh().uv(128, 64)),
    });
    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: 120.0,
        ..default()
    });
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

// Bevy system parameters: the queries are disjoint by their filters.
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::type_complexity
)]
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
    mut planet_materials: ResMut<Assets<PlanetMaterial>>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut bodies: Query<
        (&mut Transform, &BodyVisual, &MeshMaterial3d<PlanetMaterial>),
        (Without<StarVisual>, Without<StarLight>, Without<AirVisual>),
    >,
    mut air: Query<
        (&mut Transform, &MeshMaterial3d<PlanetMaterial>),
        (With<AirVisual>, Without<StarVisual>, Without<StarLight>),
    >,
    mut star_materials: ResMut<Assets<StarMaterial>>,
    mut star_spheres: Query<
        (&mut Transform, &MeshMaterial3d<StarMaterial>),
        (With<StarVisual>, Without<BodyVisual>, Without<AirVisual>),
    >,
    mut lights: Query<
        (&mut Transform, &mut PointLight),
        (
            With<StarLight>,
            Without<StarVisual>,
            Without<BodyVisual>,
            Without<AirVisual>,
        ),
    >,
) {
    placements.0.clear();
    let Some(result) = view.result() else {
        return;
    };
    let snapshot = &result.snapshot;

    // A new system: start from an empty scene.
    if entities.generation != view.generation() {
        for (_, entity) in std::mem::take(&mut entities.bodies) {
            commands.entity(entity).despawn();
        }
        for (_, entity) in std::mem::take(&mut entities.air) {
            commands.entity(entity).despawn();
        }
        for (_, star) in std::mem::take(&mut entities.stars) {
            commands.entity(star.sphere).despawn();
            commands.entity(star.light).despawn();
        }
        entities.generation = view.generation();
    }

    let scale = &settings.scale;
    let eye = orbit.eye();
    let apparent_floor = |position: Vec3| apparent_floor(eye, position);
    let layout = Layout::at(&snapshot.hierarchy, scale, playback.orbit_days);
    let stars = layout::stars(&snapshot.hierarchy);
    let pixels_per_radian = window
        .single()
        .map_or(900.0, |w| w.physical_height() as f32)
        / (2.0 * (0.5 * FIELD_OF_VIEW).tan());
    // The granules boil with the orbital clock, so they stop when it does.
    let star_clock = (playback.orbit_days * 0.01).rem_euclid(1000.0) as f32;

    // The stars.
    for star in &stars {
        let world = star_position_of(&layout, star);
        let position = orbit.local(world);
        let nearest = layout
            .stars
            .iter()
            .filter(|(id, _)| **id != star.id)
            .map(|(_, other)| (*other - world).length() as f32)
            .fold(f32::INFINITY, f32::min);
        // Nor wider than half the distance of its closest planet.
        let closest_planet = snapshot
            .bodies
            .iter()
            .filter(|b| b.host == HostRef::Star(star.id))
            .map(|b| scale.distance((b.orbit.a_au * (1.0 - b.orbit.e)) as f32))
            .fold(f32::INFINITY, f32::min);
        let body_radius = scale
            .star_radius(star.radius_rsun as f32)
            .min(MAX_STAR_RADIUS_IN_SEPARATIONS * nearest)
            .min(0.5 * closest_planet);
        let radius = body_radius.max(apparent_floor(position) * 1.6);
        let color = palette::star_color(star.color_linear_rgb);
        let transform = Transform::from_translation(position).with_scale(Vec3::splat(radius));

        match entities.stars.get(&star.id) {
            Some(existing) => {
                if let Ok((mut current, material)) = star_spheres.get_mut(existing.sphere) {
                    *current = transform;
                    if let Some(mut material) = star_materials.get_mut(&material.0) {
                        let pixels =
                            pixels_per_radian * radius / (position - eye).length().max(1e-9);
                        material.update(eye, star_clock, pixels);
                    }
                }
            }
            None => {
                let mut material = StarMaterial::new(color, star.teff_k, star.id.0 as f32 * 7.3);
                material.update(eye, star_clock, 100.0);
                let material = star_materials.add(material);
                let sphere = commands
                    .spawn((
                        StarVisual,
                        Mesh3d(assets.globe.clone()),
                        MeshMaterial3d(material),
                        transform,
                    ))
                    .id();
                let light = commands
                    .spawn((
                        StarLight,
                        PointLight {
                            range: 1.0e7,
                            color: Color::LinearRgba(color),
                            ..default()
                        },
                        Transform::from_translation(position),
                    ))
                    .id();
                entities
                    .stars
                    .insert(star.id, StarEntities { sphere, light });
            }
        }
        placements.0.insert(
            Target::Star(star.id),
            Placement {
                position,
                radius,
                body_radius,
                focus_radius: body_radius,
            },
        );
    }

    // The bodies. Each is lit in the shader by every star, in proportion
    // to the light it really receives from each: that needs true
    // distances, whatever the scale mode on screen.
    let true_layout = Layout::at(
        &snapshot.hierarchy,
        &ScaleBlend::settled(ScaleMode::True),
        playback.orbit_days,
    );
    let spin = (TAU * playback.orbit_days / DRAWN_ROTATION_DAYS).rem_euclid(TAU) as f32;
    let shown = display::bodies_at(result, playback.t_myr);
    let mut stale: Vec<BodyId> = entities.bodies.keys().copied().collect();
    for body in &shown {
        let (Some(centre), Some(true_centre), Some(host_mass)) = (
            layout.host(body.host),
            true_layout.host(body.host),
            layout::host_mass(snapshot, body.host),
        ) else {
            continue;
        };
        stale.retain(|id| *id != body.id);
        let orbital = kepler::position(elements(body, host_mass, playback.orbit_days));
        let offset = layout::map_offset(scale, orbital);
        let world = centre + offset;
        let position = orbit.local(world);
        let true_position = true_centre + layout::to_scene(orbital);
        let presence = body.presence.clamp(0.0, 1.0);
        let body_radius = scale.planet_radius(body.radius_km as f32);
        let radius = body_radius.max(apparent_floor(position)) * presence.max(0.02);
        let transform = Transform::from_translation(position).with_scale(Vec3::splat(radius));

        let shares = planet::light_shares(
            &stars
                .iter()
                .map(|star| {
                    let at = star_position_of(&true_layout, star);
                    (star.luminosity_lsun, (at - true_position).length())
                })
                .collect::<Vec<_>>(),
        );
        let lighting: Vec<StarLighting> = stars
            .iter()
            .zip(shares)
            .take(planet::MAX_STARS)
            .map(|(star, share)| StarLighting {
                direction: (star_position_of(&layout, star) - world)
                    .normalize_or(DVec3::X)
                    .as_vec3(),
                color: palette::star_color(star.color_linear_rgb).to_vec3(),
                share,
            })
            .collect();
        let pixels = pixels_per_radian * radius / (position - eye).length().max(1e-9);

        // A body that has just become a finished planet is drawn anew.
        if let Some(&entity) = entities.bodies.get(&body.id) {
            if bodies
                .get(entity)
                .is_ok_and(|(_, visual, _)| visual.class != body.class)
            {
                commands.entity(entity).despawn();
                entities.bodies.remove(&body.id);
                if let Some(shell) = entities.air.remove(&body.id) {
                    commands.entity(shell).despawn();
                }
            }
        }

        match entities.bodies.get(&body.id) {
            Some(&entity) => {
                if let Ok((mut current, visual, material)) = bodies.get_mut(entity) {
                    *current = transform;
                    if let Some(mut material) = planet_materials.get_mut(&material.0) {
                        planet::update_uniform(
                            &mut material.uniform,
                            &visual.look,
                            eye,
                            &lighting,
                            spin,
                            pixels,
                        );
                    }
                    if let Some((mut shell, material)) = entities
                        .air
                        .get(&body.id)
                        .and_then(|shell| air.get_mut(*shell).ok())
                    {
                        let height = visual.look.air.map_or(0.0, |a| a.height);
                        *shell = transform.with_scale(Vec3::splat(radius * (1.0 + height)));
                        if let Some(mut material) = planet_materials.get_mut(&material.0) {
                            planet::update_uniform(
                                &mut material.uniform,
                                &visual.look,
                                eye,
                                &lighting,
                                spin,
                                pixels,
                            );
                        }
                    }
                }
            }
            None => {
                let look = snapshot
                    .bodies
                    .iter()
                    .find(|b| b.id == body.id && body.class.is_some())
                    .map_or_else(
                        || surface::embryo_appearance(body.id.0),
                        surface::appearance,
                    );
                let mut material_for = |shell: bool| {
                    let mut uniform = planet::uniform_for(&look, shell);
                    planet::update_uniform(&mut uniform, &look, eye, &lighting, spin, pixels);
                    planet_materials.add(PlanetMaterial { uniform, shell })
                };
                if let Some(gas) = look.air {
                    let shell = commands
                        .spawn((
                            AirVisual,
                            Mesh3d(assets.globe.clone()),
                            MeshMaterial3d(material_for(true)),
                            transform.with_scale(Vec3::splat(radius * (1.0 + gas.height))),
                        ))
                        .id();
                    entities.air.insert(body.id, shell);
                }
                let entity = commands
                    .spawn((
                        Mesh3d(assets.globe.clone()),
                        MeshMaterial3d(material_for(false)),
                        transform,
                        BodyVisual {
                            class: body.class,
                            look,
                        },
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
                        offset.length() as f32 * belt::SELECTED_VIEW_FRACTION
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
        if let Some(entity) = entities.air.remove(&id) {
            commands.entity(entity).despawn();
        }
        // A selected planet stays selected while playback is before its
        // birth; anything not in the finished system is deselected.
        let survives = snapshot.bodies.iter().any(|b| b.id == id);
        if selection.0 == Some(Target::Body(id)) && !survives {
            selection.0 = None;
        }
    }

    // Light the scene so that whatever the camera is looking at is well
    // exposed: `FOLLOWED_LUX` in total at the body being followed (or, when
    // a star is followed, at the distance it is viewed from), or
    // `OVERVIEW_LUX` a third of the way out when the whole system is in
    // view. Each star gives its share of the total in proportion to its
    // luminosity (a point light of Φ lumens gives Φ/4πd² lux at distance
    // d). Real starlight falls off as 1/d²; a fixed physical brightness
    // would leave either the inner or the outer system unreadable.
    let total_luminosity: f64 = stars.iter().map(|star| star.luminosity_lsun).sum();
    let share = |star: &Star| (star.luminosity_lsun / total_luminosity) as f32;
    let star_position = |star: &Star| orbit.local(star_position_of(&layout, star));
    // Lux received at the reference point per unit of (total flux / 4π).
    let received_at = |point: Vec3, at_least: f32| -> f32 {
        stars
            .iter()
            .map(|star| {
                let d = (point - star_position(star)).length().max(at_least);
                share(star) / (d * d)
            })
            .sum()
    };
    let (lux, received) = match selection.0.and_then(|target| {
        placements
            .0
            .get(&target)
            .map(|placement| (target, placement))
    }) {
        Some((Target::Body(_), placement)) => (
            FOLLOWED_LUX,
            received_at(placement.position, placement.radius),
        ),
        // Looking at a star: expose for what orbits it at about the
        // distance the camera is viewing from.
        Some((Target::Star(_), placement)) => {
            let view = orbit.goal_distance.max(placement.radius);
            (
                FOLLOWED_LUX,
                received_at(placement.position + Vec3::X * view, view),
            )
        }
        None => {
            let reference = 0.3 * layout::system_extent(snapshot, scale);
            (OVERVIEW_LUX, 1.0 / (reference * reference))
        }
    };
    for star in &stars {
        let Some(existing) = entities.stars.get(&star.id) else {
            continue;
        };
        if let Ok((mut transform, mut light)) = lights.get_mut(existing.light) {
            transform.translation = star_position(star);
            light.intensity = lux * 4.0 * std::f32::consts::PI * share(star) / received;
        }
    }
}

fn star_position_of(layout: &Layout, star: &Star) -> DVec3 {
    layout.stars.get(&star.id).copied().unwrap_or(DVec3::ZERO)
}

fn ring(gizmos: &mut Gizmos, centre: Vec3, radius: f32, color: Color) {
    let points = (0..=ORBIT_SEGMENTS).map(|k| {
        let angle = std::f32::consts::TAU * k as f32 / ORBIT_SEGMENTS as f32;
        centre + Vec3::new(radius * angle.cos(), 0.0, radius * angle.sin())
    });
    gizmos.linestrip(points, color);
}

/// Draws the paths the stars of every pair trace around their centres.
fn star_paths(
    gizmos: &mut Gizmos,
    node: &HierarchyNode,
    layout: &Layout,
    scale: &ScaleBlend,
    camera: &OrbitCamera,
) {
    let HierarchyNode::Pair { id, a, b, orbit } = node else {
        return;
    };
    let centre = layout.centres.get(id).copied().unwrap_or(DVec3::ZERO);
    let (mass_a, mass_b) = (layout::node_mass(a), layout::node_mass(b));
    let total = mass_a + mass_b;
    for fraction in [-mass_b / total, mass_a / total] {
        let points = layout::member_path(orbit, fraction, scale, ORBIT_SEGMENTS)
            .into_iter()
            .map(|p| camera.local(centre + p));
        gizmos.linestrip(points, palette::STAR_ORBIT_COLOR);
    }
    star_paths(gizmos, a, layout, scale, camera);
    star_paths(gizmos, b, layout, scale, camera);
}

fn draw_overlays(
    mut gizmos: Gizmos,
    view: Res<SystemView>,
    playback: Res<Playback>,
    settings: Res<ViewSettings>,
    selection: Res<Selection>,
    orbit: Res<OrbitCamera>,
) {
    let Some(result) = view.result() else {
        return;
    };
    let snapshot = &result.snapshot;
    let scale = &settings.scale;
    let layout = Layout::at(&snapshot.hierarchy, scale, playback.orbit_days);
    let world_centre_of = |host: HostRef| layout.host(host);
    let centre_of = |host: HostRef| layout.host(host).map(|centre| orbit.local(centre));

    if settings.show_habitable_zone {
        for hz in &snapshot.habitable_zones {
            let Some(centre) = centre_of(hz.host) else {
                continue;
            };
            for (au, color) in [
                (hz.conservative_inner_au, palette::HZ_COLOR),
                (hz.conservative_outer_au, palette::HZ_COLOR),
                (hz.optimistic_inner_au, palette::HZ_OPTIMISTIC_COLOR),
                (hz.optimistic_outer_au, palette::HZ_OPTIMISTIC_COLOR),
            ] {
                ring(&mut gizmos, centre, scale.distance(au as f32), color);
            }
        }
    }

    if settings.show_stability_limits {
        for zone in &snapshot.planet_zones {
            let Some(centre) = centre_of(zone.host) else {
                continue;
            };
            for (limited, au) in [
                (zone.limited_inside, zone.inner_au),
                (zone.limited_outside, zone.outer_au),
            ] {
                if limited {
                    ring(
                        &mut gizmos,
                        centre,
                        scale.distance(au as f32),
                        palette::STABILITY_LIMIT_COLOR,
                    );
                }
            }
        }
    }

    if settings.show_orbits {
        star_paths(&mut gizmos, &snapshot.hierarchy, &layout, scale, &orbit);
        for body in display::bodies_at(result, playback.t_myr) {
            let (Some(centre), Some(host_mass)) = (
                world_centre_of(body.host),
                layout::host_mass(snapshot, body.host),
            ) else {
                continue;
            };
            let el = elements(&body, host_mass, playback.orbit_days);
            let selected = selection.0 == Some(Target::Body(body.id));
            let color = if selected {
                palette::ORBIT_SELECTED_COLOR
            } else {
                palette::ORBIT_COLOR.with_alpha(0.35 * body.presence)
            };
            let points = (0..=ORBIT_SEGMENTS).map(|k| {
                let big_e = TAU * k as f64 / ORBIT_SEGMENTS as f64;
                orbit.local(
                    centre
                        + layout::map_offset(scale, kepler::point_at_eccentric_anomaly(el, big_e)),
                )
            });
            gizmos.linestrip(points, color);
        }
    }

    if settings.show_formation_highlights {
        let lookback = view.timeline_end_myr() * HIGHLIGHT_LOOKBACK;
        for highlight in display::highlights_at(result, playback.t_myr, lookback) {
            match highlight {
                Highlight::Sweep {
                    host,
                    inner_au,
                    outer_au,
                    age,
                } => {
                    let Some(centre) = centre_of(host) else {
                        continue;
                    };
                    let color = palette::SWEEP_COLOR.with_alpha(0.6 * (1.0 - age));
                    ring(&mut gizmos, centre, scale.distance(inner_au as f32), color);
                    ring(&mut gizmos, centre, scale.distance(outer_au as f32), color);
                }
                Highlight::Collision { host, a_au, age } => {
                    let Some(centre) = centre_of(host) else {
                        continue;
                    };
                    // An expanding, fading pair of rings around the orbit.
                    let color = palette::COLLISION_COLOR.with_alpha(1.0 - age);
                    let r = scale.distance(a_au as f32);
                    ring(&mut gizmos, centre, r * (1.0 + 0.04 * age), color);
                    ring(&mut gizmos, centre, r * (1.0 - 0.04 * age), color);
                }
            }
        }
    }
}

/// Adds the Stardust scene to a Bevy app.
pub struct StardustRenderPlugin;

impl Plugin for StardustRenderPlugin {
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
            .init_resource::<sky::SkyState>();
        planet::build(app);
        crate::star::build(app);
        crate::disk::build(app);
        app.add_systems(Startup, (setup, camera::spawn_camera))
            .add_systems(
                Update,
                (
                    camera::control_camera,
                    camera::rebase_camera,
                    advance,
                    sync_scene,
                    belt::sync_belts,
                    crate::disk::sync_disks,
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

    #[test]
    fn orbital_clock_advances_the_mean_anomaly() {
        let body = DisplayBody {
            id: BodyId(0),
            host: HostRef::Star(StarId(0)),
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
        // Around a star four times heavier the year is half as long.
        let p = kepler::position(elements(&body, 4.0, 365.25 / 4.0));
        assert!((p[0] + 1.0).abs() < 1e-9 && p[1].abs() < 1e-9);
    }
}
