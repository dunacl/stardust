//! Stellar relations (PLAN.md §5.1).
//!
//! M1 scope: main-sequence scaling relations, effective temperature, spectral
//! class, and the Kopparapu habitable zone that replaces StarGen's constant
//! ecosphere. The IMF, multiplicity and evolution tracks arrive in M3, when
//! the scaling relations here are replaced by the Tout et al. (1996) /
//! Hurley et al. (2000) fits.

pub mod evolution;

use sg_core::constants::SOLAR_TEFF_K;
use sg_core::math::{pow, sqrt};
use sg_core::units::{Au, Kelvin, SolarLuminosity, SolarMass, SolarRadius};

/// Errors of the stellar relations.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum StellarError {
    #[error("{quantity} = {value} is outside the valid range [{min}, {max}]")]
    OutOfRange {
        quantity: &'static str,
        value: f64,
        min: f64,
        max: f64,
    },
}

/// Main-sequence luminosity from mass.
///
/// Piecewise power law from Duric (2004), *Advanced Astrophysics*, §1.3.4,
/// and Salaris & Cassisi (2005), *Evolution of Stars and Stellar
/// Populations*:
///
/// - `L = 0.23·M^2.3` for M < 0.43
/// - `L = M^4` for 0.43 ≤ M < 2
/// - `L = 1.4·M^3.5` for 2 ≤ M < 55
/// - `L = 32000·M` for M ≥ 55
///
/// Valid for main-sequence stars of roughly 0.1–100 M☉; accuracy is a few
/// tens of percent, and the pieces do not join exactly. An M3 placeholder.
pub fn main_sequence_luminosity(mass: SolarMass) -> SolarLuminosity {
    let m = mass.0;
    SolarLuminosity(if m < 0.43 {
        0.23 * pow(m, 2.3)
    } else if m < 2.0 {
        pow(m, 4.0)
    } else if m < 55.0 {
        1.4 * pow(m, 3.5)
    } else {
        32000.0 * m
    })
}

/// Main-sequence radius from mass.
///
/// Empirical power law `R = M^0.8` below 1 M☉ and `R = M^0.57` above, the
/// standard two-slope approximation to the observed mass–radius relation of
/// detached eclipsing binaries (compare Demircan & Kahraman 1991, *Ap&SS*
/// 181, 313, who fit slopes of 0.95 and 0.56 with slightly different
/// normalisation). Valid for roughly 0.1–20 M☉ main-sequence stars, accurate
/// to ~15%. An M3 placeholder.
pub fn main_sequence_radius(mass: SolarMass) -> SolarRadius {
    let m = mass.0;
    SolarRadius(if m < 1.0 { pow(m, 0.8) } else { pow(m, 0.57) })
}

/// Effective temperature from luminosity and radius.
///
/// Stefan–Boltzmann law, `L = 4πR²σT⁴`, in solar units:
/// `T = T☉·(L/R²)^(1/4)` with the IAU 2015 nominal `T☉ = 5772 K`. Exact for
/// any star, by the definition of effective temperature.
pub fn effective_temperature(luminosity: SolarLuminosity, radius: SolarRadius) -> Kelvin {
    Kelvin(SOLAR_TEFF_K * sqrt(sqrt(luminosity.0 / (radius.0 * radius.0))))
}

/// Harvard spectral class with decimal subclass, e.g. `G2`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpectralClass {
    /// One of `O B A F G K M`, or `L` for anything cooler.
    pub letter: char,
    /// 0 (hottest) to 9 (coolest) within the class.
    pub subclass: u8,
}

impl core::fmt::Display for SpectralClass {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}{}", self.letter, self.subclass)
    }
}

/// Lower temperature bound of each class, K, hottest first. Harvard class
/// boundaries for main-sequence stars as tabulated by Habets & Heintze
/// (1981), *A&AS* 46, 193.
const CLASS_BOUNDS: [(char, f64); 7] = [
    ('O', 30_000.0),
    ('B', 10_000.0),
    ('A', 7_500.0),
    ('F', 6_000.0),
    ('G', 5_200.0),
    ('K', 3_700.0),
    ('M', 2_400.0),
];

