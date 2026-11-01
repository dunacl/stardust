//! The star material: limb darkening and granulation
//! ([`star.wesl`](../star.wesl)).

use bevy::asset::embedded_asset;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

/// Brightness of the centre of a star's disc before tone mapping, when the
/// star is small on screen (enough to bloom) and when it fills the view
/// (low enough for its surface to show), as a camera's exposure would
/// adapt. An exposure convention.
const DISC_BRIGHTNESS: f32 = 40.0;
const DISC_BRIGHTNESS_CLOSE: f32 = 1.7;
/// Apparent radii, pixels, between which the exposure adapts.
const EXPOSURE_ADAPTS_OVER: (f32, f32) = (25.0, 140.0);

/// Brightness of the disc's centre for a star of this apparent radius.
pub fn disc_brightness(apparent_radius_pixels: f32) -> f32 {
    let (near, far) = EXPOSURE_ADAPTS_OVER;
    let x = ((apparent_radius_pixels.max(1.0).ln() - near.ln()) / (far.ln() - near.ln()))
        .clamp(0.0, 1.0);
    // Interpolate in stops, like an exposure.
    (DISC_BRIGHTNESS.ln() + (DISC_BRIGHTNESS_CLOSE.ln() - DISC_BRIGHTNESS.ln()) * x).exp()
}

/// The Sun's linear limb-darkening coefficients near 650, 550 and 450 nm
/// (Cox 2000, *Allen's Astrophysical Quantities*, 4th ed., §14.7: about
/// 0.52, 0.65 and 0.83). The limb is darker, and redder, than the centre.
/// Used for every star: the coefficients vary with temperature and gravity
/// (Claret 2000, A&A 363, 1081), by less than the difference between the
/// colours here.
pub const SOLAR_LIMB_DARKENING: [f32; 3] = [0.52, 0.65, 0.83];

/// Strength of the granulation drawn on a star of this effective
/// temperature, 0 to 1.
///
/// Granules are the tops of convection cells, so they need a convective
/// envelope. That thins out between spectral types F5 and A7 (about 6500 to
/// 7800 K; Gray 2005, *The Observation and Analysis of Stellar
/// Photospheres*, ch. 17), above which the surface is radiative and smooth.
pub fn granulation(teff_k: f64) -> f32 {
    let x = ((7800.0 - teff_k) / (7800.0 - 6500.0)).clamp(0.0, 1.0);
    (x * x * (3.0 - 2.0 * x)) as f32
}

/// The shader's uniform; see `star.wesl`.
#[derive(Debug, Clone, Default, ShaderType)]
pub struct StarUniform {
    pub eye: Vec4,
    pub color: Vec4,
    pub limb: Vec4,
    pub detail: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct StarMaterial {
    #[uniform(0)]
    pub uniform: StarUniform,
}

impl Material for StarMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://sg_render/star.wesl".into()
    }
}

impl StarMaterial {
    /// A star of the given colour (linear RGB) and temperature.
    pub fn new(color: LinearRgba, teff_k: f64, seed: f32) -> Self {
        let [r, g, b] = SOLAR_LIMB_DARKENING;
        Self {
            uniform: StarUniform {
                eye: Vec4::ZERO,
                color: Vec4::new(color.red, color.green, color.blue, DISC_BRIGHTNESS),
                limb: Vec4::new(r, g, b, 0.0),
                // The contrast drawn is a quarter at most: the Sun's
                // granules differ by 10–20% in brightness.
                detail: Vec4::new(0.25 * granulation(teff_k), 0.0, 100.0, seed),
            },
        }
    }

    /// The parts that change every frame.
    pub fn update(&mut self, eye: Vec3, clock: f32, apparent_radius_pixels: f32) {
        self.uniform.eye = eye.extend(1.0);
        self.uniform.detail.y = clock;
        self.uniform.detail.z = apparent_radius_pixels;
        self.uniform.color.w = disc_brightness(apparent_radius_pixels);
    }
}

pub(crate) fn build(app: &mut App) {
    embedded_asset!(app, "star.wesl");
    app.add_plugins(MaterialPlugin::<StarMaterial>::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cool_stars_are_granulated_and_hot_stars_smooth() {
        // The Sun (5772 K) and an M dwarf are fully convective at the
        // surface; Sirius (9940 K) and Vega (9600 K) are not.
        assert_eq!(granulation(5772.0), 1.0);
        assert_eq!(granulation(3000.0), 1.0);
        assert_eq!(granulation(9940.0), 0.0);
        // Procyon (F5, 6530 K) still shows granulation; an F0 star less.
        assert!(granulation(6530.0) > 0.95);
        let f0 = granulation(7200.0);
        assert!(f0 > 0.2 && f0 < 0.8, "{f0}");
    }

    #[test]
    fn exposure_adapts_as_a_star_fills_the_view() {
        assert_eq!(disc_brightness(2.0), DISC_BRIGHTNESS);
        assert!((disc_brightness(400.0) - DISC_BRIGHTNESS_CLOSE).abs() < 1e-4);
        let mut previous = disc_brightness(1.0);
        for pixels in 2..400 {
            let now = disc_brightness(pixels as f32);
            assert!(now <= previous && previous / now < 1.2);
            previous = now;
        }
    }

    #[test]
    fn the_limb_is_darker_and_redder() {
        // At the limb (μ = 0) the linear law leaves 1 − u of the light.
        let limb: Vec<f32> = SOLAR_LIMB_DARKENING.iter().map(|u| 1.0 - u).collect();
        assert!(limb[0] > limb[1] && limb[1] > limb[2]);
        // The Sun's limb in green light is about 35–40% of its centre.
        assert!((0.3..0.45).contains(&limb[1]));
        let material = StarMaterial::new(LinearRgba::WHITE, 5772.0, 1.0);
        assert_eq!(
            material.uniform.limb.truncate().to_array(),
            SOLAR_LIMB_DARKENING
        );
        assert!(material.uniform.detail.x > 0.0);
        assert_eq!(
            StarMaterial::new(LinearRgba::WHITE, 9940.0, 1.0)
                .uniform
                .detail
                .x,
            0.0
        );
    }
}
