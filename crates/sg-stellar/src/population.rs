//! Stellar populations: how massive stars are, how often they have
//! companions, and what the companions' orbits look like (PLAN.md §5.1).
//!
//! Every function here is a statistical description of observed stars, with
//! the scatter and incompleteness that implies. Sources and simplifications
//! are given per function.

use sg_core::math::{log10, pow};
use sg_core::rng::RngStream;
use sg_core::units::{Days, SolarMass};

/// Smallest mass that fuses hydrogen: the stellar/brown-dwarf boundary.
pub const HYDROGEN_BURNING_LIMIT_MSUN: f64 = 0.08;

/// Mass at quantile `u ∈ [0, 1)` of the stellar initial mass function
/// between `min` and `max`.
///
/// Kroupa (2001), MNRAS 322, 231, eq. 2: `ξ(m) ∝ m^-1.3` for 0.08 ≤ m < 0.5
/// M☉ and `∝ m^-2.3` above. Valid for stars, 0.08–150 M☉; the brown-dwarf
/// segment below 0.08 M☉ is not included, so `min` is raised to that limit.
pub fn kroupa_mass(u: f64, min: SolarMass, max: SolarMass) -> SolarMass {
    const BREAK: f64 = 0.5;
    let lo = min.0.max(HYDROGEN_BURNING_LIMIT_MSUN);
    let hi = max.0.max(lo);
    // ∫ m^-α dm between a and b, and its inverse from a.
    let integral =
        |a: f64, b: f64, alpha: f64| (pow(b, 1.0 - alpha) - pow(a, 1.0 - alpha)) / (1.0 - alpha);
    let invert = |a: f64, area: f64, alpha: f64| {
        pow(
            pow(a, 1.0 - alpha) + area * (1.0 - alpha),
            1.0 / (1.0 - alpha),
        )
    };
    // Continuity at the break: the upper segment is scaled by 0.5^(2.3-1.3).
    let join = BREAK;
    let low = if lo < BREAK {
        integral(lo, hi.min(BREAK), 1.3)
    } else {
        0.0
    };
    let high = if hi > BREAK {
        join * integral(lo.max(BREAK), hi, 2.3)
    } else {
        0.0
    };
    let target = u.clamp(0.0, 1.0) * (low + high);
    let mass = if target < low {
        invert(lo, target, 1.3)
    } else {
        invert(lo.max(BREAK), (target - low) / join, 2.3)
    };
    SolarMass(mass.clamp(lo, hi))
}

/// How many stars a system has.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MultiplicityFractions {
    /// Fraction of systems that are single stars.
    pub single: f64,
    /// Fraction that are binaries.
    pub binary: f64,
    /// Fraction with three or more stars.
    pub triple_or_higher: f64,
}

/// Multiplicity as a function of the primary's mass.
///
/// Linear interpolation in log mass between survey results:
///
/// | primary | multiple | 3 or more | source |
/// |---|---|---|---|
/// | 0.1 M☉ | 22% | 3% | Duchêne & Kraus (2013), ARA&A 51, 269, table 1 |
/// | 0.3 M☉ | 27% | 5% | Winters et al. (2019), AJ 157, 216 |
/// | 1.0 M☉ | 44% | 11% | Raghavan et al. (2010), ApJS 190, 1 |
/// | 3.5 M☉ | 59% | 22% | Moe & Di Stefano (2017), ApJS 230, 15, table 13 |
/// | 7 M☉ | 76% | 37% | Moe & Di Stefano (2017), table 13 |
///
/// Held constant outside 0.1–7 M☉. The surveys' own uncertainties are
/// several percentage points, more for the higher-order fraction.
pub fn multiplicity_fractions(primary: SolarMass) -> MultiplicityFractions {
    const ANCHORS: [(f64, f64, f64); 5] = [
        (0.1, 0.22, 0.03),
        (0.3, 0.27, 0.05),
        (1.0, 0.44, 0.11),
        (3.5, 0.59, 0.22),
        (7.0, 0.76, 0.37),
    ];
    let m = primary.0.clamp(ANCHORS[0].0, ANCHORS[4].0);
    let upper = ANCHORS.iter().position(|a| a.0 >= m).unwrap_or(4).max(1);
    let (m0, f0, h0) = ANCHORS[upper - 1];
    let (m1, f1, h1) = ANCHORS[upper];
    let t = (log10(m) - log10(m0)) / (log10(m1) - log10(m0));
    let multiple = f0 + (f1 - f0) * t;
    let higher = h0 + (h1 - h0) * t;
    MultiplicityFractions {
        single: 1.0 - multiple,
        binary: multiple - higher,
        triple_or_higher: higher,
    }
}