/// Upper bound assumed for class O when interpolating subclasses, K.
const O_CLASS_TOP_K: f64 = 50_000.0;

/// Spectral class from effective temperature.
///
/// Class boundaries per Habets & Heintze (1981); the subclass is a linear
/// interpolation in temperature across the class, which is accurate to about
/// one subclass. Valid for main-sequence stars; below 2400 K the result is
/// `L` with subclass 0.
pub fn spectral_class(teff: Kelvin) -> SpectralClass {
    let t = teff.0;
    let mut upper = O_CLASS_TOP_K;
    for (letter, lower) in CLASS_BOUNDS {
        if t >= lower {
            let position = ((upper - t) / (upper - lower)).clamp(0.0, 1.0);
            let subclass = ((position * 10.0) as u8).min(9);
            return SpectralClass { letter, subclass };
        }
        upper = lower;
    }
    SpectralClass {
        letter: 'L',
        subclass: 0,
    }
}

/// Highest temperature of Krystek's fit, K.
const KRYSTEK_MAX_K: f64 = 15_000.0;
/// Lowest temperature of Krystek's fit, K.
const KRYSTEK_MIN_K: f64 = 1_000.0;

/// CIE 1931 chromaticity `(x, y)` of a blackbody.
///
/// Krystek, M. (1985), "An algorithm to calculate correlated colour
/// temperature", *Color Research & Application* 10, 38: rational fits to the
/// Planckian locus in CIE 1960 `(u, v)`, converted to `(x, y)`. Valid for
/// 1000–15000 K, where it is accurate to about 1e-4 in chromaticity. Outside
/// that range the temperature is clamped: hotter blackbodies change colour
/// very little (the locus converges on a blue-white limit).
pub fn blackbody_chromaticity(teff: Kelvin) -> (f64, f64) {
    let t = teff.0.clamp(KRYSTEK_MIN_K, KRYSTEK_MAX_K);
    let t2 = t * t;
    let u = (0.860117757 + 1.54118254e-4 * t + 1.28641212e-7 * t2)
        / (1.0 + 8.42420235e-4 * t + 7.08145163e-7 * t2);
    let v = (0.317398726 + 4.22806245e-5 * t + 4.20481691e-8 * t2)
        / (1.0 - 2.89741816e-5 * t + 1.61456053e-7 * t2);
    let denom = 2.0 * u - 8.0 * v + 4.0;
    (3.0 * u / denom, 2.0 * v / denom)
}

/// Colour of a blackbody as linear sRGB, scaled so the largest channel is 1.
///
/// [`blackbody_chromaticity`] converted with the XYZ → linear sRGB matrix of
/// IEC 61966-2-1 (D65 white). Channels that fall outside the sRGB gamut
/// (cool stars are redder than sRGB can show) are clipped to 0. This is the
/// star's hue only; brightness is carried by its luminosity. Same validity
/// as [`blackbody_chromaticity`].
pub fn blackbody_linear_srgb(teff: Kelvin) -> [f64; 3] {
    let (x, y) = blackbody_chromaticity(teff);
    let (cx, cy, cz) = (x / y, 1.0, (1.0 - x - y) / y);
    let rgb = [
        3.2406 * cx - 1.5372 * cy - 0.4986 * cz,
        -0.9689 * cx + 1.8758 * cy + 0.0415 * cz,
        0.0557 * cx - 0.2040 * cy + 1.0570 * cz,
    ]
    .map(|c| c.max(0.0));
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    rgb.map(|c| c / max)
}

/// A habitable-zone boundary of Kopparapu et al. (2014).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HzLimit {
    /// Optimistic inner edge.
    RecentVenus,
    /// Conservative inner edge (1 M⊕ planet).
    RunawayGreenhouse,
    /// Conservative outer edge.
    MaximumGreenhouse,
    /// Optimistic outer edge.
    EarlyMars,
}

