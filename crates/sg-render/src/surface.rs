//! How a body looks: from the snapshot to the numbers the planet shader
//! takes ([`crate::planet`]).
//!
//! Pure functions, no Bevy types. What comes from the model and what is a
//! drawing choice:
//!
//! - **From the model**: the class, the equilibrium temperature, the ocean,
//!   ice and cloud fractions (Classic tier only: the Standard tier has no
//!   climate yet, so its rocky planets are drawn bare), whether there is an
//!   atmosphere, tidal locking, the rotation period and obliquity where the
//!   tier gives them, and the texture seed.
//! - **Physically based**: thermal glow is the blackbody colour of the
//!   equilibrium temperature; giant-planet cloud colours follow the
//!   temperature classes of Sudarsky, Burrows & Pinto (2000, ApJ 538, 885);
//!   the number of bands follows the Rhines scale; sunsets redden by
//!   Rayleigh's λ⁻⁴.
//! - **Drawing choices**: every colour swatch, the shape of the terrain
//!   (noise, not geology), the craters on bare rock (their sizes, places
//!   and dark plains), the dry belts on land, how much the estimated
//!   minerals tint bare rock (the estimate is `sg_planet::minerals`), the height of the drawn atmosphere, and the wind
//!   speed and (where the tier gives none) the day length that set the band
//!   count. These are marked where they are made.

use sg_core::units::Kelvin;
use sg_schema::{Body, Mineral, PlanetClass};
use sg_stellar::blackbody_linear_srgb;

/// Which procedure the shader runs. The numbers are the shader's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
    /// Bare rock: airless worlds and embryos.
    Rock = 0,
    /// Land, sea and polar ice in the fractions the model gives.
    LandAndSea = 1,
    /// A frozen surface with cracks.
    Ice = 2,
    /// Dark crust over glowing rock.
    Lava = 3,
    /// Cloud bands of a giant.
    Bands = 4,
    /// A featureless deck of cloud or haze.
    Haze = 5,
}

/// The gas drawn around a body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Air {
    /// Linear RGB of the scattered light.
    pub color: [f32; 3],
    /// 0 (a trace) to 1 (as thick as can be drawn).
    pub density: f32,
    /// Height of the drawn shell as a fraction of the body's radius. Real
    /// atmospheres are a tenth of this or less; it is exaggerated so that
    /// it can be seen.
    pub height: f32,
}

/// Everything the planet shader needs for one body.
#[derive(Debug, Clone, PartialEq)]
pub struct Appearance {
    pub kind: SurfaceKind,
    /// Three linear-RGB surface or cloud colours; their use depends on the
    /// kind (low/high ground and accents; belts, zones and storms).
    pub colors: [[f32; 3]; 3],
    /// Linear RGB of open water.
    pub ocean_color: [f32; 3],
    pub ocean_frac: f32,
    pub ice_frac: f32,
    pub cloud_frac: f32,
    /// Bands from pole to pole (kind `Bands` and `Haze`).
    pub band_count: f32,
    /// 0 (uniform) to 1 (strongly banded).
    pub band_contrast: f32,
    /// 0 (none) to 1 (many) large storms.
    pub storms: f32,
    /// Linear RGB of the body's own thermal glow.
    pub glow_color: [f32; 3],
    /// 0 (dark) to 1 (white-hot) strength of that glow.
    pub glow: f32,
    pub air: Option<Air>,
    /// One face always toward the star.
    pub locked: bool,
    /// Tilt of the spin axis, degrees; 0 where the tier gives none.
    pub obliquity_deg: f32,
    /// The band count rests on an assumed day length, because the tier
    /// gives none.
    pub assumed_day: bool,
    /// Seed of the procedural detail, in `[0, 1000)`.
    pub seed: f32,
}

/// sRGB triple to linear RGB.
fn srgb(r: f32, g: f32, b: f32) -> [f32; 3] {
    [r, g, b].map(|c| {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    })
}

/// Rayleigh blue: what a clear nitrogen or oxygen sky scatters.
fn rayleigh_blue() -> [f32; 3] {
    srgb(0.36, 0.60, 1.0)
}