/// Mean and standard deviation of `log10(P / days)` for companions of a
/// primary of the given mass: a log-normal period distribution.
///
/// Solar-type stars: mean 5.03, σ 2.28 (Raghavan et al. 2010, fig. 13).
/// M dwarfs: closer and narrower, mean ≈ 3.8 and σ ≈ 1.3 (Duchêne & Kraus
/// 2013, table 1: mean separation ≈ 5.3 AU; Winters et al. 2019).
/// Interpolated in log mass between 0.3 and 1 M☉ and held constant outside.
/// Stars above about 1.5 M☉ have a broader, two-peaked distribution (Moe &
/// Di Stefano 2017) that this does not reproduce.
pub fn log_period_distribution(primary: SolarMass) -> (f64, f64) {
    let t = ((log10(primary.0) - log10(0.3)) / (log10(1.0) - log10(0.3))).clamp(0.0, 1.0);
    (3.8 + (5.03 - 3.8) * t, 1.3 + (2.28 - 1.3) * t)
}

/// Shortest and longest companion periods drawn, as `log10(P / days)`:
/// 2 days (closer pairs interact) to 10⁸ days (wider pairs are unbound by
/// passing stars within a few Gyr; Raghavan et al. 2010, §5.3).
pub const LOG_PERIOD_RANGE: (f64, f64) = (0.3, 8.0);

/// Draws a companion's orbital period.
pub fn sample_period(rng: &mut RngStream, primary: SolarMass) -> Days {
    let (mean, sigma) = log_period_distribution(primary);
    loop {
        let log_p = mean + sigma * rng.standard_normal();
        if (LOG_PERIOD_RANGE.0..=LOG_PERIOD_RANGE.1).contains(&log_p) {
            return Days(pow(10.0, log_p));
        }
    }
}

/// Largest eccentricity a binary of period `P` can have without the stars
/// touching at periastron: `1 − (P / 2 days)^(-2/3)`, zero for P ≤ 2 days
/// (Moe & Di Stefano 2017, eq. 3).
pub fn max_eccentricity(period: Days) -> f64 {
    if period.0 <= 2.0 {
        0.0
    } else {
        1.0 - pow(period.0 / 2.0, -2.0 / 3.0)
    }
}

/// Draws a binary eccentricity for the given period.
///
/// Uniform between 0 and the smaller of [`max_eccentricity`] and 0.9: the
/// observed distribution for solar-type binaries is close to flat out to
/// about 0.6 and thins beyond, with short periods circularised by tides
/// (Raghavan et al. 2010, fig. 14). The cap at 0.9 is a simplification.
pub fn sample_eccentricity(rng: &mut RngStream, period: Days) -> f64 {
    rng.uniform_range(0.0, max_eccentricity(period).min(0.9))
}

/// Draws a companion mass for a primary: mass ratio `q = M₂/M₁` uniform in
/// `[0.1, 1]`, never below the hydrogen-burning limit.
///
/// The observed distribution for solar-type primaries is roughly flat in q
/// with an excess of near-equal "twins" (Raghavan et al. 2010, fig. 16);
/// the twin excess is not modelled.
pub fn sample_companion_mass(rng: &mut RngStream, primary: SolarMass) -> SolarMass {
    let q_min = (HYDROGEN_BURNING_LIMIT_MSUN / primary.0).clamp(0.1, 1.0);
    SolarMass(primary.0 * rng.uniform_range(q_min, 1.0))
}

/// Median of the Kroupa IMF over a range, for tests and diagnostics.
pub fn kroupa_median(min: SolarMass, max: SolarMass) -> SolarMass {
    kroupa_mass(0.5, min, max)
}

