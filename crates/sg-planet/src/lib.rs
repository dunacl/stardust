//! What a formed planet is like at the system's age: its size, how much of
//! its atmosphere it has kept, whether it is tidally locked, and what kind
//! of planet that makes it (PLAN.md §5.8–5.9).
//!
//! Masses are in Earth masses, radii in Earth radii, distances in AU.
//!
//! Moons and rings are in [`moons`].
//!
//! Not modelled yet: surface climate and atmospheric composition, the
//! inflated radii of strongly irradiated giants, core-powered mass loss,
//! and interior differentiation beyond a fixed rock-to-iron ratio.

pub mod minerals;
pub mod moons;

use sg_core::math::{exp, ln, log10, pow, sqrt};

/// Rock share of the rock-plus-iron in an Earth-like interior.
pub const EARTH_ROCK_FRACTION: f64 = 0.67;

/// Radius of a planet of rock and iron.
///
/// Fortney, Marley & Barnes (2007, ApJ 659, 1661), eq. 8, with `rmf` the
/// rock mass fraction (1 = pure rock, 0 = pure iron):
/// `R = (0.0592 rmf + 0.0975)(log M)² + (0.2337 rmf + 0.4938) log M +
/// (0.3102 rmf + 0.7932)`. A fit to interior models, good to a few percent
/// for 0.01–1000 M⊕. Gives 1.00 R⊕ for 1 M⊕ at `rmf = 0.67`.
pub fn rock_iron_radius(mass_mearth: f64, rock_mass_fraction: f64) -> f64 {
    let rmf = rock_mass_fraction.clamp(0.0, 1.0);
    let l = log10(mass_mearth);
    (0.0592 * rmf + 0.0975) * l * l + (0.2337 * rmf + 0.4938) * l + (0.3102 * rmf + 0.7932)
}

/// Radius of a planet of ice and rock.
///
/// Fortney, Marley & Barnes (2007), eq. 7, with `imf` the ice mass fraction
/// (1 = pure ice, 0 = pure rock):
/// `R = (0.0912 imf + 0.1603)(log M)² + (0.3330 imf + 0.7387) log M +
/// (0.4639 imf + 1.1193)`. Same range and accuracy as [`rock_iron_radius`].
pub fn ice_rock_radius(mass_mearth: f64, ice_mass_fraction: f64) -> f64 {
    let imf = ice_mass_fraction.clamp(0.0, 1.0);
    let l = log10(mass_mearth);
    (0.0912 * imf + 0.1603) * l * l + (0.3330 * imf + 0.7387) * l + (0.4639 * imf + 1.1193)
}

/// Radius of a planet's solid part: an Earth-like rock and iron interior,
/// swollen by its water fraction.
///
/// The two Fortney et al. (2007) fits are combined: the rock–iron radius at
/// Earth's rock fraction, times the ratio by which the ice–rock fit grows
/// with ice fraction. Exact for a dry planet; an interpolation otherwise.
/// Below the fits' lower limit of 0.01 M⊕, where self-compression no longer
/// matters, the radius follows constant density, `R ∝ M^(1/3)`.
pub fn solid_radius(mass_mearth: f64, water_mass_fraction: f64) -> f64 {
    const FIT_FLOOR_MEARTH: f64 = 0.01;
    let mass = mass_mearth.max(FIT_FLOOR_MEARTH);
    let fitted = rock_iron_radius(mass, EARTH_ROCK_FRACTION)
        * ice_rock_radius(mass, water_mass_fraction)
        / ice_rock_radius(mass, 0.0);
    fitted * sg_core::math::cbrt(mass_mearth.min(FIT_FLOOR_MEARTH) / FIT_FLOOR_MEARTH).max(0.0)
}

