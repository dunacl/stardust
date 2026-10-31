//! The 3D scene: star, bodies, orbit lines and formation highlights. Belts
//! are in [`crate::belt`] and the sky in [`crate::sky`].

use crate::camera::{self, OrbitCamera, PointerCaptured};
use crate::clearance;
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

/// How much the renderer spends on effects (PLAN.md §8.6): everything, or
/// a lighter set for phones and for browsers without WebGPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    #[default]
    Full,
    /// No bloom or multisampling, coarser spheres, fewer belt rocks, and
    /// surface detail limited to four octaves of noise.
    Reduced,
}

impl Quality {
    pub const ALL: [Quality; 2] = [Quality::Full, Quality::Reduced];

    pub fn label(self) -> &'static str {
        match self {
            Quality::Full => "Full",
            Quality::Reduced => "Reduced",
        }
    }

    /// Rocks drawn per belt.
    pub fn rocks_per_belt(self) -> usize {
        match self {
            Quality::Full => 1200,
            Quality::Reduced => 300,
        }
    }

    /// Segments around a sphere's equator; half as many pole to pole.
    pub fn sphere_segments(self) -> u32 {
        match self {
            Quality::Full => 128,
            Quality::Reduced => 48,
        }
    }

    /// Largest apparent radius, pixels, the surface shaders are told: they
    /// choose their octaves of noise from it (`planet.wesl`), so capping
    /// it caps their cost per pixel.
    pub fn detail_pixels(self, apparent_radius_pixels: f32) -> f32 {
        match self {
            Quality::Full => apparent_radius_pixels,
            // log2(40) − 1 = four octaves.
            Quality::Reduced => apparent_radius_pixels.min(40.0),
        }
    }

    pub fn bloom(self) -> bool {
        self == Quality::Full
    }

    pub fn multisampling(self) -> bool {
        self == Quality::Full
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
    pub quality: Quality,
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
            quality: Quality::default(),
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
    /// A moon: drawn at its size with no minimum, so that it is seen only
    /// from near its planet, and not given a generous click target.
    pub moon: bool,
}

/// Scene position and drawn radius of everything selectable, refreshed
/// every frame.
#[derive(Resource, Debug, Default)]
pub struct BodyPlacements(BTreeMap<Target, Placement>, Vec<MoonOrbit>);

/// The path of a moon around its planet this frame, for drawing it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoonOrbit {
    pub planet: BodyId,
    pub moon: BodyId,
    /// The planet's position.
    pub centre: Vec3,
    /// Two perpendicular vectors spanning the orbit's plane, each the
    /// length of the orbit's semi-major axis as drawn.
    pub across: Vec3,
    pub along: Vec3,
    pub e: f32,
    /// Direction of pericentre in the plane, radians from `across`.
    pub arg_periapsis: f32,
}

impl BodyPlacements {
    pub fn get(&self, target: Target) -> Option<Placement> {
        self.0.get(&target).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (Target, Placement)> + '_ {
        self.0.iter().map(|(t, p)| (*t, *p))
    }

    /// The moons' paths this frame.
    pub fn moon_orbits(&self) -> &[MoonOrbit] {
        &self.1
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
    /// One sphere per quality level, in the order of [`Quality::ALL`].
    globes: [Handle<Mesh>; 2],
}

impl SceneAssets {
    fn globe(&self, quality: Quality) -> Handle<Mesh> {
        self.globes[quality as usize].clone()
    }
}

/// The sphere and the light of one star.
struct StarEntities {
    sphere: Entity,
    light: Entity,
}

#[derive(Resource, Default)]
struct SceneEntities {
    generation: u64,
    quality: Quality,
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
        globes: Quality::ALL.map(|quality| {
            let around = quality.sphere_segments();
            meshes.add(Sphere::new(1.0).mesh().uv(around, around / 2))
        }),
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
    placements.1.clear();
    let Some(result) = view.result() else {
        return;
    };
    let snapshot = &result.snapshot;

