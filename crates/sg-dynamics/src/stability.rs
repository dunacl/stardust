//! Analytic stability limits for multiple-star systems (PLAN.md §5.1, §5.2).
//!
//! - whether a triple star holds together (Mardling & Aarseth 2001);
//! - how far from a star a planet can orbit when the star has a companion,
//!   and how close a planet can orbit a pair (Holman & Wiegert 1999).
//!
//! All are fits to numerical integrations of coplanar systems. They say
//! where orbits survive for the length of the integrations (10⁴ binary
//! periods for Holman & Wiegert), not for a system's whole life, and they
//! ignore resonances, which carve unstable gaps outside the limits.

use sg_core::math::{pow, sqrt};
use sg_core::units::{Au, SolarMass};

/// Smallest stable ratio of the outer orbit's periastron distance to the
/// inner orbit's semi-major axis, for a hierarchical triple.
///
/// Mardling & Aarseth (2001), MNRAS 321, 398, eq. 90:
/// `(a_out(1−e_out)/a_in)_crit = 2.8·[(1+q_out)(1+e_out)/√(1−e_out)]^(2/5)`
/// with `q_out = m₃/(m₁+m₂)`. For coplanar, prograde orbits; inclined outer
/// orbits are somewhat more stable (their factor `1 − 0.3·i/180°`), which is
/// not applied here. Valid for `0 ≤ e_out < 1`.
pub fn triple_critical_ratio(inner_mass: SolarMass, outer_mass: SolarMass, e_outer: f64) -> f64 {
    let q_out = outer_mass.0 / inner_mass.0;
    2.8 * pow((1.0 + q_out) * (1.0 + e_outer) / sqrt(1.0 - e_outer), 0.4)
}

/// True if a hierarchical triple is stable by [`triple_critical_ratio`].
/// `inner_mass` is the combined mass of the inner pair.
pub fn triple_is_stable(
    a_inner: Au,
    a_outer: Au,
    e_outer: f64,
    inner_mass: SolarMass,
    outer_mass: SolarMass,
) -> bool {
    a_outer.0 * (1.0 - e_outer) / a_inner.0 > triple_critical_ratio(inner_mass, outer_mass, e_outer)
}

/// Eccentricity and mass-ratio ranges the Holman & Wiegert fits cover.
pub const HW_MAX_ECCENTRICITY: f64 = 0.8;

/// Largest stable semi-major axis for a planet orbiting one star of a
/// binary (an S-type orbit), as a fraction of the binary's semi-major axis.
///
/// Holman & Wiegert (1999), AJ 117, 621, eq. 1:
/// `a_c/a_b = 0.464 − 0.380μ − 0.631e + 0.586μe + 0.150e² − 0.198μe²`,
/// where `μ = m_companion/(m_host + m_companion)` and `e` is the binary's
/// eccentricity. Fitted for 0.1 ≤ μ ≤ 0.9 and 0 ≤ e ≤ 0.8, to about 4–11%;
/// inputs are clamped into that range. Never negative.
pub fn s_type_critical_ratio(host: SolarMass, companion: SolarMass, e_binary: f64) -> f64 {
    let mu = (companion.0 / (host.0 + companion.0)).clamp(0.1, 0.9);
    let e = e_binary.clamp(0.0, HW_MAX_ECCENTRICITY);
    let ratio =
        0.464 - 0.380 * mu - 0.631 * e + 0.586 * mu * e + 0.150 * e * e - 0.198 * mu * e * e;
    ratio.max(0.0)
}

