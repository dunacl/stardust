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
}