/// Thickness of a hydrogen–helium envelope, R⊕.
///
/// Lopez & Fortney (2014, ApJ 792, 1), eq. 3 (their enhanced-opacity fit):
/// `R_env = 2.06 R⊕ (M/M⊕)^-0.21 (f_env/5%)^0.59 (F/F⊕)^0.044
/// (age/5 Gyr)^-0.18`. A summary of thermal-evolution models for
/// 1–20 M⊕, envelope fractions of 0.01–60%, 0.1–1000 times Earth's
/// insolation and ages above 100 Myr; good to about 0.1 dex. Inputs are
/// clamped into those ranges.
pub fn envelope_thickness(
    mass_mearth: f64,
    envelope_fraction: f64,
    flux_earth: f64,
    age_gyr: f64,
) -> f64 {
    if envelope_fraction <= 0.0 {
        return 0.0;
    }
    2.06 * pow(mass_mearth.max(0.5), -0.21)
        * pow(envelope_fraction.clamp(1.0e-4, 0.6) / 0.05, 0.59)
        * pow(flux_earth.clamp(0.1, 1000.0), 0.044)
        * pow(age_gyr.clamp(0.1, 10.0) / 5.0, -0.18)
}

/// Radius of a cold planet that is mostly hydrogen and helium, R⊕.
///
/// Interpolated in log mass through Saturn (95.2 M⊕, 9.14 R⊕) and Jupiter
/// (317.8 M⊕, 11.21 R⊕), and flat above Jupiter, where added mass barely
/// changes the radius (Fortney et al. 2007, table 4: 1.0–1.1 Jupiter radii
/// from 1 to 10 Jupiter masses at 4.5 Gyr). Approximate to about 15%;
/// strongly irradiated giants are larger than this.
pub fn giant_radius(mass_mearth: f64) -> f64 {
    const SATURN: (f64, f64) = (95.2, 9.14);
    const JUPITER: (f64, f64) = (317.8, 11.21);
    if mass_mearth >= JUPITER.0 {
        return JUPITER.1;
    }
    let slope = log10(JUPITER.1 / SATURN.1) / log10(JUPITER.0 / SATURN.0);
    SATURN.1 * pow(mass_mearth / SATURN.0, slope)
}

/// What a planet is made of.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Makeup {
    /// Rock, iron and water, M⊕.
    pub core_mearth: f64,
    /// Hydrogen and helium, M⊕.
    pub envelope_mearth: f64,
    /// Water as a fraction of the core.
    pub water_fraction: f64,
}

impl Makeup {
    pub fn mass_mearth(&self) -> f64 {
        self.core_mearth + self.envelope_mearth
    }

    /// Hydrogen–helium share of the whole planet.
    pub fn envelope_fraction(&self) -> f64 {
        self.envelope_mearth / self.mass_mearth()
    }
}

/// Envelope fractions between which a planet's radius passes from the
/// sub-Neptune relation to the giant one.
const GIANT_BLEND: (f64, f64) = (0.4, 0.7);

/// Radius of a planet, R⊕: its solid part plus its envelope
/// ([`solid_radius`], [`envelope_thickness`]), passing to [`giant_radius`]
/// as hydrogen and helium come to dominate.
pub fn radius(makeup: &Makeup, flux_earth: f64, age_gyr: f64) -> f64 {
    let f = makeup.envelope_fraction();
    let layered = solid_radius(makeup.core_mearth, makeup.water_fraction)
        + envelope_thickness(makeup.mass_mearth(), f, flux_earth, age_gyr);
    let t = ((f - GIANT_BLEND.0) / (GIANT_BLEND.1 - GIANT_BLEND.0)).clamp(0.0, 1.0);
    let weight = t * t * (3.0 - 2.0 * t);
    (1.0 - weight) * layered + weight * giant_radius(makeup.mass_mearth())
}

/// Stellar flux at a planet in units of Earth's: `L/a²`.
pub fn flux_earth(luminosity_lsun: f64, a_au: f64) -> f64 {
    luminosity_lsun / (a_au * a_au)
}

/// Equilibrium temperature of a planet that reradiates from its whole
/// surface, K: `278.3 K · (L/L☉)^(1/4) (a/AU)^(-1/2) (1 − A)^(1/4)`, the
/// black-body balance for the solar constant at 1 AU (1361 W/m²).
pub fn equilibrium_temperature(luminosity_lsun: f64, a_au: f64, bond_albedo: f64) -> f64 {
    278.3 * sqrt(sqrt(luminosity_lsun * (1.0 - bond_albedo))) / sqrt(a_au)
}