/// Colour factor of bare rock from its estimated surface: iron oxides
/// redden it, as on Mars; sulfides yellow it, as on Io; ices whiten it. How
/// much is a drawing choice.
fn mineral_tint(body: &Body) -> [f32; 3] {
    let mut tint = [1.0_f32; 3];
    for part in &body.minerals {
        let share = part.share as f32;
        let (color, strength): ([f32; 3], f32) = match part.mineral {
            Mineral::IronOxides => ([1.45, 0.72, 0.48], 3.0),
            Mineral::Sulfides => ([1.25, 1.1, 0.6], 3.0),
            Mineral::Carbonates => ([1.2, 1.2, 1.15], 2.0),
            Mineral::WaterIce => ([1.7, 1.8, 1.95], 1.0),
            Mineral::VolatileIces => ([1.5, 1.3, 1.1], 1.0),
            Mineral::Silicates | Mineral::MoltenRock => ([1.0, 1.0, 1.0], 0.0),
        };
        for (channel, target) in tint.iter_mut().zip(color) {
            *channel *= 1.0 + (target - 1.0) * (share * strength).min(1.0);
        }
    }
    tint
}

/// Equilibrium temperature at which rock begins to glow visibly, and at
/// which the glow is drawn at full strength. Incandescence becomes visible
/// near 800 K (the Draper point is 798 K).
const GLOW_START_K: f64 = 800.0;
const GLOW_FULL_K: f64 = 2400.0;

/// Strength, 0 to 1, of the thermal glow at an equilibrium temperature.
pub fn glow_strength(temp_k: f64) -> f32 {
    let x = ((temp_k - GLOW_START_K) / (GLOW_FULL_K - GLOW_START_K)).clamp(0.0, 1.0);
    (x * x * (3.0 - 2.0 * x)) as f32
}

/// Day length assumed for a giant when the tier gives none, hours: between
/// Jupiter's (9.9) and Saturn's (10.7).
pub const ASSUMED_GIANT_DAY_HOURS: f64 = 10.0;
/// Wind speed assumed for the band count, m/s: about Jupiter's jets.
const BAND_WIND_M_S: f64 = 40.0;

/// Number of cloud bands from pole to pole.
///
/// The Rhines scale `L = π·√(U/β)` with `β = 2Ω/R` at the equator sets the
/// width of a jet (Rhines 1975, J. Fluid Mech. 69, 417); the bands from
/// pole to pole are then `πR/L = √(2ΩR/U)`. With Jupiter's radius and day
/// and `U = 40 m/s` this gives about 25 alternating belts and zones, near
/// the planet's own count. `U` is an assumption, so this is an order of
/// magnitude, clamped to 3–30: slow rotators such as tidally locked hot
/// Jupiters get a few broad bands, which is also what circulation models of
/// them find.
pub fn band_count(radius_km: f64, day_hours: f64) -> f32 {
    let omega = std::f64::consts::TAU / (day_hours.max(1.0) * 3600.0);
    let bands = (2.0 * omega * radius_km * 1000.0 / BAND_WIND_M_S).sqrt();
    bands.clamp(3.0, 30.0) as f32
}

