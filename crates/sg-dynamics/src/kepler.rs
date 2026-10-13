//! Two-body (Keplerian) orbit geometry.
//!
//! Source: Murray, C. D. & Dermott, S. F. (1999), *Solar System Dynamics*,
//! §2.4 (Kepler's equation and the orbit in its plane) and §2.8 (orientation
//! in space). Valid for bound orbits, `0 ≤ e < 1`.

use sg_core::math::{abs, cos, sin, sqrt};
use sg_core::units::{Au, Days, SolarMass};

/// 2π.
const TAU: f64 = 2.0 * core::f64::consts::PI;

/// Days in a Julian year, for Kepler's third law in (AU, M☉, yr) units.
const DAYS_PER_YEAR: f64 = 365.25;

/// Reduces an angle in radians to `[0, 2π)`.
pub fn wrap_angle(angle: f64) -> f64 {
    let wrapped = angle % TAU;
    if wrapped < 0.0 {
        wrapped + TAU
    } else {
        wrapped
    }
}

/// Orbital period from Kepler's third law, `P² = a³ / M` in years, AU and
/// solar masses (Murray & Dermott eq. 2.22). `total_mass` is the sum of the
/// two bodies' masses. Exact for two bodies.
pub fn orbital_period(a: Au, total_mass: SolarMass) -> Days {
    Days(sqrt(a.0 * a.0 * a.0 / total_mass.0) * DAYS_PER_YEAR)
}

/// Solves Kepler's equation `M = E − e·sin E` for the eccentric anomaly `E`
/// (radians), given the mean anomaly `M` (radians) and eccentricity.
///
/// Newton–Raphson from the starting value `E₀ = M + e·sin M` (or π for very
/// eccentric orbits), with bisection as a safeguard (Murray & Dermott §2.4).
/// The result satisfies the equation to ~1e-14 for `0 ≤ e < 1`. `e` is
/// clamped into `[0, 1)`.
pub fn eccentric_anomaly(mean_anomaly: f64, e: f64) -> f64 {
    let e = e.clamp(0.0, 1.0 - 1.0e-12);
    let m = wrap_angle(mean_anomaly);
    if e == 0.0 {
        return m;
    }

    // The root lies in [0, 2π] and f(E) = E − e·sin E − M is increasing.
    let (mut lo, mut hi) = (0.0f64, TAU);
    let mut big_e = if e < 0.8 {
        m + e * sin(m)
    } else {
        core::f64::consts::PI
    };

    for _ in 0..64 {
        let f = big_e - e * sin(big_e) - m;
        if abs(f) < 1.0e-15 {
            break;
        }
        if f > 0.0 {
            hi = big_e;
        } else {
            lo = big_e;
        }
        let step = f / (1.0 - e * cos(big_e));
        let next = big_e - step;
        // Fall back to bisection if Newton leaves the bracket.
        big_e = if next > lo && next < hi {
            next
        } else {
            0.5 * (lo + hi)
        };
    }
    big_e
}

/// Orientation and phase of a Keplerian orbit; angles in radians.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Elements {
    /// Semi-major axis.
    pub a: Au,
    /// Eccentricity.
    pub e: f64,
    /// Inclination.
    pub i: f64,
    /// Longitude of the ascending node, Ω.
    pub long_asc_node: f64,
    /// Argument of periapsis, ω.
    pub arg_periapsis: f64,
    /// Mean anomaly, M.
    pub mean_anomaly: f64,
}

/// Position on the orbit relative to the focus, in AU, in the reference
/// frame whose `z` axis is the reference plane's normal.
///
/// In the orbital plane the position is `(a(cos E − e), a√(1−e²)·sin E)`
/// (Murray & Dermott eq. 2.41); it is then rotated by ω about `z`, `i` about
/// `x`, and Ω about `z` (eq. 2.122).
pub fn position(el: Elements) -> [f64; 3] {
    let e = el.e.clamp(0.0, 1.0 - 1.0e-12);
    let big_e = eccentric_anomaly(el.mean_anomaly, e);
    let x = el.a.0 * (cos(big_e) - e);
    let y = el.a.0 * sqrt(1.0 - e * e) * sin(big_e);
    rotate_to_reference(x, y, el)
}

/// Point of the orbit at eccentric anomaly `big_e`, for drawing the ellipse.
/// Same frame as [`position`].
pub fn point_at_eccentric_anomaly(el: Elements, big_e: f64) -> [f64; 3] {
    let e = el.e.clamp(0.0, 1.0 - 1.0e-12);
    let x = el.a.0 * (cos(big_e) - e);
    let y = el.a.0 * sqrt(1.0 - e * e) * sin(big_e);
    rotate_to_reference(x, y, el)
}

