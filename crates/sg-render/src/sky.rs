//! The sky behind the system: a star field, the band of the host galaxy
//! with its dust lanes and glowing clouds, and a few distant galaxies.
//!
//! **This is scenery, not simulation.** Nothing here comes from the model
//! and nothing in the model depends on it. A generated system has no place
//! in any galaxy, so its sky is invented: each system gets its own, chosen
//! by a seed, and it is kept dim so it reads as far away. What is borrowed
//! from nature is only the look: stars are coloured as blackbodies and
//! crowd toward the galactic plane, where dust also hides them.
//!
//! Everything is generated on the CPU into ordinary textures and meshes, so
//! it works unchanged on WebGL2.

use crate::scene::{SystemView, ViewSettings};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use sg_core::hash::fnv1a64;
use sg_core::rng::{ModuleId, RngStream};
use sg_core::units::Kelvin;
use std::f32::consts::{PI, TAU};

/// Distance of the sky from the camera, scene units. It only has to be
/// farther than anything in the system; the sky moves with the camera.
const SKY_RADIUS: f32 = 1.0e6;
const DOME_WIDTH: u32 = 1024;
const DOME_HEIGHT: u32 = 512;
const STAR_COUNT: usize = 9000;
const GALAXY_COUNT: usize = 9;
const GALAXY_TEXTURE_SIZE: u32 = 96;
/// Gaussian half-thickness of the galactic band, radians.
const BAND_SIGMA: f32 = 0.17;

/// Orientation of the invented host galaxy on the sky.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GalacticFrame {
    /// Unit vector toward the galactic pole.
    pub pole: Vec3,
    /// Unit vector toward the galactic centre, perpendicular to `pole`.
    pub center: Vec3,
}

impl GalacticFrame {
    pub fn from_seed(seed: u64) -> Self {
        let mut rng = RngStream::new(seed, ModuleId::VISUAL);
        let pole = random_direction(&mut rng);
        let other = random_direction(&mut rng);
        // Any direction not parallel to the pole gives a centre in the plane.
        let center = (other - pole * other.dot(pole)).normalize_or(pole.any_orthonormal_vector());
        Self { pole, center }
    }

    /// Galactic latitude of `dir`, radians: 0 in the plane, ±π/2 at the poles.
    pub fn latitude(&self, dir: Vec3) -> f32 {
        dir.dot(self.pole).clamp(-1.0, 1.0).asin()
    }

    /// Cosine of the angle between `dir` and the galactic centre.
    pub fn toward_center(&self, dir: Vec3) -> f32 {
        dir.dot(self.center)
    }
}

/// Uniform on the unit sphere.
fn random_direction(rng: &mut RngStream) -> Vec3 {
    let z = rng.uniform_range(-1.0, 1.0) as f32;
    let phi = rng.uniform_range(0.0, f64::from(TAU)) as f32;
    let r = (1.0 - z * z).max(0.0).sqrt();
    Vec3::new(r * phi.cos(), z, r * phi.sin())
}

/// Direction for texture coordinates `(u, v)` in `[0, 1]` of the sky dome.
/// The dome's mesh and its texture both use this, so they cannot disagree.
pub fn dome_direction(u: f32, v: f32) -> Vec3 {
    let longitude = u * TAU;
    let colatitude = v * PI;
    Vec3::new(
        colatitude.sin() * longitude.cos(),
        colatitude.cos(),
        colatitude.sin() * longitude.sin(),
    )
}

fn hash3(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let mut h = seed
        ^ (x as u32).wrapping_mul(0x8DA6_B343)
        ^ (y as u32).wrapping_mul(0xD816_3841)
        ^ (z as u32).wrapping_mul(0xCB1A_B31F);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// Smooth value noise in `[0, 1]`.
fn noise3(p: Vec3, seed: u32) -> f32 {
    let cell = p.floor();
    let f = p - cell;
    let w = f * f * (Vec3::splat(3.0) - 2.0 * f);
    let (x, y, z) = (cell.x as i32, cell.y as i32, cell.z as i32);
    let corner = |dx: i32, dy: i32, dz: i32| hash3(x + dx, y + dy, z + dz, seed);
    let along_x =
        |dy: i32, dz: i32| corner(0, dy, dz) + (corner(1, dy, dz) - corner(0, dy, dz)) * w.x;
    let along_y = |dz: i32| along_x(0, dz) + (along_x(1, dz) - along_x(0, dz)) * w.y;
    along_y(0) + (along_y(1) - along_y(0)) * w.z
}

/// Fractal noise in `[0, 1]`: `octaves` layers, each twice as fine and half
/// as strong as the last.
pub fn fbm(p: Vec3, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut amplitude, mut total, mut q) = (0.0, 0.5, 0.0, p);
    for octave in 0..octaves {
        sum += amplitude * noise3(q, seed.wrapping_add(octave));
        total += amplitude;
        amplitude *= 0.5;
        q *= 2.03;
    }
    sum / total
}

fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How much of the light from behind gets through the galaxy's dust in
/// direction `dir`: 1 away from the plane, down to about 0.1 in the lanes.
pub fn dust_transmission(frame: &GalacticFrame, dir: Vec3, seed: u32) -> f32 {
    let latitude = frame.latitude(dir);
    // Lanes hug the plane, in a thinner layer than the stars.
    let layer = (-(latitude / (0.45 * BAND_SIGMA)).powi(2)).exp();
    let lanes = smoothstep(
        0.42,
        0.70,
        fbm(dir * 5.0 + Vec3::splat(11.0), 5, seed ^ 0xD057),
    );
    1.0 - 0.9 * layer * lanes
}

/// Linear RGB of the diffuse sky in direction `dir`: unresolved starlight of
/// the galactic band, dimmed by dust, plus faint glowing clouds.
pub fn dome_color(frame: &GalacticFrame, dir: Vec3, seed: u32) -> Vec3 {
    let latitude = frame.latitude(dir);
    let toward_center = frame.toward_center(dir);

    // The band is brighter and thicker toward the galactic centre.
    let bulge = smoothstep(0.55, 1.0, toward_center);
    let sigma = BAND_SIGMA * (1.0 + 0.9 * bulge);
    let band = (-(latitude / sigma).powi(2)).exp() * (0.45 + 0.35 * toward_center + 0.9 * bulge);
    let mottle = 0.55 + 0.9 * fbm(dir * 3.0, 5, seed);
    let starlight = Vec3::new(0.060, 0.056, 0.064).lerp(Vec3::new(0.085, 0.066, 0.045), bulge);
    let mut color = starlight * band * mottle * dust_transmission(frame, dir, seed);

    // Emission clouds (red, hydrogen) and reflection clouds (blue), in
    // patches near the plane.
    let near_plane = (-(latitude / (1.6 * BAND_SIGMA)).powi(2)).exp();
    let red = smoothstep(
        0.60,
        0.82,
        fbm(dir * 4.0 + Vec3::splat(37.0), 5, seed ^ 0xE1),
    );
    let blue = smoothstep(
        0.62,
        0.85,
        fbm(dir * 3.5 + Vec3::splat(71.0), 5, seed ^ 0xB2),
    );
    color += Vec3::new(0.070, 0.018, 0.028) * red * near_plane;
    color += Vec3::new(0.014, 0.030, 0.065) * blue * (0.25 + 0.75 * near_plane);

    // Very faint glow everywhere, so the sky is never a flat black.
    let haze = fbm(dir * 1.7 + Vec3::splat(5.0), 4, seed ^ 0x4A);
    color + Vec3::new(0.0028, 0.0032, 0.0060) * (0.4 + haze)
}

fn linear_to_srgb8(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let encoded = if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0 + 0.5) as u8
}

/// The dome texture: `width × height` sRGB RGBA pixels, equirectangular,
/// laid out by [`dome_direction`].
pub fn dome_pixels(frame: &GalacticFrame, seed: u32, width: u32, height: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let v = (y as f32 + 0.5) / height as f32;
        for x in 0..width {
            let u = (x as f32 + 0.5) / width as f32;
            let c = dome_color(frame, dome_direction(u, v), seed);
            data.extend_from_slice(&[
                linear_to_srgb8(c.x),
                linear_to_srgb8(c.y),
                linear_to_srgb8(c.z),
                255,
            ]);
        }
    }
    data
}