/// Cloud colours of a hydrogen-rich giant at an equilibrium temperature:
/// (zones, belts, storms), with band contrast.
///
/// The temperature classes are Sudarsky, Burrows & Pinto (2000): I ammonia
/// clouds (below about 150 K), II water clouds (to 250 K), III cloudless
/// (to 800 K), IV alkali metals (to 1400 K), V silicate clouds (above). The
/// swatches for each class are a drawing choice, blended across the
/// boundaries.
fn giant_clouds(temp_k: f64) -> ([[f32; 3]; 3], f32) {
    // (upper limit K, zones, belts, storms, contrast)
    let classes: [(f64, [[f32; 3]; 3], f32); 5] = [
        (
            150.0,
            [
                srgb(0.90, 0.84, 0.70),
                srgb(0.66, 0.46, 0.30),
                srgb(0.78, 0.36, 0.24),
            ],
            0.9,
        ),
        (
            250.0,
            [
                srgb(0.93, 0.94, 0.95),
                srgb(0.70, 0.78, 0.86),
                srgb(0.98, 0.98, 0.98),
            ],
            0.5,
        ),
        (
            800.0,
            [
                srgb(0.22, 0.38, 0.72),
                srgb(0.14, 0.26, 0.58),
                srgb(0.36, 0.52, 0.82),
            ],
            0.25,
        ),
        (
            1400.0,
            [
                srgb(0.30, 0.27, 0.27),
                srgb(0.17, 0.15, 0.17),
                srgb(0.42, 0.34, 0.30),
            ],
            0.45,
        ),
        (
            f64::INFINITY,
            [
                srgb(0.50, 0.50, 0.44),
                srgb(0.30, 0.28, 0.25),
                srgb(0.62, 0.56, 0.44),
            ],
            0.6,
        ),
    ];
    // Blend over 15% of the boundary temperature on either side.
    let mut i = classes.iter().position(|c| temp_k < c.0).unwrap_or(4);
    let mut t = 0.0_f32;
    if i > 0 {
        let lower = classes[i - 1].0;
        let x = (temp_k - lower) / (0.15 * lower);
        if x < 1.0 {
            // Just above the boundary: halfway to fully this class.
            t = (0.5 - 0.5 * x) as f32;
        }
    }
    if i < 4 {
        let upper = classes[i].0;
        let x = (upper - temp_k) / (0.15 * upper);
        if x < 1.0 {
            // Just below the boundary: blend toward the next class.
            i += 1;
            t = (0.5 + 0.5 * x) as f32;
        }
    }
    if t == 0.0 {
        return (classes[i].1, classes[i].2);
    }
    let (a, b) = (&classes[i], &classes[i - 1]);
    let mut colors = a.1;
    for (k, color) in colors.iter_mut().enumerate() {
        for (c, channel) in color.iter_mut().enumerate() {
            *channel += (b.1[k][c] - *channel) * t;
        }
    }
    (colors, a.2 + (b.2 - a.2) * t)
}

fn seed_of(texture_seed: u64) -> f32 {
    (texture_seed % 100_000) as f32 / 100.0
}