fn rotate_to_reference(x: f64, y: f64, el: Elements) -> [f64; 3] {
    let (so, co) = (sin(el.arg_periapsis), cos(el.arg_periapsis));
    let (si, ci) = (sin(el.i), cos(el.i));
    let (sn, cn) = (sin(el.long_asc_node), cos(el.long_asc_node));
    // Rz(Ω) · Rx(i) · Rz(ω) applied to (x, y, 0).
    let x1 = x * co - y * so;
    let y1 = x * so + y * co;
    [x1 * cn - y1 * ci * sn, x1 * sn + y1 * ci * cn, y1 * si]
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::f64::consts::{FRAC_PI_2, PI};
    use proptest::prelude::*;

    fn elements(a: f64, e: f64, mean_anomaly: f64) -> Elements {
        Elements {
            a: Au(a),
            e,
            i: 0.0,
            long_asc_node: 0.0,
            arg_periapsis: 0.0,
            mean_anomaly,
        }
    }

    fn norm(p: [f64; 3]) -> f64 {
        (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt()
    }

    /// Earth: 1 AU around 1 M☉ is one Julian year. Jupiter (5.2044 AU):
    /// 11.86 yr.
    #[test]
    fn third_law_for_earth_and_jupiter() {
        assert_eq!(orbital_period(Au(1.0), SolarMass(1.0)).0, 365.25);
        let jupiter = orbital_period(Au(5.2044), SolarMass(1.0 + 9.55e-4)).0 / 365.25;
        assert!((jupiter - 11.862).abs() < 0.01, "jupiter = {jupiter}");
    }

    /// Worked example: for e = 0.5 and E = π/2, Kepler's equation gives
    /// M = π/2 − 0.5.
    #[test]
    fn eccentric_anomaly_of_a_worked_example() {
        let big_e = eccentric_anomaly(FRAC_PI_2 - 0.5, 0.5);
        assert!((big_e - FRAC_PI_2).abs() < 1e-13, "E = {big_e}");
        // Circular orbit: E = M. Periapsis and apoapsis are fixed points.
        assert_eq!(eccentric_anomaly(1.234, 0.0), 1.234);
        assert_eq!(eccentric_anomaly(0.0, 0.7), 0.0);
        assert!((eccentric_anomaly(PI, 0.7) - PI).abs() < 1e-13);
    }

    /// Distances at periapsis and apoapsis are a(1 − e) and a(1 + e).
    #[test]
    fn apsides_are_at_the_right_distances() {
        let peri = position(elements(2.0, 0.3, 0.0));
        assert!((peri[0] - 1.4).abs() < 1e-12 && peri[1].abs() < 1e-12);
        let apo = position(elements(2.0, 0.3, PI));
        assert!((apo[0] + 2.6).abs() < 1e-12 && apo[1].abs() < 1e-9);
    }

    /// Rotations: ω turns the periapsis within the plane, i tilts the orbit
    /// about the line of nodes, Ω turns the node.
    #[test]
    fn orientation_angles_rotate_as_defined() {
        let mut el = elements(1.0, 0.0, 0.0);
        el.arg_periapsis = FRAC_PI_2;
        let p = position(el);
        assert!(p[0].abs() < 1e-12 && (p[1] - 1.0).abs() < 1e-12);
        el.i = FRAC_PI_2; // orbit now stands on the x axis
        let p = position(el);
        assert!(p[1].abs() < 1e-12 && (p[2] - 1.0).abs() < 1e-12);
        el.arg_periapsis = 0.0;
        el.long_asc_node = FRAC_PI_2; // node, hence periapsis, along +y
        let p = position(el);
        assert!(p[0].abs() < 1e-12 && (p[1] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn angles_wrap_into_one_turn() {
        assert!((wrap_angle(-FRAC_PI_2) - 1.5 * PI).abs() < 1e-15);
        assert!((wrap_angle(5.0 * PI) - PI).abs() < 1e-12);
    }

    proptest! {
        #[test]
        fn solution_satisfies_keplers_equation(m in 0.0f64..TAU, e in 0.0f64..0.995) {
            let big_e = eccentric_anomaly(m, e);
            let back = big_e - e * big_e.sin();
            prop_assert!((back - m).abs() < 1e-12, "residual {}", back - m);
        }

        #[test]
        fn radius_stays_between_the_apsides(
            m in 0.0f64..TAU, e in 0.0f64..0.99, a in 0.01f64..100.0,
            i in 0.0f64..PI, node in 0.0f64..TAU, peri in 0.0f64..TAU,
        ) {
            let el = Elements { a: Au(a), e, i, long_asc_node: node, arg_periapsis: peri, mean_anomaly: m };
            let r = norm(position(el));
            prop_assert!(r >= a * (1.0 - e) * (1.0 - 1e-9));
            prop_assert!(r <= a * (1.0 + e) * (1.0 + 1e-9));
            // The drawn ellipse passes through the body's position.
            let on_curve = point_at_eccentric_anomaly(el, eccentric_anomaly(m, e));
            prop_assert!(norm([on_curve[0] - position(el)[0], on_curve[1] - position(el)[1], on_curve[2] - position(el)[2]]) < 1e-9);
        }
    }
}