/// A background star.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyStar {
    pub dir: Vec3,
    /// Linear RGB, already scaled by brightness; may exceed 1 so the
    /// brightest stars bloom.
    pub color: Vec3,
    /// Angular half-size as drawn, radians.
    pub half_size: f32,
}

/// `count` background stars. They are drawn where a rejection test against
/// the band's density and the dust lets them through, so they crowd toward
/// the galactic plane and thin out in the dark lanes.
pub fn star_field(frame: &GalacticFrame, seed: u64, count: usize) -> Vec<SkyStar> {
    let mut rng = RngStream::new(seed ^ 0x5741_5253, ModuleId::VISUAL);
    let dust_seed = seed as u32;
    let mut stars = Vec::with_capacity(count);
    while stars.len() < count {
        let dir = random_direction(&mut rng);
        let latitude = frame.latitude(dir);
        let density = 0.22 + 0.78 * (-(latitude / (1.5 * BAND_SIGMA)).powi(2)).exp();
        let keep = density * dust_transmission(frame, dir, dust_seed);
        if rng.uniform() as f32 > keep {
            continue;
        }
        // Most stars are faint: brightness falls steeply from a few bright ones.
        let rank = rng.uniform() as f32;
        let brightness = 0.10 + 3.4 * rank.powi(9) + 0.35 * rank.powi(2);
        // Temperatures from red dwarfs to hot blue stars, cool ones commonest.
        let teff_k = 3000.0 + 9000.0 * rng.uniform().powi(2);
        let [r, g, b] = sg_stellar::blackbody_linear_srgb(Kelvin(teff_k));
        stars.push(SkyStar {
            dir,
            color: Vec3::new(r as f32, g as f32, b as f32) * brightness,
            half_size: 0.0011 + 0.0016 * rank.powi(6),
        });
    }
    stars
}

