//! The disk of gas and dust shown during formation playback (PLAN.md
//! §8.4), drawn by [`disk.wesl`](../disk.wesl).
//!
//! What comes from the model:
//!
//! - **Standard tier**: the gas surface density at each distance and time
//!   (the self-similar profile of `sg-disk`, from the radius and viscous
//!   time the snapshot records), its clearing when the disk disperses, and
//!   a gap at each planet the model says has opened one.
//! - **Classic tier**: the extent of the dust (the zone open to planets),
//!   the lanes each body has swept clear so far, and the time the dust is
//!   gone. The dust density law is not in the snapshot, so the dust
//!   that remains is drawn evenly.
//!
//! What is a drawing choice: opacity follows the cube root of the surface
//! density (the density spans a factor of thousands, which no screen can
//! show linearly), the colour, the streaks, and the flatness: a real disk
//! flares, this one is a sheet.

use crate::display;
use crate::layout::{self, Layout};
use crate::scale::ScaleBlend;
use crate::scene::{Playback, SystemView, ViewSettings};
use crate::OrbitCamera;
use bevy::asset::embedded_asset;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use sg_schema::{BodyId, EventKind, HostRef};
use sg_sim::SimulationResult;
use std::collections::BTreeMap;

/// Opacity samples across the disk, evenly spaced in scene radius.
pub const SAMPLES: usize = 64;
/// Most gaps drawn in one disk.
pub const MAX_GAPS: usize = 8;

/// Half-width of the gap a giant opens, in Hill radii: about two (Crida,
/// Morbidelli & Masset 2006, Icarus 181, 587, fig. 6).
const GAP_HALF_WIDTH_HILL: f64 = 2.0;
/// How far the gas is drawn, in characteristic radii: the profile falls
/// as `exp(−r/R₁)/r`, so beyond this there is little to show.
const DRAWN_EXTENT_R1: f64 = 3.0;
/// Radius, in characteristic radii, at which the initial disk is drawn
/// fully opaque; the rest is relative to it.
const REFERENCE_R1: f64 = 0.1;
/// Opacity of Classic-tier dust that has not been swept.
const CLASSIC_DUST_OPACITY: f32 = 0.45;
/// Fraction of the Classic timeline over which the last dust fades.
const CLASSIC_FADE: f64 = 0.12;
const EARTHS_PER_SUN: f64 = 332_946.0;

/// A gap in a disk.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gap {
    pub a_au: f64,
    pub half_width_au: f64,
}

/// One disk as drawn at one moment.
#[derive(Debug, Clone, PartialEq)]
pub struct DiskView {
    pub host: HostRef,
    /// The range drawn, AU.
    pub inner_au: f64,
    pub outer_au: f64,
    /// Opacity, 0 to 1, at [`SAMPLES`] radii evenly spaced in scene radius
    /// from the inner to the outer edge.
    pub opacity: [f32; SAMPLES],
    pub gaps: Vec<Gap>,
}

/// Opacity drawn for a gas surface density relative to the reference: its
/// cube root, capped at 1. A display mapping, not a physical one.
pub fn opacity_of(relative_density: f64) -> f32 {
    relative_density.max(0.0).cbrt().min(1.0) as f32
}

