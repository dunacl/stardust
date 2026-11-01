//! The planet material: one shader ([`planet.wesl`](../planet.wesl)) that
//! draws every kind of surface and the air around it.
//!
//! [`crate::surface`] decides how a body looks; this module packs that into
//! the shader's uniform and keeps the per-frame parts (where the stars are,
//! where the camera is, how large the body is on screen) up to date.

use crate::surface::{self, Appearance};
use bevy::asset::embedded_asset;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

/// Most stars that light one body: the members of a hierarchical triple.
pub const MAX_STARS: usize = 3;

/// Brightness of a fully lit white surface before tone mapping. Planets
/// are exposed alike wherever they are (a real one at 30 AU receives a
/// thousandth of the light of one at 1 AU and would be invisible beside
/// it): a drawing convention.
const EXPOSURE: f32 = 1.15;

/// The shader's uniform; see `planet.wesl` for the meaning of each field.
#[derive(Debug, Clone, Default, ShaderType)]
pub struct PlanetUniform {
    pub eye: Vec4,
    pub frame_x: Vec4,
    pub frame_y: Vec4,
    pub frame_z: Vec4,
    pub star_dir: [Vec4; MAX_STARS],
    pub star_color: [Vec4; MAX_STARS],
    pub color_a: Vec4,
    pub color_b: Vec4,
    pub color_c: Vec4,
    pub ocean_color: Vec4,
    pub air: Vec4,
    pub glow: Vec4,
    pub cover: Vec4,
    pub shape: Vec4,
    pub flags: Vec4,
    pub extra: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct PlanetMaterial {
    #[uniform(0)]
    pub uniform: PlanetUniform,
    /// The shell of air rather than the surface.
    pub shell: bool,
}

impl Material for PlanetMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://sg_render/planet.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        if self.shell {
            AlphaMode::Premultiplied
        } else {
            AlphaMode::Opaque
        }
    }
}

/// One star as it lights a body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StarLighting {
    /// Unit vector from the body to the star.
    pub direction: Vec3,
    pub color: Vec3,
    /// Its share of the light the body receives, 0 to 1.
    pub share: f32,
}

/// Shares of the light a body receives from stars of the given luminosity
/// at the given distances: each in proportion to `L/d²`, summing to 1.
pub fn light_shares(stars: &[(f64, f64)]) -> Vec<f32> {
    let flux: Vec<f64> = stars
        .iter()
        .map(|(luminosity, distance)| luminosity / (distance * distance).max(f64::MIN_POSITIVE))
        .collect();
    let total: f64 = flux.iter().sum();
    flux.iter()
        .map(|f| if total > 0.0 { (f / total) as f32 } else { 0.0 })
        .collect()
}

fn hash3(p: Vec3) -> f32 {
    let mut q = (p * 0.1031).fract_gl();
    q += Vec3::splat(q.dot(Vec3::new(q.z, q.y, q.x) + 31.32));
    let h = (q.x + q.y) * q.z;
    h - h.floor()
}

fn value_noise(p: Vec3) -> f32 {
    let i = p.floor();
    let f = p - i;
    let u = f * f * (Vec3::splat(3.0) - 2.0 * f);
    let at = |x: f32, y: f32, z: f32| hash3(i + Vec3::new(x, y, z));
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let a = lerp(at(0.0, 0.0, 0.0), at(1.0, 0.0, 0.0), u.x);
    let b = lerp(at(0.0, 1.0, 0.0), at(1.0, 1.0, 0.0), u.x);
    let c = lerp(at(0.0, 0.0, 1.0), at(1.0, 0.0, 1.0), u.x);
    let d = lerp(at(0.0, 1.0, 1.0), at(1.0, 1.0, 1.0), u.x);
    lerp(lerp(a, b, u.y), lerp(c, d, u.y), u.z)
}

/// The shader's fractal noise, on the CPU: used to calibrate
/// [`noise_quantile`], not to draw.
pub fn fbm(p: Vec3, octaves: u32) -> f32 {
    let (mut sum, mut norm, mut amp, mut q) = (0.0, 0.0, 0.5, p);
    for _ in 0..octaves.min(9) {
        sum += amp * value_noise(q);
        norm += amp;
        amp *= 0.5;
        q = q * 2.03 + Vec3::new(17.1, 3.7, 9.2);
    }
    sum / norm.max(1e-5)
}