/// One mesh holding a small square for every star, each facing the centre
/// of the unit sphere it sits on.
fn star_mesh(stars: &[SkyStar]) -> Mesh {
    let mut positions = Vec::with_capacity(stars.len() * 4);
    let mut uvs = Vec::with_capacity(stars.len() * 4);
    let mut colors = Vec::with_capacity(stars.len() * 4);
    let mut indices = Vec::with_capacity(stars.len() * 6);
    for star in stars {
        let (right, up) = star.dir.any_orthonormal_pair();
        let base = positions.len() as u32;
        for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let p = star.dir + (right * sx + up * sy) * star.half_size;
            positions.push(p.to_array());
            uvs.push([0.5 + 0.5 * sx, 0.5 + 0.5 * sy]);
            colors.push([star.color.x, star.color.y, star.color.z, 1.0]);
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
}

/// The inside of a unit sphere, textured by [`dome_direction`].
fn dome_mesh() -> Mesh {
    const COLUMNS: u32 = 64;
    const ROWS: u32 = 32;
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    for row in 0..=ROWS {
        for column in 0..=COLUMNS {
            let (u, v) = (column as f32 / COLUMNS as f32, row as f32 / ROWS as f32);
            positions.push(dome_direction(u, v).to_array());
            uvs.push([u, v]);
        }
    }
    for row in 0..ROWS {
        for column in 0..COLUMNS {
            let a = row * (COLUMNS + 1) + column;
            let b = a + COLUMNS + 1;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

fn texture(width: u32, height: u32, data: Vec<u8>) -> Image {
    Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// A soft round dot: white, with alpha falling from 1 at the centre to 0 at
/// the edge.
fn dot_pixels(size: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let p = (Vec2::new(x as f32, y as f32) + 0.5) / size as f32 * 2.0 - 1.0;
            let alpha = (-(p.length() / 0.42).powi(2)).exp() * smoothstep(1.0, 0.75, p.length());
            data.extend_from_slice(&[255, 255, 255, (alpha * 255.0 + 0.5) as u8]);
        }
    }
    data
}

/// The kinds of distant galaxy drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GalaxyKind {
    Spiral,
    Elliptical,
}

/// A distant galaxy on the sky.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyGalaxy {
    pub dir: Vec3,
    pub kind: GalaxyKind,
    /// Angular half-size, radians.
    pub half_size: f32,
    /// Rotation of the image on the sky, radians.
    pub position_angle: f32,
    /// Minor-to-major axis ratio as seen: 1 face-on, small edge-on.
    pub axis_ratio: f32,
    pub brightness: f32,
    pub seed: u32,
}

/// `count` galaxies, kept away from the galactic plane, where the host
/// galaxy's dust would hide them (the "zone of avoidance").
pub fn galaxies(frame: &GalacticFrame, seed: u64, count: usize) -> Vec<SkyGalaxy> {
    let mut rng = RngStream::new(seed ^ 0x4741_4C58, ModuleId::VISUAL);
    let mut out = Vec::with_capacity(count);
    while out.len() < count {
        let dir = random_direction(&mut rng);
        if frame.latitude(dir).abs() < 2.2 * BAND_SIGMA {
            continue;
        }
        let kind = if rng.uniform() < 0.65 {
            GalaxyKind::Spiral
        } else {
            GalaxyKind::Elliptical
        };
        let size_rank = rng.uniform() as f32;
        out.push(SkyGalaxy {
            dir,
            kind,
            half_size: 0.010 + 0.034 * size_rank.powi(3),
            position_angle: rng.uniform_range(0.0, f64::from(TAU)) as f32,
            axis_ratio: match kind {
                GalaxyKind::Spiral => rng.uniform_range(0.18, 1.0) as f32,
                GalaxyKind::Elliptical => rng.uniform_range(0.55, 1.0) as f32,
            },
            brightness: rng.uniform_range(0.35, 0.8) as f32,
            seed: rng.next_u64() as u32,
        });
    }
    out
}

/// Linear RGBA of a galaxy's image at `p` in `[-1, 1]²`, before brightness.
/// Fades to nothing at the edge of the unit circle.
pub fn galaxy_color(galaxy: &SkyGalaxy, p: Vec2) -> Vec3 {
    let q = Vec2::new(p.x, p.y / galaxy.axis_ratio);
    let r = q.length();
    let edge = smoothstep(1.0, 0.7, r);
    let warm = Vec3::new(1.0, 0.86, 0.66);
    match galaxy.kind {
        GalaxyKind::Elliptical => warm * (-(r / 0.16).sqrt() * 1.6).exp() * 1.6 * edge,
        GalaxyKind::Spiral => {
            let theta = q.y.atan2(q.x);
            // Two logarithmic arms; the pitch varies from galaxy to galaxy.
            let pitch = 0.22 + 0.16 * hash3(1, 0, 0, galaxy.seed);
            let phase = 2.0 * (theta - (r + 0.02).ln() / pitch);
            let arms = 0.5 + 0.5 * phase.cos();
            let clumps = 0.6 + 0.8 * fbm(Vec3::new(q.x, q.y, 0.0) * 9.0, 3, galaxy.seed);
            let disk = (-r / 0.26).exp() * (0.25 + 0.75 * arms * arms) * clumps;
            let bulge = (-(r / 0.10).powi(2)).exp();
            (Vec3::new(0.62, 0.74, 1.0) * disk * 0.9 + warm * bulge) * edge
        }
    }
}

fn galaxy_pixels(galaxy: &SkyGalaxy, size: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let p = (Vec2::new(x as f32, y as f32) + 0.5) / size as f32 * 2.0 - 1.0;
            let c = galaxy_color(galaxy, p) * galaxy.brightness;
            data.extend_from_slice(&[
                linear_to_srgb8(c.x),
                linear_to_srgb8(c.y),
                linear_to_srgb8(c.z),
                255,
            ]);
        }
    }
    data
}

/// A unit square in the xy plane, used for every galaxy.
fn quad_mesh() -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [1.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0],
        ],
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
    )
    .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]))
}

/// The seed of a system's sky: from the parameters that produced it, so the
/// same run always has the same sky.
pub fn sky_seed(params_hash: &str) -> u64 {
    fnv1a64(params_hash.as_bytes())
}

/// Parent of everything in the sky. It sits on the camera, so the sky never
/// gets closer.
#[derive(Component)]
pub(crate) struct SkyRoot;

#[derive(Resource, Default)]
pub(crate) struct SkyState {
    generation: u64,
    root: Option<Entity>,
}

/// An unlit material that ignores the scene's light and adds to what is
/// behind it.
fn glow(image: Handle<Image>) -> StandardMaterial {
    StandardMaterial {
        base_color_texture: Some(image),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        cull_mode: None,
        ..default()
    }
}