/// Smallest stable semi-major axis for a planet orbiting both stars of a
/// binary (a P-type, circumbinary orbit), as a multiple of the binary's
/// semi-major axis.
///
/// Holman & Wiegert (1999), eq. 3:
/// `a_c/a_b = 1.60 + 5.10e − 2.22e² + 4.12μ − 4.27eμ − 5.09μ² + 4.61e²μ²`,
/// where `μ = m₂/(m₁+m₂)` is the lighter star's share of the mass. Fitted
/// for 0.1 ≤ μ ≤ 0.5 and 0 ≤ e ≤ 0.7, to about 3–6%; inputs are clamped
/// into that range.
pub fn p_type_critical_ratio(m1: SolarMass, m2: SolarMass, e_binary: f64) -> f64 {
    let lighter = m1.0.min(m2.0);
    let mu = (lighter / (m1.0 + m2.0)).clamp(0.1, 0.5);
    let e = e_binary.clamp(0.0, 0.7);
    1.60 + 5.10 * e - 2.22 * e * e + 4.12 * mu - 4.27 * e * mu - 5.09 * mu * mu
        + 4.61 * e * e * mu * mu
}

/// A planet's orbit, for the long-term stability of a planetary system.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlanetOrbit {
    pub mass_msun: f64,
    pub a_au: f64,
    pub e: f64,
    /// Inclination to the system's plane, radians.
    pub i_rad: f64,
}

/// Angular momentum deficit of a set of planets, in units where
/// `G·M★ = 1`: `Σ m·√a·(1 − √(1−e²)·cos i)`, the angular momentum they
/// would gain if every orbit were made circular and coplanar. It is
/// conserved by the secular (long-term averaged) motion, and can be passed
/// between planets (Laskar 1997, A&A 317, L75).
pub fn angular_momentum_deficit(planets: &[PlanetOrbit]) -> f64 {
    planets
        .iter()
        .map(|p| {
            p.mass_msun * sqrt(p.a_au) * (1.0 - sqrt(1.0 - p.e * p.e) * sg_core::math::cos(p.i_rad))
        })
        .sum()
}

/// Critical relative angular momentum deficit of a pair of planets: with
/// more than this (relative to the outer planet's circular angular
/// momentum) their orbits can be made to cross.
///
/// Laskar & Petit (2017), A&A 605, A72, eqs. 29, 35 and 39: with
/// `α = a/a′` and `γ = m/m′` (inner over outer), the critical inner
/// eccentricity `e₀` solves
/// `α·e + γ·e/√(α(1−e²) + γ²e²) − 1 + α = 0`, the outer one is
/// `e₀′ = 1 − α − α·e₀`, and
/// `C_c = γ√α·(1 − √(1−e₀²)) + (1 − √(1−e₀′²))`. Valid for `0 < α < 1`.
pub fn critical_amd(alpha: f64, gamma: f64) -> f64 {
    let alpha = alpha.clamp(1.0e-9, 1.0 - 1.0e-12);
    let f = |e: f64| {
        alpha * e + gamma * e / sqrt(alpha * (1.0 - e * e) + gamma * gamma * e * e) - 1.0 + alpha
    };
    // f(0) = α − 1 < 0 and f(1) = 2α > 0, and f rises with e.
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if f(mid) < 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let e0 = 0.5 * (lo + hi);
    let e0_outer = (1.0 - alpha - alpha * e0).clamp(0.0, 1.0);
    gamma * sqrt(alpha) * (1.0 - sqrt(1.0 - e0 * e0)) + (1.0 - sqrt(1.0 - e0_outer * e0_outer))
}