/// Value of the shader's fractal noise below which a fraction 0, 0.05, …, 1
/// of a sphere's surface lies (five octaves; measured by the test
/// `quantiles_match_the_noise`).
const NOISE_QUANTILES: [f32; 21] = [
    0.138, 0.316, 0.349, 0.374, 0.396, 0.416, 0.434, 0.451, 0.466, 0.480, 0.496, 0.512, 0.529,
    0.545, 0.562, 0.581, 0.601, 0.624, 0.652, 0.691, 0.839,
];

/// Level of the shader's noise below which `fraction` of the surface lies:
/// where to put sea level so that the drawn ocean covers the fraction the
/// model gives. Outside the open interval the result is out of the noise's
/// range, meaning "none" (−1) or "all" (2).
pub fn noise_quantile(fraction: f32) -> f32 {
    if fraction <= 0.001 {
        return -1.0;
    }
    if fraction >= 0.999 {
        return 2.0;
    }
    let x = fraction * 20.0;
    let i = (x.floor() as usize).min(19);
    NOISE_QUANTILES[i] + (NOISE_QUANTILES[i + 1] - NOISE_QUANTILES[i]) * (x - i as f32)
}

/// The parts of the uniform that do not change from frame to frame.
pub fn uniform_for(look: &Appearance, shell: bool) -> PlanetUniform {
    let rgb = |c: [f32; 3], a: f32| Vec4::new(c[0], c[1], c[2], a);
    let air = look.air;
    PlanetUniform {
        color_a: rgb(look.colors[0], 1.0),
        color_b: rgb(look.colors[1], 1.0),
        color_c: rgb(look.colors[2], 1.0),
        ocean_color: rgb(look.ocean_color, 1.0),
        air: air.map_or(Vec4::ZERO, |a| rgb(a.color, a.density)),
        glow: rgb(look.glow_color, look.glow),
        cover: Vec4::new(
            noise_quantile(look.ocean_frac),
            surface::ice_cap_sin_latitude(look.ice_frac),
            // Cloud lies above its level, so it covers the top fraction.
            match noise_quantile(1.0 - look.cloud_frac) {
                q if q < 0.0 => -1.0,
                q => q,
            },
            look.seed,
        ),
        shape: Vec4::new(
            look.kind as u32 as f32,
            look.band_count,
            look.band_contrast,
            look.storms,
        ),
        flags: Vec4::new(
            f32::from(u8::from(look.locked)),
            f32::from(u8::from(shell)),
            100.0,
            air.map_or(0.0, |a| a.height),
        ),
        extra: Vec4::new(0.0, EXPOSURE, 0.0, 0.0),
        ..default()
    }
}

/// The planet's frame in world space. A locked body keeps longitude 0
/// under its star; any other turns about its axis by `spin` radians. The
/// axis is tilted from the orbit's normal (+Y) by the obliquity, toward a
/// direction fixed by the body's seed, since no tier gives one.
pub fn frame(look: &Appearance, to_star: Vec3, spin: f32) -> [Vec3; 3] {
    if look.locked {
        let x = to_star.normalize_or(Vec3::X);
        let z = x.cross(Vec3::Y).normalize_or(Vec3::Z);
        return [x, z.cross(x), z];
    }
    let turn = axis_tilt(look) * Quat::from_rotation_y(spin);
    [turn * Vec3::X, turn * Vec3::Y, turn * Vec3::Z]
}

/// The rotation that takes the orbit's normal (+Y) to the body's spin
/// axis: its obliquity, toward a direction fixed by its seed. Its moons
/// and rings lie in the plane this tilts.
pub fn axis_tilt(look: &Appearance) -> Quat {
    Quat::from_rotation_y(look.seed) * Quat::from_rotation_x(look.obliquity_deg.to_radians())
}

/// Fills in the parts of the uniform that change every frame.
pub fn update_uniform(
    uniform: &mut PlanetUniform,
    look: &Appearance,
    eye: Vec3,
    stars: &[StarLighting],
    spin: f32,
    apparent_radius_pixels: f32,
) {
    uniform.eye = eye.extend(1.0);
    let brightest = stars
        .iter()
        .max_by(|a, b| a.share.total_cmp(&b.share))
        .map_or(Vec3::X, |s| s.direction);
    let [x, y, z] = frame(look, brightest, spin);
    uniform.frame_x = x.extend(0.0);
    uniform.frame_y = y.extend(0.0);
    uniform.frame_z = z.extend(0.0);
    for k in 0..MAX_STARS {
        let (dir, color) = stars.get(k).map_or((Vec4::ZERO, Vec4::ZERO), |s| {
            (s.direction.extend(s.share), s.color.extend(1.0))
        });
        uniform.star_dir[k] = dir;
        uniform.star_color[k] = color;
    }
    // A locked world's "first star" in the shader is the one it faces.
    if look.locked && stars.len() > 1 {
        let first = stars
            .iter()
            .position(|s| s.direction == brightest)
            .unwrap_or(0);
        uniform.star_dir.swap(0, first);
        uniform.star_color.swap(0, first);
    }
    uniform.flags.z = apparent_radius_pixels;
    // Clouds drift a little faster than the ground turns.
    uniform.extra.x = 0.35 * spin;
}