/// High-energy (X-ray and extreme ultraviolet) luminosity of a star, L☉.
///
/// Owen & Wu (2017, ApJ 847, 29), eq. 15: a saturated `10^-3.5 L☉ (M/M☉)`
/// for the first 100 Myr, then falling as `t^-1.5`. Follows the observed
/// decay of stellar X-ray emission with age (Jackson, Davis & Wheatley
/// 2012).
pub fn xuv_luminosity(star_mass_msun: f64, age_gyr: f64) -> f64 {
    const SATURATION_GYR: f64 = 0.1;
    let saturated = pow(10.0, -3.5) * star_mass_msun;
    if age_gyr <= SATURATION_GYR {
        saturated
    } else {
        saturated * pow(age_gyr / SATURATION_GYR, -1.5)
    }
}

/// Fraction of the absorbed high-energy flux that goes into lifting gas out
/// of the planet's gravity (Lopez, Fortney & Miller 2012; Owen & Wu 2017
/// find about 0.1 for planets of a few Earth masses).
const ESCAPE_EFFICIENCY: f64 = 0.1;
/// Smallest envelope kept, as a fraction of the planet; below it the planet
/// is counted as stripped. The lower end of the radius fit's range.
const MIN_ENVELOPE_FRACTION: f64 = 1.0e-4;
/// Escape is followed from this age, when the disk is gone, Gyr.
const ESCAPE_START_GYR: f64 = 0.01;
const ESCAPE_STEPS: u32 = 80;

/// `π · L☉ · R⊕³ / (4π AU² · G · M⊕²)` in Gyr⁻¹: turns
/// `ε · L_XUV[L☉] · R³[R⊕] / (a²[AU] · M[M⊕])` into a mass-loss rate in
/// M⊕/Gyr. From L☉ = 3.828×10²⁶ W, R⊕ = 6.378×10⁶ m, AU = 1.496×10¹¹ m,
/// G = 6.674×10⁻¹¹, M⊕ = 5.972×10²⁴ kg and 1 Gyr = 3.156×10¹⁶ s.
const ESCAPE_RATE_CONSTANT: f64 = 3.828e26 * 6.378e6 * 6.378e6 * 6.378e6
    / (4.0 * 1.496e11 * 1.496e11 * 6.674e-11 * 5.972e24 * 5.972e24)
    * 3.156e16;

/// The envelope a planet has left at `age_gyr`, M⊕, after its star's
/// high-energy light has driven gas off it.
///
/// Energy-limited escape (Watson, Donahue & Walker 1981; in the form of
/// Lopez, Fortney & Miller 2012, ApJ 761, 59, eq. 1):
/// `Ṁ = ε π F_XUV R³ / (G M)`, integrated from the disk's dispersal with the
/// planet's radius ([`radius`]) shrinking as it loses gas and cools. This
/// is the process that separates stripped super-Earths from sub-Neptunes
/// (Owen & Wu 2017). Tidal enhancement of the loss and the short "boil-off"
/// at dispersal are not included.
pub fn envelope_after_escape(
    makeup: &Makeup,
    a_au: f64,
    star_mass_msun: f64,
    star_luminosity_lsun: f64,
    age_gyr: f64,
) -> f64 {
    let mut envelope = makeup.envelope_mearth;
    if envelope <= 0.0 || age_gyr <= ESCAPE_START_GYR {
        return envelope.max(0.0);
    }
    let flux = flux_earth(star_luminosity_lsun, a_au);
    let log_span = ln(age_gyr / ESCAPE_START_GYR);
    let mut t = ESCAPE_START_GYR;
    for step in 1..=ESCAPE_STEPS {
        let next = ESCAPE_START_GYR * exp(log_span * f64::from(step) / f64::from(ESCAPE_STEPS));
        let mid = sqrt(t * next);
        let now = Makeup {
            envelope_mearth: envelope,
            ..*makeup
        };
        let r = radius(&now, flux, mid);
        let rate = ESCAPE_EFFICIENCY
            * ESCAPE_RATE_CONSTANT
            * xuv_luminosity(star_mass_msun, mid)
            * r
            * r
            * r
            / (a_au * a_au * now.mass_mearth());
        envelope -= rate * (next - t);
        if envelope < MIN_ENVELOPE_FRACTION * makeup.core_mearth {
            return 0.0;
        }
        t = next;
    }
    envelope
}