impl HzLimit {
    /// `(S_eff☉, a, b, c, d)` from Kopparapu et al. (2014), Table 1, for a
    /// 1 M⊕ planet. Checked against the authors' `hzcalc.py`.
    fn coefficients(self) -> (f64, f64, f64, f64, f64) {
        match self {
            HzLimit::RecentVenus => (1.776, 2.136e-4, 2.533e-8, -1.332e-11, -3.097e-15),
            HzLimit::RunawayGreenhouse => (1.107, 1.332e-4, 1.580e-8, -8.308e-12, -1.931e-15),
            HzLimit::MaximumGreenhouse => (0.356, 6.171e-5, 1.698e-9, -3.198e-12, -5.575e-16),
            HzLimit::EarlyMars => (0.320, 5.547e-5, 1.526e-9, -2.874e-12, -5.011e-16),
        }
    }
}

/// Effective stellar flux (in units of the solar constant) at a
/// habitable-zone boundary.
///
/// Kopparapu et al. (2014), *ApJL* 787, L29, eq. 4:
/// `S_eff = S_eff☉ + a·T + b·T² + c·T³ + d·T⁴` with `T = T_eff − 5780 K`.
/// Valid for 2600 K ≤ T_eff ≤ 7200 K; outside that range an error is
/// returned rather than an extrapolation.
pub fn hz_flux(limit: HzLimit, teff: Kelvin) -> Result<f64, StellarError> {
    if !(2600.0..=7200.0).contains(&teff.0) {
        return Err(StellarError::OutOfRange {
            quantity: "effective temperature (K)",
            value: teff.0,
            min: 2600.0,
            max: 7200.0,
        });
    }
    let (s, a, b, c, d) = limit.coefficients();
    let t = teff.0 - 5780.0;
    Ok(s + a * t + b * t * t + c * t * t * t + d * t * t * t * t)
}

/// Orbital distance of a habitable-zone boundary.
///
/// Kopparapu et al. (2014), eq. 5: `d = sqrt((L/L☉) / S_eff)` AU. Same
/// validity range as [`hz_flux`].
pub fn hz_distance(
    limit: HzLimit,
    luminosity: SolarLuminosity,
    teff: Kelvin,
) -> Result<Au, StellarError> {
    Ok(Au(sqrt(luminosity.0 / hz_flux(limit, teff)?)))
}

/// The four habitable-zone boundaries of a star, AU.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HabitableZone {
    pub recent_venus: Au,
    pub runaway_greenhouse: Au,
    pub maximum_greenhouse: Au,
    pub early_mars: Au,
}