    // A new system: start from an empty scene.
    if entities.generation != view.generation() || entities.quality != settings.quality {
        entities.quality = settings.quality;
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
                        Mesh3d(assets.globe(settings.quality)),
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
                moon: false,
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
    let ring_outer: BTreeMap<BodyId, f32> = snapshot
        .bodies
        .iter()
        .filter_map(|b| Some((b.id, crate::ring::extent_in_radii(b)?.1)))
        .collect();
    let sizes = clearance::drawn_sizes(&shown, &ring_outer, scale);
    let mut stale: Vec<BodyId> = entities.bodies.keys().copied().collect();
    // Where each planet is this frame, for its moons, which follow it in
    // the list.
    let mut planets_now: BTreeMap<BodyId, PlanetNow> = BTreeMap::new();
    for body in &shown {
        let presence = body.presence.clamp(0.0, 1.0);
        let body_radius = sizes[&body.id].radius;
        let is_moon = matches!(body.host, HostRef::Body(_));
        let (world, true_position, offset_length) = if let HostRef::Body(planet) = body.host {
            let Some(host) = planets_now.get(&planet) else {
                continue;
            };
            let semi_major = sizes[&body.id].semi_major;
            let offset = moon_offset(body, host, semi_major, playback.orbit_days);
            placements.1.push(MoonOrbit {
                planet,
                moon: body.id,
                centre: orbit.local(host.world),
                across: host.tilt * Vec3::X * semi_major,
                along: host.tilt * Vec3::NEG_Z * semi_major,
                e: body.e as f32,
                arg_periapsis: body.arg_periapsis_deg.to_radians() as f32,
            });
            (host.world + offset, host.true_position, offset.length())
        } else {
            let (Some(centre), Some(true_centre), Some(host_mass)) = (
                layout.host(body.host),
                true_layout.host(body.host),
                layout::host_mass(snapshot, body.host),
            ) else {
                continue;
            };
            let orbital = kepler::position(elements(body, host_mass, playback.orbit_days));
            let offset = layout::map_offset(scale, orbital);
            (
                centre + offset,
                true_centre + layout::to_scene(orbital),
                offset.length(),
            )
        };
        stale.retain(|id| *id != body.id);
        let position = orbit.local(world);
        // A planet is never drawn smaller than a dot; a moon is drawn at
        // its size, and so comes into view as its planet is approached.
        let radius = if is_moon {
            // ...unless it or its planet is selected: then it is at least
            // a small dot, so that the moons of the planet in view can be
            // found.
            let in_view = match (selection.0, body.host) {
                (Some(Target::Body(selected)), HostRef::Body(planet)) => {
                    selected == planet || selected == body.id
                }
                _ => false,
            };
            let floor = if in_view {
                MOON_APPARENT_FLOOR * apparent_floor(position)
            } else {
                0.0
            };
            body_radius.max(floor) * presence.max(0.02)
        } else {
            body_radius.max(apparent_floor(position)) * presence.max(0.02)
        };
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
        let pixels = settings
            .quality
            .detail_pixels(pixels_per_radian * radius / (position - eye).length().max(1e-9));

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
                            Mesh3d(assets.globe(settings.quality)),
                            MeshMaterial3d(material_for(true)),
                            transform.with_scale(Vec3::splat(radius * (1.0 + gas.height))),
                        ))
                        .id();
                    entities.air.insert(body.id, shell);
                }
                let entity = commands
                    .spawn((
                        Mesh3d(assets.globe(settings.quality)),
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
                    Some(class) if !is_moon && belt::is_belt_sized(class, body.mass_mearth) => {
                        offset_length as f32 * belt::SELECTED_VIEW_FRACTION
                    }
                    // A planet with moons is framed a little wider, to
                    // bring the innermost into view.
                    _ if !is_moon && body.class.is_some() => {
                        let innermost = snapshot
                            .moons_of(body.id)
                            .map(|moon| moon.orbit.a_au * KM_PER_AU / body.radius_km)
                            .fold(f64::INFINITY, f64::min);
                        if innermost.is_finite() {
                            body_radius * (0.3 * innermost as f32).clamp(1.0, 2.5)
                        } else {
                            body_radius
                        }
                    }
                    _ => body_radius,
                },
                moon: is_moon,
            },
        );
        if !is_moon {
            let tilt = entities
                .bodies
                .get(&body.id)
                .and_then(|entity| bodies.get(*entity).ok())
                .map_or(Quat::IDENTITY, |(_, visual, _)| {
                    planet::axis_tilt(&visual.look)
                });
            planets_now.insert(
                body.id,
                PlanetNow {
                    world,
                    true_position,
                    mass_mearth: body.mass_mearth,
                    tilt,
                },
            );
        }
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