/// The orbital distance, AU, that the scale draws at `scene_radius`: the
/// inverse of [`ScaleBlend::distance_f64`], by bisection (the mapping is
/// monotonic but, mid-transition, has no closed inverse).
pub fn au_at_scene_radius(scale: &ScaleBlend, scene_radius: f64, upper_au: f64) -> f64 {
    let (mut lo, mut hi) = (0.0, upper_au);
    for _ in 0..48 {
        let mid = 0.5 * (lo + hi);
        if scale.distance_f64(mid) < scene_radius {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// The radii, AU, of the opacity samples.
fn sample_radii(scale: &ScaleBlend, inner_au: f64, outer_au: f64) -> [f64; SAMPLES] {
    let (inner, outer) = (scale.distance_f64(inner_au), scale.distance_f64(outer_au));
    std::array::from_fn(|k| {
        let rho = inner + (outer - inner) * k as f64 / (SAMPLES - 1) as f64;
        au_at_scene_radius(scale, rho, outer_au).clamp(inner_au, outer_au)
    })
}

/// The disks to draw at playback time `t_myr`. Empty once the gas (or, in
/// the Classic tier, the dust) is gone.
pub fn disk_views(result: &SimulationResult, t_myr: f64, scale: &ScaleBlend) -> Vec<DiskView> {
    let snapshot = &result.snapshot;
    let end = display::timeline_end_myr(result);
    if t_myr >= end {
        return Vec::new();
    }
    let events = &result.timeline.events;
    let happened = &events[..events.partition_point(|e| e.t_myr <= t_myr)];
    let mut views = Vec::new();

    // Standard tier: the gas.
    for disk in &snapshot.disks_at_dispersal {
        let gas = sg_disk::gas_fraction(t_myr, disk.lifetime_myr);
        if gas <= 0.0 {
            continue;
        }
        let r1 = disk.characteristic_radius_au;
        let big_t = 1.0 + t_myr / disk.viscous_time_myr;
        let inner_au = disk.inner_au;
        let outer_au = disk.outer_au.min(DRAWN_EXTENT_R1 * r1).max(inner_au * 1.01);
        let reference = sg_disk::relative_surface_density(REFERENCE_R1, 1.0);
        let opacity = sample_radii(scale, inner_au, outer_au).map(|r| {
            opacity_of(gas * sg_disk::relative_surface_density(r / r1, big_t) / reference)
        });
        let Some(host_mass) = layout::host_mass(snapshot, disk.host) else {
            continue;
        };
        let opened: Vec<BodyId> = happened
            .iter()
            .filter_map(|event| match event.kind {
                EventKind::GapOpened { body } => Some(body),
                _ => None,
            })
            .collect();
        let mut carvers: Vec<_> = display::bodies_at(result, t_myr)
            .into_iter()
            .filter(|b| b.host == disk.host && opened.contains(&b.id))
            .collect();
        carvers.sort_by(|a, b| b.mass_mearth.total_cmp(&a.mass_mearth));
        let gaps = carvers
            .iter()
            .take(MAX_GAPS)
            .map(|b| {
                let hill = b.a_au * (b.mass_mearth / EARTHS_PER_SUN / (3.0 * host_mass)).cbrt();
                Gap {
                    a_au: b.a_au,
                    half_width_au: GAP_HALF_WIDTH_HILL * hill,
                }
            })
            .collect();
        views.push(DiskView {
            host: disk.host,
            inner_au,
            outer_au,
            opacity,
            gaps,
        });
    }
    if !views.is_empty() || !snapshot.disks_at_dispersal.is_empty() {
        return views;
    }

    // Classic tier: the dust, less the lanes swept so far.
    let fade = ((end - t_myr) / (CLASSIC_FADE * end)).clamp(0.0, 1.0) as f32;
    for zone in &snapshot.planet_zones {
        let swept: Vec<(f64, f64)> = happened
            .iter()
            .filter_map(|event| match event.kind {
                EventKind::DustSwept {
                    body,
                    inner_au,
                    outer_au,
                    ..
                } if display::host_of(result, body) == Some(zone.host) => {
                    Some((inner_au, outer_au))
                }
                _ => None,
            })
            .collect();
        let opacity = sample_radii(scale, zone.inner_au, zone.outer_au).map(|r| {
            if swept
                .iter()
                .any(|(inner, outer)| (*inner..=*outer).contains(&r))
            {
                0.0
            } else {
                CLASSIC_DUST_OPACITY * fade
            }
        });
        if opacity.iter().any(|o| *o > 0.0) {
            views.push(DiskView {
                host: zone.host,
                inner_au: zone.inner_au,
                outer_au: zone.outer_au,
                opacity,
                gaps: Vec::new(),
            });
        }
    }
    views
}

/// The shader's uniform; see `disk.wesl`.
#[derive(Debug, Clone, Default, ShaderType)]
pub struct DiskUniform {
    pub centre: Vec4,
    pub extent: Vec4,
    pub color: Vec4,
    pub samples: [Vec4; SAMPLES / 4],
    pub gaps: [Vec4; MAX_GAPS],
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct DiskMaterial {
    #[uniform(0)]
    pub uniform: DiskUniform,
}

impl Material for DiskMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://sg_render/disk.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }
}

/// Colour of starlit dust, linear RGB, before the star's own colour.
const DUST_COLOR: Vec3 = Vec3::new(0.80, 0.52, 0.28);
/// Scales every opacity: the disk should not hide what forms inside it.
const OVERALL_OPACITY: f32 = 0.6;
/// Days of the orbital clock in which the streaks wind through one cycle.
const STREAK_CYCLE_DAYS: f64 = 6000.0;

/// Packs a view for the shader. `centre` is the host's position relative
/// to the camera's origin.
pub fn uniform_for(
    view: &DiskView,
    scale: &ScaleBlend,
    centre: Vec3,
    star_color: Vec3,
    orbit_days: f64,
) -> DiskUniform {
    let mut uniform = DiskUniform {
        centre: centre.extend(1.0),
        extent: Vec4::new(
            scale.distance_f64(view.inner_au) as f32,
            scale.distance_f64(view.outer_au) as f32,
            (orbit_days / STREAK_CYCLE_DAYS).rem_euclid(1.0) as f32,
            OVERALL_OPACITY,
        ),
        color: (DUST_COLOR * star_color).extend(1.0),
        ..default()
    };
    for (k, opacity) in view.opacity.iter().enumerate() {
        uniform.samples[k / 4][k % 4] = *opacity;
    }
    for (slot, gap) in uniform.gaps.iter_mut().zip(&view.gaps) {
        let inner = scale.distance_f64(gap.a_au - gap.half_width_au);
        let outer = scale.distance_f64(gap.a_au + gap.half_width_au);
        *slot = Vec4::new(
            scale.distance_f64(gap.a_au) as f32,
            (0.5 * (outer - inner)) as f32,
            0.92,
            0.0,
        );
    }
    uniform
}

#[derive(Component)]
pub(crate) struct DiskVisual;

/// The two faces of one host's disk, and their shared material.
struct DiskFaces {
    faces: [Entity; 2],
    material: Handle<DiskMaterial>,
}

#[derive(Resource, Default)]
pub(crate) struct DiskEntities {
    generation: u64,
    disks: BTreeMap<HostRef, DiskFaces>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn sync_disks(
    mut commands: Commands,
    view: Res<SystemView>,
    playback: Res<Playback>,
    settings: Res<ViewSettings>,
    orbit: Res<OrbitCamera>,
    mut entities: ResMut<DiskEntities>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<DiskMaterial>>,
    mut faces: Query<(&mut Transform, &mut Visibility), With<DiskVisual>>,
) {
    let Some(result) = view.result() else {
        return;
    };
    if entities.generation != view.generation() {
        for (_, disk) in std::mem::take(&mut entities.disks) {
            for face in disk.faces {
                commands.entity(face).despawn();
            }
        }
        entities.generation = view.generation();
    }
    let snapshot = &result.snapshot;
    let scale = &settings.scale;
    let views = if settings.show_disk {
        disk_views(result, playback.t_myr, scale)
    } else {
        Vec::new()
    };
    let layout = Layout::at(&snapshot.hierarchy, scale, playback.orbit_days);

    for disk in entities.disks.values() {
        for face in disk.faces {
            if let Ok((_, mut visibility)) = faces.get_mut(face) {
                *visibility = Visibility::Hidden;
            }
        }
    }
    for disk_view in &views {
        let Some(centre) = layout.host(disk_view.host).map(|c| orbit.local(c)) else {
            continue;
        };
        // Lit by the brightest star it surrounds.
        let star_color = layout::find_host(&snapshot.hierarchy, disk_view.host)
            .map(layout::stars)
            .and_then(|stars| {
                stars
                    .into_iter()
                    .max_by(|a, b| a.luminosity_lsun.total_cmp(&b.luminosity_lsun))
            })
            .map_or(Vec3::ONE, |star| {
                crate::palette::star_color(star.color_linear_rgb).to_vec3()
            });
        let uniform = uniform_for(disk_view, scale, centre, star_color, playback.orbit_days);
        let outer = uniform.extent.y;
        // A circle in the XY plane, laid flat: one face up, one down.
        let placed = |turn: f32| {
            Transform::from_translation(centre)
                .with_rotation(Quat::from_rotation_x(turn))
                .with_scale(Vec3::splat(outer))
        };
        let (up, down) = (
            placed(-std::f32::consts::FRAC_PI_2),
            placed(std::f32::consts::FRAC_PI_2),
        );
        match entities.disks.get(&disk_view.host) {
            Some(disk) => {
                if let Some(mut material) = materials.get_mut(&disk.material) {
                    material.uniform = uniform;
                }
                for (face, transform) in disk.faces.into_iter().zip([up, down]) {
                    if let Ok((mut current, mut visibility)) = faces.get_mut(face) {
                        *current = transform;
                        *visibility = Visibility::Visible;
                    }
                }
            }
            None => {
                let mesh = meshes.add(Circle::new(1.0).mesh().resolution(128));
                let material = materials.add(DiskMaterial { uniform });
                let spawn = |commands: &mut Commands, transform: Transform| {
                    commands
                        .spawn((
                            DiskVisual,
                            Mesh3d(mesh.clone()),
                            MeshMaterial3d(material.clone()),
                            transform,
                            Visibility::Visible,
                        ))
                        .id()
                };
                let faces = [spawn(&mut commands, up), spawn(&mut commands, down)];
                entities
                    .disks
                    .insert(disk_view.host, DiskFaces { faces, material });
            }
        }
    }
}

pub(crate) fn build(app: &mut App) {
    embedded_asset!(app, "disk.wesl");
    app.add_plugins(MaterialPlugin::<DiskMaterial>::default())
        .init_resource::<DiskEntities>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::ScaleMode;
    use sg_schema::{Fidelity, SimulationParams};

    fn run(fidelity: Fidelity, seed: u64) -> SimulationResult {
        sg_sim::simulate(&SimulationParams {
            seed,
            fidelity,
            ..SimulationParams::default()
        })
        .expect("run")
    }

    #[test]
    fn scene_radius_inverts_every_scale() {
        let mut blend = ScaleBlend::settled(ScaleMode::True);
        blend.go_to(ScaleMode::Schematic);
        blend.advance(0.3);
        for scale in [
            ScaleBlend::settled(ScaleMode::True),
            ScaleBlend::settled(ScaleMode::Schematic),
            blend,
        ] {
            for au in [0.05, 1.0, 30.0, 180.0] {
                let back = au_at_scene_radius(&scale, scale.distance_f64(au), 500.0);
                assert!((back / au - 1.0).abs() < 1e-9, "{au} → {back}");
            }
        }
    }

    #[test]
    fn opacity_is_the_cube_root_capped() {
        assert_eq!(opacity_of(0.0), 0.0);
        assert_eq!(opacity_of(0.125), 0.5);
        assert_eq!(opacity_of(8.0), 1.0);
        assert_eq!(opacity_of(-1.0), 0.0);
    }

    #[test]
    fn standard_gas_thins_outward_drains_and_clears() {
        let result = run(Fidelity::Standard, 1);
        let scale = ScaleBlend::settled(ScaleMode::Exaggerated);
        let disk = &result.snapshot.disks_at_dispersal[0];
        let end = display::timeline_end_myr(&result);

        let early = &disk_views(&result, 0.01, &scale)[0];
        assert_eq!(early.host, disk.host);
        assert!(early.outer_au <= DRAWN_EXTENT_R1 * disk.characteristic_radius_au + 1e-9);
        // Denser inside than outside, and never above full opacity.
        assert!(early.opacity[2] > early.opacity[40]);
        assert!(early.opacity.iter().all(|o| (0.0..=1.0).contains(o)));

        // Later there is less gas in the inner disk (the innermost samples
        // may still be drawn at full opacity) and no more anywhere near it.
        let later = &disk_views(&result, 0.9 * disk.lifetime_myr, &scale)[0];
        for k in 0..SAMPLES / 4 {
            assert!(later.opacity[k] <= early.opacity[k], "sample {k}");
        }
        assert!(later.opacity[SAMPLES / 8] < early.opacity[SAMPLES / 8]);
        // Gone once the gas has cleared, and at the end of the timeline.
        assert!(disk_views(&result, disk.lifetime_myr * 1.11, &scale).is_empty());
        assert!(disk_views(&result, end, &scale).is_empty());
    }

    #[test]
    fn gaps_appear_only_where_the_model_opened_one() {
        // A system with a giant: some body opens a gap during the run.
        let result = sg_sim::simulate(&sg_sim::preset_params(
            sg_schema::PresetId::HotJupiter,
            2,
            Fidelity::Standard,
        ))
        .expect("run");
        let scale = ScaleBlend::settled(ScaleMode::Exaggerated);
        let first_gap = result
            .timeline
            .events
            .iter()
            .find(|e| matches!(e.kind, EventKind::GapOpened { .. }))
            .expect("a gap opens")
            .t_myr;
        let before = disk_views(&result, 0.5 * first_gap, &scale);
        assert!(before.iter().all(|view| view.gaps.is_empty()));
        let disk = &result.snapshot.disks_at_dispersal[0];
        let after = disk_views(&result, 0.5 * (first_gap + disk.lifetime_myr), &scale);
        let gaps: Vec<&Gap> = after.iter().flat_map(|view| &view.gaps).collect();
        assert!(!gaps.is_empty() && gaps.len() <= MAX_GAPS);
        for gap in gaps {
            // A few Hill radii: narrow next to the orbit.
            assert!(gap.half_width_au > 0.0 && gap.half_width_au < 0.5 * gap.a_au);
        }
    }

    #[test]
    fn classic_dust_is_swept_lane_by_lane_and_fades() {
        let result = run(Fidelity::Classic, 42);
        let scale = ScaleBlend::settled(ScaleMode::Exaggerated);
        let end = display::timeline_end_myr(&result);
        let clear = |t: f64| {
            disk_views(&result, t, &scale)
                .iter()
                .flat_map(|view| view.opacity)
                .filter(|o| *o == 0.0)
                .count()
        };
        // Nothing swept at the start; more and more as bodies grow.
        assert_eq!(clear(0.0), 0);
        assert!(clear(0.5 * end) > 0);
        assert!(clear(0.8 * end) >= clear(0.3 * end));
        // The remaining dust is fainter near the end, and gone at it.
        let brightest = |t: f64| {
            disk_views(&result, t, &scale)
                .iter()
                .flat_map(|view| view.opacity)
                .fold(0.0f32, f32::max)
        };
        assert!(brightest(0.97 * end) < brightest(0.5 * end));
        assert!(disk_views(&result, end, &scale).is_empty());
    }
}