/// All four Kopparapu et al. (2014) boundaries for a star; see
/// [`hz_distance`].
pub fn habitable_zone(
    luminosity: SolarLuminosity,
    teff: Kelvin,
) -> Result<HabitableZone, StellarError> {
    Ok(HabitableZone {
        recent_venus: hz_distance(HzLimit::RecentVenus, luminosity, teff)?,
        runaway_greenhouse: hz_distance(HzLimit::RunawayGreenhouse, luminosity, teff)?,
        maximum_greenhouse: hz_distance(HzLimit::MaximumGreenhouse, luminosity, teff)?,
        early_mars: hz_distance(HzLimit::EarlyMars, luminosity, teff)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn sun_is_the_unit_star() {
        assert_eq!(main_sequence_luminosity(SolarMass(1.0)).0, 1.0);
        assert_eq!(main_sequence_radius(SolarMass(1.0)).0, 1.0);
        let teff = effective_temperature(SolarLuminosity(1.0), SolarRadius(1.0));
        assert_eq!(teff.0, 5772.0);
        assert_eq!(spectral_class(teff).to_string(), "G2");
    }

    /// Published values: Sirius A (2.06 M☉) has L = 25.4 L☉; Proxima Centauri
    /// (0.122 M☉) has L = 0.0017 L☉. The power laws are good to tens of
    /// percent.
    #[test]
    fn luminosity_of_known_stars() {
        let sirius = main_sequence_luminosity(SolarMass(2.06)).0;
        assert!((sirius / 25.4 - 1.0).abs() < 0.35, "sirius = {sirius}");
        let proxima = main_sequence_luminosity(SolarMass(0.122)).0;
        assert!((proxima / 0.0017 - 1.0).abs() < 0.35, "proxima = {proxima}");
    }

    /// Published radii: Sirius A 1.71 R☉, α Cen B (0.91 M☉) 0.86 R☉.
    #[test]
    fn radius_of_known_stars() {
        let sirius = main_sequence_radius(SolarMass(2.06)).0;
        assert!((sirius / 1.71 - 1.0).abs() < 0.15, "sirius = {sirius}");
        let alf_cen_b = main_sequence_radius(SolarMass(0.91)).0;
        assert!(
            (alf_cen_b / 0.86 - 1.0).abs() < 0.15,
            "alf cen b = {alf_cen_b}"
        );
    }

    /// Stefan–Boltzmann with measured L and R: Sirius A (25.4 L☉, 1.711 R☉)
    /// has T_eff = 9940 K; α Cen A (1.519 L☉, 1.2234 R☉) has 5790 K.
    #[test]
    fn effective_temperature_of_known_stars() {
        let sirius = effective_temperature(SolarLuminosity(25.4), SolarRadius(1.711)).0;
        assert!((sirius - 9940.0).abs() < 100.0, "sirius = {sirius}");
        let alf_cen_a = effective_temperature(SolarLuminosity(1.519), SolarRadius(1.2234)).0;
        assert!((alf_cen_a - 5790.0).abs() < 15.0, "alf cen a = {alf_cen_a}");
    }

    #[test]
    fn spectral_classes_of_known_stars() {
        let class = |t: f64| spectral_class(Kelvin(t)).letter;
        assert_eq!(class(35_000.0), 'O');
        assert_eq!(class(9_940.0), 'A'); // Sirius A, A1
        assert_eq!(class(6_530.0), 'F'); // Procyon, F5
        assert_eq!(class(5_772.0), 'G'); // Sun, G2
        assert_eq!(class(5_260.0), 'G'); // α Cen B is K1 at 5260 K: boundary case
        assert_eq!(class(4_400.0), 'K'); // 61 Cyg A, K5
        assert_eq!(class(3_050.0), 'M'); // Proxima, M5.5
        assert_eq!(class(1_500.0), 'L');
        assert_eq!(spectral_class(Kelvin(3_050.0)).to_string(), "M5");
        assert_eq!(spectral_class(Kelvin(60_000.0)).subclass, 0);
    }

    /// Planckian locus chromaticities from the CIE tables: 2856 K (standard
    /// illuminant A) is (0.4476, 0.4074); 6500 K is (0.3135, 0.3237);
    /// 10000 K is (0.2807, 0.2884).
    #[test]
    fn blackbody_chromaticity_matches_the_planckian_locus() {
        for (t, x, y) in [
            (2856.0, 0.4476, 0.4074),
            (6500.0, 0.3135, 0.3237),
            (10_000.0, 0.2807, 0.2884),
        ] {
            let (gx, gy) = blackbody_chromaticity(Kelvin(t));
            assert!(
                (gx - x).abs() < 1e-3 && (gy - y).abs() < 1e-3,
                "{t} K: ({gx}, {gy})"
            );
        }
    }

    /// Cool stars are orange-red, the Sun is near white (slightly warm
    /// against the D65 white point), hot stars are blue-white.
    #[test]
    fn star_colours_run_from_red_to_blue() {
        let m_dwarf = blackbody_linear_srgb(Kelvin(3000.0));
        // Commonly tabulated sRGB for 3000 K is (255, 180, 107), i.e. about
        // (1, 0.47, 0.15) in linear light.
        assert_eq!(m_dwarf[0], 1.0);
        assert!((m_dwarf[1] - 0.47).abs() < 0.03, "{m_dwarf:?}");
        assert!((m_dwarf[2] - 0.15).abs() < 0.03, "{m_dwarf:?}");
        let sun = blackbody_linear_srgb(Kelvin(5772.0));
        assert!(sun[0] == 1.0 && sun[1] > 0.85 && sun[2] > 0.75, "{sun:?}");
        let a_star = blackbody_linear_srgb(Kelvin(9500.0));
        assert!(a_star[2] == 1.0 && a_star[0] < 0.75, "{a_star:?}");
        // Clamped outside the fit's range.
        assert_eq!(
            blackbody_linear_srgb(Kelvin(40_000.0)),
            blackbody_linear_srgb(Kelvin(15_000.0))
        );
    }

    /// Kopparapu et al. (2014) define their fluxes at T_eff = 5780 K, where
    /// S_eff = S_eff☉. For L = 1 the conservative zone is 0.95–1.68 AU and
    /// the optimistic one 0.75–1.77 AU, as quoted in the paper.
    #[test]
    fn solar_habitable_zone_matches_kopparapu_2014() {
        let sun = Kelvin(5780.0);
        assert_eq!(hz_flux(HzLimit::RunawayGreenhouse, sun).unwrap(), 1.107);
        assert_eq!(hz_flux(HzLimit::MaximumGreenhouse, sun).unwrap(), 0.356);
        let hz = habitable_zone(SolarLuminosity(1.0), sun).unwrap();
        assert!((hz.runaway_greenhouse.0 - 0.950).abs() < 1e-3);
        assert!((hz.maximum_greenhouse.0 - 1.676).abs() < 1e-3);
        assert!((hz.recent_venus.0 - 0.750).abs() < 1e-3);
        assert!((hz.early_mars.0 - 1.768).abs() < 1e-3);
    }

    /// A cooler star's light is redder and warms a planet more efficiently,
    /// so the boundary fluxes drop: for a 3000 K M dwarf the polynomial gives
    /// S_eff(runaway greenhouse) = 0.923 and S_eff(maximum greenhouse) = 0.233.
    #[test]
    fn m_dwarf_fluxes_match_the_polynomial() {
        let m = Kelvin(3000.0);
        let inner = hz_flux(HzLimit::RunawayGreenhouse, m).unwrap();
        let outer = hz_flux(HzLimit::MaximumGreenhouse, m).unwrap();
        assert!((inner - 0.923).abs() < 2e-3, "inner = {inner}");
        assert!((outer - 0.233).abs() < 2e-3, "outer = {outer}");
    }

    #[test]
    fn out_of_range_temperatures_are_rejected() {
        assert!(hz_flux(HzLimit::EarlyMars, Kelvin(2599.0)).is_err());
        assert!(hz_flux(HzLimit::EarlyMars, Kelvin(7201.0)).is_err());
        assert!(hz_flux(HzLimit::EarlyMars, Kelvin(7200.0)).is_ok());
    }

    proptest! {
        #[test]
        fn boundaries_are_ordered(teff in 2600.0f64..7200.0, lum in 1e-4f64..10.0) {
            let hz = habitable_zone(SolarLuminosity(lum), Kelvin(teff)).unwrap();
            prop_assert!(hz.recent_venus.0 < hz.runaway_greenhouse.0);
            prop_assert!(hz.runaway_greenhouse.0 < hz.maximum_greenhouse.0);
            prop_assert!(hz.maximum_greenhouse.0 < hz.early_mars.0);
        }

        #[test]
        fn relations_increase_with_mass(m in 0.1f64..50.0) {
            let a = SolarMass(m);
            let b = SolarMass(m * 1.2);
            // Radius is monotonic everywhere; luminosity is monotonic within
            // each piece of the power law (the pieces do not join exactly).
            prop_assert!(main_sequence_radius(a).0 < main_sequence_radius(b).0);
            let same_piece = [0.43, 2.0, 55.0].iter().all(|edge| (m < *edge) == (m * 1.2 < *edge));
            if same_piece {
                prop_assert!(main_sequence_luminosity(a).0 < main_sequence_luminosity(b).0);
            }
        }
    }
}