/// How a finished body is drawn.
#[allow(clippy::too_many_lines)]
pub fn appearance(body: &Body) -> Appearance {
    let temp = body.equilibrium_temp_k;
    let glow_color = blackbody_linear_srgb(Kelvin(temp)).map(|c| c as f32);
    let glow = glow_strength(temp);
    let locked = body.rotation.tidally_locked;
    let day_known = body.rotation.period_hours;
    let pressure_bar = body
        .atmosphere
        .as_ref()
        .and_then(|a| a.surface_pressure_mbar)
        .map(|mbar| mbar / 1000.0);
    // Density of the drawn air from surface pressure: a trace at 1 mbar,
    // full at 10 bar and beyond.
    let air_density = |fallback: f32| {
        pressure_bar.map_or(fallback, |bar| {
            ((bar.max(1e-3).log10() + 3.0) / 4.0).clamp(0.05, 1.0) as f32
        })
    };
    let mut look = Appearance {
        kind: SurfaceKind::Rock,
        colors: [
            srgb(0.42, 0.39, 0.36),
            srgb(0.60, 0.56, 0.51),
            srgb(0.27, 0.25, 0.24),
        ],
        ocean_color: srgb(0.03, 0.14, 0.36),
        ocean_frac: body.visual.ocean_frac as f32,
        ice_frac: body.visual.ice_frac as f32,
        cloud_frac: body.visual.cloud_frac as f32,
        band_count: 0.0,
        band_contrast: 0.0,
        storms: 0.0,
        glow_color,
        glow: 0.0,
        air: None,
        locked,
        obliquity_deg: body.rotation.obliquity_deg.unwrap_or(0.0) as f32,
        assumed_day: false,
        seed: seed_of(body.visual.texture_seed),
    };
    let giant = |look: &mut Appearance, contrast_scale: f32, air_height: f32| {
        let (colors, contrast) = giant_clouds(temp);
        look.kind = SurfaceKind::Bands;
        look.colors = colors;
        look.band_contrast = contrast * contrast_scale;
        look.band_count = band_count(body.radius_km, day_known.unwrap_or(ASSUMED_GIANT_DAY_HOURS));
        look.assumed_day = day_known.is_none();
        look.storms = 0.5 * contrast_scale;
        // The night side of a hot giant glows with its own heat.
        look.glow = glow;
        look.ocean_frac = 0.0;
        look.ice_frac = 0.0;
        look.cloud_frac = 0.0;
        look.air = Some(Air {
            color: [
                0.5 * (colors[0][0] + 1.0),
                0.5 * (colors[0][1] + 1.0),
                0.5 * (colors[0][2] + 1.0),
            ],
            density: 0.8,
            height: air_height,
        });
    };
    match body.class {
        PlanetClass::Asteroids | PlanetClass::Rock | PlanetClass::Rocky | PlanetClass::Unknown => {
            look.ocean_frac = 0.0;
            look.cloud_frac = 0.0;
            let tint = mineral_tint(body);
            for color in &mut look.colors {
                for (channel, factor) in color.iter_mut().zip(tint) {
                    *channel *= factor;
                }
            }
        }
        PlanetClass::SuperEarth => {
            look.colors = [
                srgb(0.47, 0.40, 0.33),
                srgb(0.64, 0.56, 0.46),
                srgb(0.30, 0.26, 0.23),
            ];
            look.ocean_frac = 0.0;
            look.cloud_frac = 0.0;
        }
        PlanetClass::LavaWorld => {
            look.kind = SurfaceKind::Lava;
            look.colors = [
                srgb(0.10, 0.09, 0.09),
                srgb(0.24, 0.20, 0.18),
                srgb(0.05, 0.05, 0.05),
            ];
            // Molten rock is at its melting point, not at the planet's
            // equilibrium temperature: draw it at 1500 K at least.
            look.glow_color = blackbody_linear_srgb(Kelvin(temp.max(1500.0))).map(|c| c as f32);
            look.glow = glow.max(0.6);
            look.ocean_frac = 0.0;
            look.cloud_frac = 0.0;
        }
        PlanetClass::Martian => {
            look.kind = SurfaceKind::LandAndSea;
            look.colors = [
                srgb(0.66, 0.36, 0.22),
                srgb(0.80, 0.52, 0.34),
                srgb(0.42, 0.24, 0.17),
            ];
            look.air = Some(Air {
                color: srgb(0.86, 0.66, 0.48),
                density: air_density(0.1),
                height: 0.02,
            });
        }
        PlanetClass::Terrestrial | PlanetClass::OneFace => {
            look.kind = SurfaceKind::LandAndSea;
            look.colors = [
                srgb(0.26, 0.40, 0.18),
                srgb(0.58, 0.50, 0.36),
                srgb(0.42, 0.36, 0.30),
            ];
            look.air = Some(Air {
                color: rayleigh_blue(),
                density: air_density(0.5),
                height: 0.025,
            });
        }
        PlanetClass::Water | PlanetClass::WaterWorld => {
            look.kind = SurfaceKind::LandAndSea;
            look.colors = [
                srgb(0.30, 0.38, 0.24),
                srgb(0.52, 0.48, 0.38),
                srgb(0.40, 0.36, 0.30),
            ];
            if body.class == PlanetClass::WaterWorld {
                // A tenth or more of the planet is water: an ocean hundreds
                // of kilometres deep, with no land. Whether it has air or
                // cloud this tier does not say, so none is drawn.
                look.ocean_frac = 1.0;
            }
            if body.atmosphere.is_some() {
                look.air = Some(Air {
                    color: rayleigh_blue(),
                    density: air_density(0.5),
                    height: 0.025,
                });
            }
        }
        PlanetClass::Ice | PlanetClass::IceWorld => {
            look.kind = SurfaceKind::Ice;
            look.colors = [
                srgb(0.86, 0.92, 0.96),
                srgb(0.62, 0.74, 0.84),
                srgb(0.50, 0.44, 0.40),
            ];
            look.ocean_frac = 0.0;
            if body.atmosphere.is_some() && body.class == PlanetClass::Ice {
                look.air = Some(Air {
                    color: rayleigh_blue(),
                    density: air_density(0.2),
                    height: 0.02,
                });
            }
        }
        PlanetClass::Venusian => {
            // The surface is never seen: a deck of sulphur-yellow cloud.
            look.kind = SurfaceKind::Haze;
            look.colors = [
                srgb(0.90, 0.80, 0.52),
                srgb(0.80, 0.66, 0.40),
                srgb(0.96, 0.90, 0.70),
            ];
            look.band_count = 6.0;
            look.band_contrast = 0.25;
            look.air = Some(Air {
                color: srgb(0.95, 0.86, 0.62),
                density: air_density(1.0),
                height: 0.03,
            });
        }
        PlanetClass::GasDwarf | PlanetClass::SubNeptune => {
            // A hydrogen envelope seen through haze: the colours of its
            // temperature class, washed out.
            giant(&mut look, 0.3, 0.035);
            look.kind = SurfaceKind::Haze;
            look.storms = 0.0;
        }
        PlanetClass::SubJovian | PlanetClass::IceGiant => {
            giant(&mut look, 0.35, 0.04);
            if temp < 150.0 {
                // Cold enough for methane to colour the gas, as on Uranus
                // and Neptune.
                look.colors = [
                    srgb(0.56, 0.80, 0.86),
                    srgb(0.30, 0.52, 0.82),
                    srgb(0.86, 0.94, 0.96),
                ];
                look.air = Some(Air {
                    color: srgb(0.62, 0.84, 0.94),
                    density: 0.8,
                    height: 0.04,
                });
            }
        }
        PlanetClass::Jovian | PlanetClass::GasGiant | PlanetClass::HotJupiter => {
            giant(&mut look, 1.0, 0.04);
        }
    }
    look
}