/// Time for tides raised by the star to slow a planet's spin until one face
/// always points at it, years.
///
/// Gladman et al. (1996, Icarus 122, 166), eq. 1:
/// `t = ω a⁶ I Q / (3 G M★² k₂ R⁵)` with moment of inertia `I = 0.4 M R²`.
/// The planet's initial spin and its tidal response are not known for a
/// generated planet; a 12 hour initial day and `Q/k₂ = 100/0.3` (values for
/// rocky bodies) are assumed, so this is an order-of-magnitude estimate.
pub fn tidal_locking_time_yr(
    a_au: f64,
    star_mass_msun: f64,
    mass_mearth: f64,
    radius_rearth: f64,
) -> f64 {
    const INITIAL_DAY_S: f64 = 12.0 * 3600.0;
    const Q_OVER_K2: f64 = 100.0 / 0.3;
    const G_SI: f64 = 6.674e-11;
    let omega = 2.0 * core::f64::consts::PI / INITIAL_DAY_S;
    let a = a_au * 1.496e11;
    let star = star_mass_msun * 1.989e30;
    let mass = mass_mearth * 5.972e24;
    let r = radius_rearth * 6.378e6;
    let inertia = 0.4 * mass * r * r;
    let seconds =
        omega * pow(a, 6.0) * inertia * Q_OVER_K2 / (3.0 * G_SI * star * star * pow(r, 5.0));
    seconds / 3.156e7
}

/// Kinds of planet, by composition and temperature (PLAN.md §5.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlanetKind {
    /// Bare rock, smaller than about 2 M⊕.
    Rocky,
    /// Bare rock of 2 M⊕ or more.
    SuperEarth,
    /// Bare rock hot enough for its dayside to melt.
    LavaWorld,
    /// A tenth or more water, warm enough that it is not all frozen.
    WaterWorld,
    /// A tenth or more water, frozen.
    IceWorld,
    /// A rocky or icy core under a hydrogen–helium envelope of under 10%.
    SubNeptune,
    /// Envelope of 10–50% of the planet: like Uranus and Neptune.
    IceGiant,
    /// Mostly hydrogen and helium.
    GasGiant,
    /// A gas giant hotter than 1000 K.
    HotJupiter,
}

/// Equilibrium temperature above which bare rock melts on the dayside
/// (silicate solidus, about 1300 K at low pressure).
const LAVA_TEMPERATURE_K: f64 = 1300.0;
/// Below this water is frozen (the temperature of the disk's ice line).
const FROZEN_TEMPERATURE_K: f64 = 170.0;