pub(crate) fn build(app: &mut App) {
    embedded_asset!(app, "planet.wesl");
    app.add_plugins(MaterialPlugin::<PlanetMaterial>::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Points spread evenly over the unit sphere (a Fibonacci lattice).
    fn sphere_points(count: usize) -> impl Iterator<Item = Vec3> {
        let golden = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
        (0..count).map(move |k| {
            let y = 1.0 - 2.0 * (k as f32 + 0.5) / count as f32;
            let r = (1.0 - y * y).sqrt();
            let a = golden * k as f32;
            Vec3::new(r * a.cos(), y, r * a.sin())
        })
    }

    fn samples() -> Vec<f32> {
        let mut values = Vec::new();
        for seed in [0.0_f32, 123.4, 567.8, 910.1] {
            let offset = Vec3::new(seed * 0.37, seed * 0.11, seed * 0.23);
            values.extend(sphere_points(20_000).map(|p| fbm((p + offset) * 1.6, 5)));
        }
        values.sort_by(f32::total_cmp);
        values
    }

    #[test]
    fn quantiles_match_the_noise() {
        let values = samples();
        let measured: Vec<f32> = (0..=20)
            .map(|k| values[(k * (values.len() - 1)) / 20])
            .collect();
        for (k, (table, now)) in NOISE_QUANTILES.iter().zip(&measured).enumerate() {
            assert!(
                (table - now).abs() < 0.01,
                "quantile {k}: table {table}, measured {now}; all: {measured:?}"
            );
        }
    }

    #[test]
    fn the_drawn_ocean_covers_the_fraction_asked_for() {
        let values = samples();
        for fraction in [0.1_f32, 0.3, 0.5, 0.71, 0.9] {
            let level = noise_quantile(fraction);
            let below = values.partition_point(|v| *v < level) as f32 / values.len() as f32;
            assert!((below - fraction).abs() < 0.02, "{fraction}: {below}");
        }
        assert!(noise_quantile(0.0) < 0.0 && noise_quantile(1.0) > 1.0);
    }

    #[test]
    fn light_is_shared_by_flux() {
        // Two equal stars, one twice as far: it gives a fifth of the light.
        let shares = light_shares(&[(1.0, 1.0), (1.0, 2.0)]);
        assert!((shares[0] - 0.8).abs() < 1e-6 && (shares[1] - 0.2).abs() < 1e-6);
        assert_eq!(light_shares(&[(2.5, 3.0)]), vec![1.0]);
        assert!(light_shares(&[]).is_empty());
    }

    #[test]
    fn a_locked_body_faces_its_star_and_others_tilt() {
        let mut look = surface::embryo_appearance(1);
        look.locked = true;
        let to_star = Vec3::new(0.6, 0.0, -0.8);
        let [x, y, z] = frame(&look, to_star, 1.0);
        assert!((x - to_star).length() < 1e-6);
        assert!(y.dot(Vec3::Y) > 0.999 && z.dot(x).abs() < 1e-6);

        look.locked = false;
        look.obliquity_deg = 23.4;
        let [x, y, z] = frame(&look, to_star, 2.0);
        // Orthonormal, with the pole 23.4° from the orbit's normal.
        assert!(x.dot(y).abs() < 1e-5 && y.dot(z).abs() < 1e-5 && (x.cross(y) - z).length() < 1e-5);
        assert!((y.dot(Vec3::Y).acos().to_degrees() - 23.4).abs() < 1e-3);
        // Spinning moves longitude 0 but not the pole.
        assert!((frame(&look, to_star, 2.5)[1] - y).length() < 1e-6);
    }

    #[test]
    fn uniform_carries_the_appearance() {
        let look = surface::embryo_appearance(7);
        let u = uniform_for(&look, false);
        assert_eq!(u.shape.x, 3.0);
        assert_eq!(u.flags.y, 0.0);
        // No ocean and no cloud: levels out of the noise's range.
        assert!(u.cover.x < 0.0 && u.cover.z > 1.5);
        assert_eq!(uniform_for(&look, true).flags.y, 1.0);
    }
}