/// How a body still forming is drawn: bare rock, glowing while it is being
/// struck. The model gives embryos no temperature, so the glow is a drawing
/// choice that marks them as unfinished.
pub fn embryo_appearance(id: u32) -> Appearance {
    Appearance {
        kind: SurfaceKind::Lava,
        colors: [
            srgb(0.30, 0.24, 0.20),
            srgb(0.46, 0.38, 0.30),
            srgb(0.16, 0.13, 0.12),
        ],
        ocean_color: [0.0; 3],
        ocean_frac: 0.0,
        ice_frac: 0.0,
        cloud_frac: 0.0,
        band_count: 0.0,
        band_contrast: 0.0,
        storms: 0.0,
        glow_color: blackbody_linear_srgb(Kelvin(1500.0)).map(|c| c as f32),
        glow: 0.35,
        air: None,
        locked: false,
        obliquity_deg: 0.0,
        assumed_day: false,
        seed: seed_of(sg_core::hash::fnv1a64(&id.to_le_bytes())),
    }
}

/// Latitude, as a sine, above which polar ice is drawn so that the two caps
/// cover `ice_frac` of the sphere: a cap from latitude φ to the pole has
/// area `2πR²(1 − sin φ)`, so `sin φ = 1 − ice_frac`.
pub fn ice_cap_sin_latitude(ice_frac: f32) -> f32 {
    1.0 - ice_frac.clamp(0.0, 1.0)
}

/// Rayleigh optical depth of red and green light relative to blue, for
/// 680, 550 and 440 nm: `(λ_blue/λ)⁴`.
pub const RAYLEIGH_RELATIVE_DEPTH: [f32; 3] = [0.175, 0.410, 1.0];

#[cfg(test)]
mod tests {
    use super::*;
    use sg_schema::{SimulationParams, SystemSnapshot};

    fn classic(seed: u64) -> SystemSnapshot {
        sg_sim::simulate(&SimulationParams {
            seed,
            ..SimulationParams::default()
        })
        .expect("classic run")
        .snapshot
    }

    #[test]
    fn jupiter_and_a_locked_giant_get_plausible_band_counts() {
        // Jupiter: 71 492 km, 9.93 h. It shows about 20 belts and zones.
        let jupiter = band_count(71_492.0, 9.93);
        assert!((18.0..30.0).contains(&jupiter), "{jupiter}");
        // √(2ΩR/U) by hand: Ω = 1.758e-4 s⁻¹ → √(2·1.758e-4·7.1492e7/40).
        assert!((jupiter - 25.07).abs() < 0.05, "{jupiter}");
        // A hot Jupiter locked to a 3-day orbit: a few broad bands.
        let hot = band_count(90_000.0, 72.0);
        assert!((3.0..12.0).contains(&hot), "{hot}");
        assert!(band_count(1000.0, 1.0e6) >= 3.0);
    }

    #[test]
    fn rayleigh_ratios_follow_the_fourth_power() {
        for (value, wavelength) in RAYLEIGH_RELATIVE_DEPTH
            .iter()
            .zip([680.0_f32, 550.0, 440.0])
        {
            assert!((value - (440.0 / wavelength).powi(4)).abs() < 2e-3);
        }
    }