/// Builds the sky of a new system, and keeps it centred on the camera.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sync_sky(
    mut commands: Commands,
    view: Res<SystemView>,
    settings: Res<ViewSettings>,
    mut state: ResMut<SkyState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    camera: Single<&Transform, (With<Camera3d>, Without<SkyRoot>)>,
    mut roots: Query<(&mut Transform, &mut Visibility), With<SkyRoot>>,
) {
    let Some(result) = view.result() else {
        return;
    };

    if state.generation != view.generation() || state.root.is_none() {
        if let Some(root) = state.root.take() {
            commands.entity(root).despawn();
        }
        state.generation = view.generation();

        let seed = sky_seed(&result.snapshot.params_hash);
        let frame = GalacticFrame::from_seed(seed);
        let no_cull = || (NoFrustumCulling, NotShadowCaster);

        let root = commands
            .spawn((SkyRoot, Transform::default(), Visibility::Inherited))
            .id();

        let dome = images.add(texture(
            DOME_WIDTH,
            DOME_HEIGHT,
            dome_pixels(&frame, seed as u32, DOME_WIDTH, DOME_HEIGHT),
        ));
        commands.spawn((
            ChildOf(root),
            Mesh3d(meshes.add(dome_mesh())),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color_texture: Some(dome),
                unlit: true,
                cull_mode: None,
                ..default()
            })),
            Transform::default(),
            no_cull(),
        ));

        let dot = images.add(texture(32, 32, dot_pixels(32)));
        commands.spawn((
            ChildOf(root),
            Mesh3d(meshes.add(star_mesh(&star_field(&frame, seed, STAR_COUNT)))),
            MeshMaterial3d(materials.add(glow(dot))),
            // Just inside the dome.
            Transform::from_scale(Vec3::splat(0.98)),
            no_cull(),
        ));

        let quad = meshes.add(quad_mesh());
        for galaxy in galaxies(&frame, seed, GALAXY_COUNT) {
            let image = images.add(texture(
                GALAXY_TEXTURE_SIZE,
                GALAXY_TEXTURE_SIZE,
                galaxy_pixels(&galaxy, GALAXY_TEXTURE_SIZE),
            ));
            // The quad faces the centre of the sphere, turned by its
            // position angle.
            let facing = Transform::from_translation(galaxy.dir * 0.99)
                .looking_at(Vec3::ZERO, galaxy.dir.any_orthonormal_vector());
            commands.spawn((
                ChildOf(root),
                Mesh3d(quad.clone()),
                MeshMaterial3d(materials.add(glow(image))),
                Transform {
                    rotation: facing.rotation * Quat::from_rotation_z(galaxy.position_angle),
                    scale: Vec3::splat(galaxy.half_size),
                    ..facing
                },
                no_cull(),
            ));
        }
        state.root = Some(root);
        return;
    }

    for (mut transform, mut visibility) in &mut roots {
        transform.translation = camera.translation;
        transform.scale = Vec3::splat(SKY_RADIUS);
        let wanted = if settings.show_backdrop {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> GalacticFrame {
        GalacticFrame::from_seed(42)
    }

    #[test]
    fn galactic_frame_is_orthonormal_and_repeats() {
        for seed in 1..200 {
            let f = GalacticFrame::from_seed(seed);
            assert!((f.pole.length() - 1.0).abs() < 1e-5);
            assert!((f.center.length() - 1.0).abs() < 1e-5);
            assert!(f.pole.dot(f.center).abs() < 1e-5);
            assert_eq!(f, GalacticFrame::from_seed(seed));
        }
        assert!((frame().latitude(frame().pole) - PI / 2.0).abs() < 1e-3);
        assert!(frame().latitude(frame().center).abs() < 1e-5);
    }

    #[test]
    fn dome_coordinates_cover_the_sphere() {
        assert!((dome_direction(0.3, 0.0) - Vec3::Y).length() < 1e-6);
        assert!((dome_direction(0.3, 1.0) + Vec3::Y).length() < 1e-6);
        assert!((dome_direction(0.0, 0.5) - Vec3::X).length() < 1e-6);
        assert!((dome_direction(0.25, 0.5) - Vec3::Z).length() < 1e-6);
        // The seam: u = 0 and u = 1 are the same direction.
        assert!((dome_direction(0.0, 0.4) - dome_direction(1.0, 0.4)).length() < 1e-5);
    }

    #[test]
    fn noise_is_bounded_and_continuous() {
        for k in 0..2000 {
            let p = Vec3::new(k as f32 * 0.137, k as f32 * 0.071, -(k as f32) * 0.053);
            let n = fbm(p, 5, 9);
            assert!((0.0..=1.0).contains(&n), "{n}");
            let step = fbm(p + Vec3::splat(1.0e-3), 5, 9);
            assert!((n - step).abs() < 0.05);
        }
    }

    /// The band is the brightest part of the diffuse sky, and the sky stays
    /// dim everywhere: it is a backdrop.
    #[test]
    fn dome_is_dim_and_brightest_in_the_band() {
        let f = frame();
        let mean = |latitude_from_pole: f32| {
            let mut sum = 0.0;
            for k in 0..360 {
                let angle = (k as f32).to_radians();
                let (a, b) = f.pole.any_orthonormal_pair();
                let in_plane = a * angle.cos() + b * angle.sin();
                let dir = (f.pole * latitude_from_pole.cos() + in_plane * latitude_from_pole.sin())
                    .normalize();
                let c = dome_color(&f, dir, 42);
                assert!(c.min_element() >= 0.0 && c.max_element() < 0.35, "{c}");
                sum += c.element_sum();
            }
            sum / 360.0
        };
        assert!(mean(PI / 2.0) > 4.0 * mean(0.3));
    }

    #[test]
    fn dome_texture_has_the_right_size_and_is_opaque() {
        let data = dome_pixels(&frame(), 42, 64, 32);
        assert_eq!(data.len(), 64 * 32 * 4);
        assert!(data.iter().skip(3).step_by(4).all(|alpha| *alpha == 255));
        assert_eq!(data, dome_pixels(&frame(), 42, 64, 32));
    }

    #[test]
    fn stars_crowd_toward_the_galactic_plane() {
        let f = frame();
        let stars = star_field(&f, 42, 6000);
        assert_eq!(stars, star_field(&f, 42, 6000));
        // Two zones of equal area: within 30° of the plane (sin 30° = 0.5).
        let near = stars
            .iter()
            .filter(|s| f.latitude(s.dir).abs() < PI / 6.0)
            .count();
        assert!(near as f32 > 0.6 * stars.len() as f32, "{near}");
        for star in &stars {
            assert!((star.dir.length() - 1.0).abs() < 1e-4);
            assert!(star.color.min_element() >= 0.0 && star.color.max_element() < 5.0);
            assert!((0.001..0.003).contains(&star.half_size));
        }
        // Mostly faint: the median star is well under full brightness.
        let mut peaks: Vec<f32> = stars.iter().map(|s| s.color.max_element()).collect();
        peaks.sort_by(f32::total_cmp);
        assert!(peaks[peaks.len() / 2] < 0.3);
    }

    #[test]
    fn star_mesh_has_a_quad_per_star() {
        let stars = star_field(&frame(), 42, 100);
        let mesh = star_mesh(&stars);
        assert_eq!(mesh.count_vertices(), 400);
        assert_eq!(mesh.indices().map(|i| i.len()), Some(600));
    }

    #[test]
    fn galaxies_avoid_the_galactic_plane() {
        let f = frame();
        let list = galaxies(&f, 42, 40);
        assert_eq!(list.len(), 40);
        for galaxy in &list {
            assert!(f.latitude(galaxy.dir).abs() >= 2.2 * BAND_SIGMA);
            assert!((0.01..0.045).contains(&galaxy.half_size));
            // Bright in the middle, gone at the edge.
            assert!(galaxy_color(galaxy, Vec2::ZERO).max_element() > 0.5);
            assert_eq!(galaxy_color(galaxy, Vec2::new(1.0, 0.0)), Vec3::ZERO);
            assert_eq!(galaxy_color(galaxy, Vec2::new(0.8, 0.8)), Vec3::ZERO);
        }
        assert!(list.iter().any(|g| g.kind == GalaxyKind::Spiral));
        assert!(list.iter().any(|g| g.kind == GalaxyKind::Elliptical));
    }
}