/// A planet as placed this frame, for placing its moons and rings.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PlanetNow {
    pub world: DVec3,
    /// Its position at true scale, AU: what its moons are lit by.
    pub true_position: DVec3,
    pub mass_mearth: f64,
    /// Orbit normal to spin axis.
    pub tilt: Quat,
}

/// Moons are drawn going round this many times slower than the orbital
/// clock would have them: at a rate that shows planets circling their
/// star, a moon with a month of a few days would be a blur. A drawing
/// convention, like the slowed rotation of the planets.
pub const MOON_CLOCK_SLOWDOWN: f64 = 20.0;
/// The smallest a moon of the selected planet is drawn, as a fraction of
/// the smallest a planet is.
const MOON_APPARENT_FLOOR: f32 = 0.6;
const EARTHS_PER_SUN: f64 = 332_946.0;
pub(crate) const KM_PER_AU: f64 = 1.495_978_707e8;

/// Where a moon is relative to its planet, in scene units. Its distance
/// keeps its true proportion to the planet's radius as drawn, so that in
/// every scale mode it sits as many planetary radii out as it really
/// does; its orbit lies in the planet's equatorial plane.
pub(crate) fn moon_offset(
    moon: &DisplayBody,
    planet: &PlanetNow,
    semi_major: f32,
    orbit_days: f64,
) -> DVec3 {
    let period = kepler::orbital_period(
        Au(moon.a_au),
        SolarMass(planet.mass_mearth / EARTHS_PER_SUN),
    )
    .0;
    let unit = kepler::position(Elements {
        a: Au(1.0),
        e: moon.e,
        i: 0.0,
        long_asc_node: 0.0,
        arg_periapsis: moon.arg_periapsis_deg.to_radians(),
        mean_anomaly: moon.mean_anomaly_deg.to_radians()
            + TAU * orbit_days / (period * MOON_CLOCK_SLOWDOWN),
    });
    let flat = layout::to_scene(unit).as_vec3();
    (planet.tilt * flat).as_dvec3() * f64::from(semi_major)
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
    placements: Res<BodyPlacements>,
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

    // The paths of the moons of the selected planet (or of the planet
    // whose moon is selected).
    if settings.show_orbits {
        let chosen = match selection.0 {
            Some(Target::Body(id)) => placements
                .moon_orbits()
                .iter()
                .find(|m| m.moon == id)
                .map_or(Some(id), |m| Some(m.planet)),
            _ => None,
        };
        for path in placements
            .moon_orbits()
            .iter()
            .filter(|m| Some(m.planet) == chosen)
        {
            let color = if selection.0 == Some(Target::Body(path.moon)) {
                palette::ORBIT_SELECTED_COLOR
            } else {
                palette::ORBIT_COLOR
            };
            let minor = (1.0 - path.e * path.e).sqrt();
            let (sin_w, cos_w) = path.arg_periapsis.sin_cos();
            let points = (0..=ORBIT_SEGMENTS).map(|k| {
                let big_e = std::f32::consts::TAU * k as f32 / ORBIT_SEGMENTS as f32;
                // In the orbit's plane, pericentre along +x.
                let (x, y) = (big_e.cos() - path.e, minor * big_e.sin());
                let (px, py) = (x * cos_w - y * sin_w, x * sin_w + y * cos_w);
                path.centre + path.across * px + path.along * py
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
            .init_resource::<crate::voyage::Voyage>()
            .init_resource::<sky::SkyState>();
        planet::build(app);
        crate::star::build(app);
        crate::disk::build(app);
        crate::ring::build(app);
        app.add_systems(
            Startup,
            (setup, camera::spawn_camera, crate::voyage::spawn_ship),
        )
        .add_systems(
            Update,
            (
                camera::control_camera,
                camera::touch_camera,
                camera::rebase_camera,
                camera::apply_quality,
                advance,
                sync_scene,
                belt::sync_belts,
                crate::disk::sync_disks,
                crate::ring::sync_rings,
                crate::voyage::sync_ship,
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