    #[test]
    fn ice_caps_cover_the_fraction_asked_for() {
        // Two caps above latitude φ cover 1 − sin φ of the sphere.
        assert_eq!(ice_cap_sin_latitude(0.0), 1.0);
        assert_eq!(ice_cap_sin_latitude(1.0), 0.0);
        // Half the sphere lies poleward of 30°.
        assert!((ice_cap_sin_latitude(0.5) - 30.0_f32.to_radians().sin()).abs() < 1e-6);
    }

    #[test]
    fn glow_starts_at_incandescence() {
        assert_eq!(glow_strength(300.0), 0.0);
        assert_eq!(glow_strength(800.0), 0.0);
        assert!(glow_strength(1600.0) > 0.4 && glow_strength(1600.0) < 0.6);
        assert_eq!(glow_strength(3000.0), 1.0);
    }

    #[test]
    fn giant_colours_follow_temperature_and_blend_smoothly() {
        // Cold: cream and brown (red above blue). Warm and cloudless: blue.
        let (cold, _) = giant_clouds(110.0);
        assert!(cold[0][0] > cold[0][2]);
        let (clear, _) = giant_clouds(500.0);
        assert!(clear[0][2] > clear[0][0]);
        // No jump anywhere from 50 K to 2500 K.
        let mut previous = giant_clouds(50.0).0;
        let mut t = 50.0;
        while t < 2500.0 {
            t += 1.0;
            let now = giant_clouds(t).0;
            for k in 0..3 {
                for c in 0..3 {
                    assert!((now[k][c] - previous[k][c]).abs() < 0.03, "jump at {t} K");
                }
            }
            previous = now;
        }
    }

    #[test]
    fn appearance_takes_its_fractions_from_the_model() {
        let mut seen_giant = false;
        let mut seen_rock = false;
        for seed in [1, 7, 42, 736] {
            for body in &classic(seed).bodies {
                let look = appearance(body);
                for color in look.colors {
                    assert!(color.iter().all(|c| (0.0..=1.0).contains(c)));
                }
                assert!((0.0..1000.0).contains(&look.seed));
                assert!((0.0..=1.0).contains(&look.glow));
                match look.kind {
                    SurfaceKind::Bands => {
                        seen_giant = true;
                        assert!(look.band_count >= 3.0 && look.air.is_some());
                        assert_eq!(look.ocean_frac, 0.0);
                        // The Classic tier gives a day length.
                        assert!(!look.assumed_day);
                    }
                    SurfaceKind::LandAndSea => {
                        assert_eq!(look.ocean_frac, body.visual.ocean_frac as f32);
                        assert_eq!(look.ice_frac, body.visual.ice_frac as f32);
                    }
                    SurfaceKind::Rock => {
                        seen_rock = true;
                        assert!(look.air.is_none());
                    }
                    _ => {}
                }
            }
        }
        assert!(seen_giant && seen_rock);
    }

    #[test]
    fn standard_tier_rock_is_drawn_bare_and_water_worlds_all_ocean() {
        let params = SimulationParams {
            seed: 3,
            fidelity: sg_schema::Fidelity::Standard,
            ..SimulationParams::default()
        };
        let snapshot = sg_sim::simulate(&params).expect("standard run").snapshot;
        assert!(!snapshot.bodies.is_empty());
        for body in &snapshot.bodies {
            let look = appearance(body);
            match body.class {
                PlanetClass::Rocky | PlanetClass::SuperEarth => {
                    assert!(look.air.is_none() && look.cloud_frac == 0.0);
                }
                PlanetClass::WaterWorld => assert_eq!(look.ocean_frac, 1.0),
                PlanetClass::GasGiant | PlanetClass::HotJupiter | PlanetClass::IceGiant => {
                    // No day length from this tier unless tidally locked.
                    assert_eq!(look.assumed_day, body.rotation.period_hours.is_none());
                }
                _ => {}
            }
        }
    }

    #[test]
    fn embryos_differ_by_id_and_glow() {
        let (a, b) = (embryo_appearance(1), embryo_appearance(2));
        assert_ne!(a.seed, b.seed);
        assert!(a.glow > 0.0 && a.air.is_none());
    }
}