/// Natural log of the ratio of star counts per unit mass at two masses, for
/// tests: `ln(ξ(a)/ξ(b))`.
#[cfg(test)]
fn log_imf_ratio(a: f64, b: f64) -> f64 {
    use sg_core::math::ln;
    let xi = |m: f64| {
        if m < 0.5 {
            pow(m, -1.3)
        } else {
            0.5 * pow(m, -2.3)
        }
    };
    ln(xi(a) / xi(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sg_core::math::ln;
    use sg_core::rng::ModuleId;

    const LO: SolarMass = SolarMass(0.08);
    const HI: SolarMass = SolarMass(100.0);

    #[test]
    fn imf_quantiles_span_the_range_and_increase() {
        assert!((kroupa_mass(0.0, LO, HI).0 - 0.08).abs() < 1e-12);
        assert!((kroupa_mass(1.0, LO, HI).0 - 100.0).abs() < 1e-6);
        let mut last = 0.0;
        for k in 0..=100 {
            let m = kroupa_mass(f64::from(k) / 100.0, LO, HI).0;
            assert!(m >= last);
            last = m;
        }
    }

    /// Most stars are red dwarfs: the median of the Kroupa IMF over
    /// 0.08–100 M☉ is about 0.2 M☉, and roughly 10% of stars exceed 1 M☉.
    #[test]
    fn imf_is_dominated_by_low_mass_stars() {
        let median = kroupa_median(LO, HI).0;
        assert!((0.17..0.25).contains(&median), "{median}");
        let above_solar = (0..10_000)
            .filter(|k| kroupa_mass(f64::from(*k) / 10_000.0, LO, HI).0 > 1.0)
            .count();
        assert!((500..1200).contains(&above_solar), "{above_solar}");
    }

    /// The sampled density reproduces the two power-law slopes.
    #[test]
    fn imf_slopes_match_kroupa() {
        let n = 400_000;
        let count = |lo: f64, hi: f64| {
            (0..n)
                .filter(|k| {
                    let m = kroupa_mass((f64::from(*k) + 0.5) / f64::from(n), LO, HI).0;
                    (lo..hi).contains(&m)
                })
                .count() as f64
        };
        // Counts in narrow bins of equal width around two masses.
        let low = ln(count(0.10, 0.11) / count(0.30, 0.31));
        assert!((low - log_imf_ratio(0.105, 0.305)).abs() < 0.05, "{low}");
        let high = ln(count(1.0, 1.1) / count(3.0, 3.1));
        assert!((high - log_imf_ratio(1.05, 3.05)).abs() < 0.08, "{high}");
    }

    #[test]
    fn imf_respects_a_restricted_range() {
        for k in 0..=50 {
            let m = kroupa_mass(f64::from(k) / 50.0, SolarMass(0.7), SolarMass(1.4)).0;
            assert!((0.7..=1.4).contains(&m));
        }
        // Brown dwarfs are never drawn.
        assert!(kroupa_mass(0.0, SolarMass(0.01), HI).0 >= 0.08);
    }

    #[test]
    fn multiplicity_matches_the_surveys_at_their_anchor_masses() {
        let sun = multiplicity_fractions(SolarMass(1.0));
        assert!((sun.single - 0.56).abs() < 1e-12);
        assert!((sun.binary - 0.33).abs() < 1e-12);
        assert!((sun.triple_or_higher - 0.11).abs() < 1e-12);
        let m_dwarf = multiplicity_fractions(SolarMass(0.3));
        assert!((m_dwarf.single - 0.73).abs() < 1e-12);
    }

    #[test]
    fn multiplicity_rises_with_mass_and_sums_to_one() {
        let mut last = 1.0;
        for mass in [0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 7.0, 30.0] {
            let f = multiplicity_fractions(SolarMass(mass));
            assert!((f.single + f.binary + f.triple_or_higher - 1.0).abs() < 1e-12);
            assert!(f.single <= last + 1e-12, "{mass}");
            assert!(f.binary > 0.0 && f.triple_or_higher > 0.0);
            last = f.single;
        }
    }

    #[test]
    fn periods_follow_the_log_normal() {
        assert_eq!(log_period_distribution(SolarMass(1.0)), (5.03, 2.28));
        let (mean, sigma) = log_period_distribution(SolarMass(0.2));
        assert!((mean - 3.8).abs() < 1e-12 && (sigma - 1.3).abs() < 1e-12);

        let mut rng = RngStream::new(11, ModuleId::STELLAR);
        let n = 20_000;
        let logs: Vec<f64> = (0..n)
            .map(|_| log10(sample_period(&mut rng, SolarMass(1.0)).0))
            .collect();
        assert!(logs.iter().all(|l| (0.3..=8.0).contains(l)));
        let mean = logs.iter().sum::<f64>() / f64::from(n);
        // Truncation at 0.3 and 8 pulls the mean down a little from 5.03.
        assert!((mean - 4.75).abs() < 0.15, "{mean}");
    }

    #[test]
    fn close_binaries_are_circular() {
        assert_eq!(max_eccentricity(Days(1.0)), 0.0);
        assert_eq!(max_eccentricity(Days(2.0)), 0.0);
        // 16 days: 1 − 8^(-2/3) = 0.75.
        assert!((max_eccentricity(Days(16.0)) - 0.75).abs() < 1e-12);
        let mut rng = RngStream::new(5, ModuleId::STELLAR);
        for _ in 0..2000 {
            assert!(sample_eccentricity(&mut rng, Days(4.0)) <= max_eccentricity(Days(4.0)));
            assert!(sample_eccentricity(&mut rng, Days(1.0e6)) <= 0.9);
        }
    }

    #[test]
    fn companions_are_stars_no_heavier_than_the_primary() {
        let mut rng = RngStream::new(3, ModuleId::STELLAR);
        for primary in [0.1, 0.5, 1.0, 5.0] {
            for _ in 0..2000 {
                let m = sample_companion_mass(&mut rng, SolarMass(primary)).0;
                assert!(
                    m <= primary && m >= 0.08f64.min(primary) - 1e-12,
                    "{primary}: {m}"
                );
            }
        }
    }
}