/// Classifies a planet from what it is made of (after escape) and its
/// equilibrium temperature.
pub fn classify(makeup: &Makeup, equilibrium_temp_k: f64) -> PlanetKind {
    let f = makeup.envelope_fraction();
    if f >= 0.5 {
        if equilibrium_temp_k >= 1000.0 {
            PlanetKind::HotJupiter
        } else {
            PlanetKind::GasGiant
        }
    } else if f >= 0.1 {
        PlanetKind::IceGiant
    } else if f > 0.0 {
        PlanetKind::SubNeptune
    } else if equilibrium_temp_k >= LAVA_TEMPERATURE_K {
        PlanetKind::LavaWorld
    } else if makeup.water_fraction >= 0.1 {
        if equilibrium_temp_k < FROZEN_TEMPERATURE_K {
            PlanetKind::IceWorld
        } else {
            PlanetKind::WaterWorld
        }
    } else if makeup.core_mearth >= 2.0 {
        PlanetKind::SuperEarth
    } else {
        PlanetKind::Rocky
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn bare(core: f64, water: f64) -> Makeup {
        Makeup {
            core_mearth: core,
            envelope_mearth: 0.0,
            water_fraction: water,
        }
    }

    fn with_envelope(core: f64, fraction: f64) -> Makeup {
        Makeup {
            core_mearth: core,
            envelope_mearth: core * fraction / (1.0 - fraction),
            water_fraction: 0.0,
        }
    }

    /// Fortney et al. (2007): Earth is 1.00 R⊕, pure rock 1.10, and a
    /// half-ice planet of 1 M⊕ about 1.35.
    #[test]
    fn solid_radii_match_fortney() {
        assert!((rock_iron_radius(1.0, 0.67) - 1.0008).abs() < 1e-3);
        assert!((rock_iron_radius(1.0, 1.0) - 1.1034).abs() < 1e-9);
        assert!((ice_rock_radius(1.0, 0.5) - 1.35125).abs() < 1e-9);
        assert!((solid_radius(1.0, 0.0) - 1.0008).abs() < 1e-3);
        // Mars (0.107 M⊕, 0.532 R⊕; the fit gives 0.50 for Earth's iron
        // share, and Mars has less iron) and a 5 M⊕ super-Earth (about
        // 1.5 R⊕).
        assert!((solid_radius(0.107, 0.0) - 0.532).abs() < 0.05);
        assert!((solid_radius(5.0, 0.0) - 1.53).abs() < 0.06);
        // The Moon (0.0123 M⊕, 0.273 R⊕), and constant density below the
        // fit: a thousandth of the mass, a tenth of the radius.
        assert!((solid_radius(0.0123, 0.0) - 0.273).abs() < 0.02);
        assert!((solid_radius(1.0e-5, 0.0) / solid_radius(0.01, 0.0) - 0.1).abs() < 1e-9);
        // Water makes a planet larger at the same mass.
        assert!(solid_radius(1.0, 0.5) > 1.15 * solid_radius(1.0, 0.0));
    }

    /// Lopez & Fortney (2014): the normalisation, and Neptune (17.1 M⊕,
    /// 3.88 R⊕) with roughly 15% hydrogen and helium.
    #[test]
    fn envelopes_match_lopez_and_fortney() {
        assert!((envelope_thickness(1.0, 0.05, 1.0, 5.0) - 2.06).abs() < 1e-12);
        let neptune = with_envelope(17.1 * 0.85, 0.15);
        let r = radius(&neptune, 1.0 / 900.0, 4.5);
        assert!((r - 3.88).abs() < 0.5, "{r}");
        assert_eq!(envelope_thickness(5.0, 0.0, 10.0, 5.0), 0.0);
        // More gas, more light and less age all mean a larger planet.
        assert!(envelope_thickness(5.0, 0.1, 10.0, 5.0) > envelope_thickness(5.0, 0.01, 10.0, 5.0));
        assert!(
            envelope_thickness(5.0, 0.05, 100.0, 5.0) > envelope_thickness(5.0, 0.05, 1.0, 5.0)
        );
        assert!(
            envelope_thickness(5.0, 0.05, 10.0, 1.0) > envelope_thickness(5.0, 0.05, 10.0, 8.0)
        );
    }

    #[test]
    fn giants_have_saturn_and_jupiter_radii() {
        assert!((giant_radius(95.2) - 9.14).abs() < 1e-9);
        assert!((giant_radius(317.8) - 11.21).abs() < 1e-9);
        assert_eq!(giant_radius(3000.0), 11.21);
        let jupiter = Makeup {
            core_mearth: 15.0,
            envelope_mearth: 302.8,
            water_fraction: 0.5,
        };
        assert!((radius(&jupiter, 0.037, 4.5) - 11.21).abs() < 1e-9);
    }

    /// Earth is at 255 K with its albedo of 0.3; a black body at 1 AU, 278 K.
    #[test]
    fn equilibrium_temperature_of_earth() {
        assert!((equilibrium_temperature(1.0, 1.0, 0.3) - 254.6).abs() < 0.5);
        assert!((equilibrium_temperature(1.0, 1.0, 0.0) - 278.3).abs() < 1e-9);
        assert!((equilibrium_temperature(16.0, 4.0, 0.0) - 278.3).abs() < 1e-9);
        assert!((flux_earth(4.0, 2.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn xuv_is_saturated_then_decays() {
        assert_eq!(xuv_luminosity(1.0, 0.05), xuv_luminosity(1.0, 0.1));
        assert!((xuv_luminosity(1.0, 0.1) - 10f64.powf(-3.5)).abs() < 1e-15);
        // A hundred times older: a thousand times fainter.
        assert!((xuv_luminosity(1.0, 10.0) / xuv_luminosity(1.0, 0.1) - 1.0e-3).abs() < 1e-12);
    }

    /// The radius valley's cause: close to its star a small core loses a
    /// thin envelope entirely, while the same planet farther out, or a
    /// heavier core, keeps it (Owen & Wu 2017).
    #[test]
    fn escape_strips_small_close_in_planets() {
        let small = with_envelope(3.0, 0.02);
        let close = envelope_after_escape(&small, 0.05, 1.0, 1.0, 5.0);
        let far = envelope_after_escape(&small, 1.0, 1.0, 1.0, 5.0);
        assert_eq!(close, 0.0);
        assert!(far > 0.9 * small.envelope_mearth, "{far}");
        let heavy = with_envelope(12.0, 0.05);
        let kept = envelope_after_escape(&heavy, 0.1, 1.0, 1.0, 5.0);
        assert!(kept > 0.5 * heavy.envelope_mearth, "{kept}");
        // A gas giant is untouched even very close in.
        let giant = with_envelope(20.0, 0.94);
        let kept = envelope_after_escape(&giant, 0.05, 1.0, 1.0, 5.0);
        assert!(kept > 0.95 * giant.envelope_mearth);
        // Nothing to lose, nothing lost.
        assert_eq!(
            envelope_after_escape(&bare(3.0, 0.0), 0.05, 1.0, 1.0, 5.0),
            0.0
        );
    }

    /// Earth is far from locked after 4.6 Gyr; a planet in the habitable
    /// zone of a red dwarf locks within a few million years.
    #[test]
    fn tidal_locking_times_have_the_right_scale() {
        let earth = tidal_locking_time_yr(1.0, 1.0, 1.0, 1.0);
        assert!(earth > 1.0e10, "{earth}");
        let red_dwarf_hz = tidal_locking_time_yr(0.05, 0.15, 1.0, 1.0);
        assert!(red_dwarf_hz < 1.0e7, "{red_dwarf_hz}");
        // a⁶: twice as far, 64 times longer.
        let ratio = tidal_locking_time_yr(2.0, 1.0, 1.0, 1.0) / earth;
        assert!((ratio - 64.0).abs() < 1e-9);
    }

    #[test]
    fn planets_are_classified_by_makeup_and_temperature() {
        assert_eq!(classify(&bare(1.0, 0.0), 255.0), PlanetKind::Rocky);
        assert_eq!(classify(&bare(5.0, 0.0), 400.0), PlanetKind::SuperEarth);
        assert_eq!(classify(&bare(5.0, 0.0), 1800.0), PlanetKind::LavaWorld);
        assert_eq!(classify(&bare(2.0, 0.4), 260.0), PlanetKind::WaterWorld);
        assert_eq!(classify(&bare(2.0, 0.4), 80.0), PlanetKind::IceWorld);
        assert_eq!(
            classify(&with_envelope(6.0, 0.03), 600.0),
            PlanetKind::SubNeptune
        );
        assert_eq!(
            classify(&with_envelope(14.0, 0.15), 60.0),
            PlanetKind::IceGiant
        );
        assert_eq!(
            classify(&with_envelope(15.0, 0.95), 120.0),
            PlanetKind::GasGiant
        );
        assert_eq!(
            classify(&with_envelope(15.0, 0.95), 1500.0),
            PlanetKind::HotJupiter
        );
    }

    proptest! {
        /// Radii are positive, finite and grow with mass for bare planets;
        /// escape never adds gas.
        #[test]
        fn radii_and_escape_are_well_behaved(
            core in 0.05f64..30.0, f in 0.0f64..0.95, water in 0.0f64..0.5,
            a in 0.02f64..30.0, age in 0.05f64..10.0,
        ) {
            let makeup = Makeup {
                core_mearth: core,
                envelope_mearth: core * f / (1.0 - f),
                water_fraction: water,
            };
            let r = radius(&makeup, flux_earth(1.0, a), age);
            prop_assert!(r.is_finite() && r > 0.1 && r < 30.0, "{}", r);
            prop_assert!(solid_radius(core * 1.5, water) > solid_radius(core, water));
            let left = envelope_after_escape(&makeup, a, 1.0, 1.0, age);
            prop_assert!(left >= 0.0 && left <= makeup.envelope_mearth + 1e-12);
        }
    }
}