/// The AMD-stability coefficient `β` of each neighbouring pair of a
/// system, ordered outward: the system's whole angular momentum deficit,
/// relative to the outer planet's circular angular momentum, over the
/// pair's critical value ([`critical_amd`]). A pair with `β < 1` can never
/// be brought to cross orbits by slow exchanges of angular momentum, even
/// if all of the system's deficit were given to it: it is "AMD-stable"
/// (Laskar & Petit 2017, eq. 58). `β > 1` does not mean the pair will
/// collide, only that this argument does not forbid it. Mean-motion
/// resonances and close encounters are outside the argument either way.
pub fn amd_coefficients(planets: &[PlanetOrbit]) -> Vec<f64> {
    let mut sorted = planets.to_vec();
    sorted.sort_by(|a, b| a.a_au.total_cmp(&b.a_au));
    let deficit = angular_momentum_deficit(&sorted);
    sorted
        .windows(2)
        .map(|pair| {
            let (inner, outer) = (pair[0], pair[1]);
            let relative = deficit / (outer.mass_msun * sqrt(outer.a_au));
            relative / critical_amd(inner.a_au / outer.a_au, inner.mass_msun / outer.mass_msun)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// Equal masses, circular outer orbit: 2.8·2^0.4 = 3.695.
    #[test]
    fn triple_limit_for_equal_masses() {
        let ratio = triple_critical_ratio(SolarMass(1.0), SolarMass(1.0), 0.0);
        assert!((ratio - 2.8 * 2.0f64.powf(0.4)).abs() < 1e-12);
        assert!((ratio - 3.695).abs() < 1e-3);
    }

    #[test]
    fn eccentric_and_massive_tertiaries_need_more_room() {
        let base = triple_critical_ratio(SolarMass(2.0), SolarMass(0.5), 0.0);
        assert!(triple_critical_ratio(SolarMass(2.0), SolarMass(0.5), 0.5) > base);
        assert!(triple_critical_ratio(SolarMass(2.0), SolarMass(2.0), 0.0) > base);
        // α Cen AB (23.5 AU) with Proxima at about 8700 AU, e ≈ 0.5: stable.
        assert!(triple_is_stable(
            Au(23.5),
            Au(8700.0),
            0.5,
            SolarMass(2.0),
            SolarMass(0.12)
        ));
        // The same pair with a tertiary at 60 AU is not.
        assert!(!triple_is_stable(
            Au(23.5),
            Au(60.0),
            0.5,
            SolarMass(2.0),
            SolarMass(0.12)
        ));
    }

    /// Holman & Wiegert's own example: α Centauri (a = 23.4 AU, e = 0.52,
    /// μ = 0.45 for planets of A) is stable out to about 2.8 AU. Equal
    /// stars on a circular orbit: 0.464 − 0.19 = 0.274.
    #[test]
    fn s_type_limit_matches_published_cases() {
        let ratio = s_type_critical_ratio(SolarMass(1.1), SolarMass(0.907), 0.52);
        assert!((ratio * 23.4 - 2.8).abs() < 0.1, "{}", ratio * 23.4);
        let equal = s_type_critical_ratio(SolarMass(1.0), SolarMass(1.0), 0.0);
        assert!((equal - 0.274).abs() < 1e-12);
    }

    /// Kepler-16 (0.6897 + 0.2026 M☉, a = 0.2243 AU, e = 0.159): the limit
    /// is about 0.65 AU, and the planet orbits just outside it at 0.705 AU
    /// (Doyle et al. 2011, Science 333, 1602). Equal stars, circular:
    /// 1.60 + 2.06 − 1.2725 = 2.3875.
    #[test]
    fn p_type_limit_matches_published_cases() {
        let ratio = p_type_critical_ratio(SolarMass(0.6897), SolarMass(0.2026), 0.159);
        let limit = ratio * 0.2243;
        assert!((limit - 0.646).abs() < 0.01, "{limit}");
        assert!(limit < 0.7048);
        let equal = p_type_critical_ratio(SolarMass(1.0), SolarMass(1.0), 0.0);
        assert!((equal - 2.3875).abs() < 1e-12);
        // The order of the two stars does not matter.
        assert_eq!(
            p_type_critical_ratio(SolarMass(0.2), SolarMass(0.7), 0.3),
            p_type_critical_ratio(SolarMass(0.7), SolarMass(0.2), 0.3)
        );
    }

    proptest! {
        /// A planet can never be stable both inside the S-type limit and
        /// outside the P-type limit at the same distance: the zones never
        /// overlap, and eccentricity only shrinks them.
        #[test]
        fn stable_zones_never_overlap(m1 in 0.08f64..5.0, m2 in 0.08f64..5.0, e in 0.0f64..0.8) {
            let s = s_type_critical_ratio(SolarMass(m1), SolarMass(m2), e);
            let p = p_type_critical_ratio(SolarMass(m1), SolarMass(m2), e);
            prop_assert!((0.0..0.5).contains(&s));
            prop_assert!(p > 1.5 && p < 5.5);
            prop_assert!(s < p);
            prop_assert!(s <= s_type_critical_ratio(SolarMass(m1), SolarMass(m2), 0.0) + 1e-12);
        }
    }

    fn planet(mass_mearth: f64, a_au: f64, e: f64, i_deg: f64) -> PlanetOrbit {
        PlanetOrbit {
            mass_msun: mass_mearth * 3.003e-6,
            a_au,
            e,
            i_rad: i_deg.to_radians(),
        }
    }

    /// The Solar System, with inclinations to the invariable plane.
    fn solar_system() -> Vec<PlanetOrbit> {
        vec![
            planet(0.0553, 0.387, 0.2056, 6.3),
            planet(0.815, 0.723, 0.0068, 2.2),
            planet(1.0, 1.0, 0.0167, 1.6),
            planet(0.107, 1.524, 0.0934, 1.7),
            planet(317.8, 5.203, 0.0484, 0.3),
            planet(95.2, 9.537, 0.0539, 0.9),
            planet(14.5, 19.19, 0.0473, 1.0),
            planet(17.1, 30.07, 0.0086, 0.7),
        ]
    }

    #[test]
    fn critical_amd_has_its_limits_right() {
        // Orbits far apart can hold almost a whole circular angular
        // momentum before crossing; orbits side by side almost none.
        assert!(critical_amd(0.01, 1.0) > 0.8);
        assert!(critical_amd(0.99, 1.0) < 1e-3);
        // It falls as the orbits approach.
        let mut previous = critical_amd(0.05, 0.5);
        for k in 2..19 {
            let now = critical_amd(0.05 * f64::from(k), 0.5);
            assert!(now < previous);
            previous = now;
        }
        // The critical eccentricities put pericentre and apocentre
        // together: for a massless inner planet (γ → 0) the outer orbit
        // stays circular and the inner one reaches e = 1/α − 1.
        let alpha = 0.8;
        let c = critical_amd(alpha, 1e-9);
        assert!(c < 1e-6, "{c}");
    }

    /// Laskar & Petit (2017), §5 and table 2: the outer Solar System is
    /// AMD-stable; the inner planets are not, taken with the giants' share
    /// of the deficit, and are when taken on their own.
    #[test]
    fn the_outer_solar_system_is_amd_stable_and_the_inner_is_not() {
        let all = solar_system();
        let beta = amd_coefficients(&all);
        assert_eq!(beta.len(), 7);
        // Jupiter–Saturn, Saturn–Uranus, Uranus–Neptune.
        for (k, b) in beta.iter().enumerate().skip(4) {
            assert!(*b < 1.0, "outer pair {k}: {b}");
        }
        // Mercury–Venus, Venus–Earth, Earth–Mars with the whole system's
        // deficit, nearly all of it the giants'.
        for (k, b) in beta.iter().enumerate().take(3) {
            assert!(*b > 1.0, "inner pair {k}: {b}");
        }
        // The inner four alone, as if cut off from the giants.
        let inner = amd_coefficients(&all[..4]);
        assert!(inner.iter().all(|b| *b < 1.0), "{inner:?}");
    }

    #[test]
    fn circular_coplanar_systems_have_no_deficit() {
        let system = [planet(1.0, 1.0, 0.0, 0.0), planet(2.0, 1.2, 0.0, 0.0)];
        assert_eq!(angular_momentum_deficit(&system), 0.0);
        assert_eq!(amd_coefficients(&system), vec![0.0]);
        assert!(amd_coefficients(&system[..1]).is_empty());
    }
}
