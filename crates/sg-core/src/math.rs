//! Deterministic math.
//!
//! Simulation code must call these instead of the `f64` inherent methods
//! (`f64::powf`, `f64::exp`, …). The inherent methods defer to the platform's
//! C library, whose results differ in the last bit between operating systems
//! and between native and WASM. `libm` is a pure-Rust implementation, so the
//! same inputs give the same bits everywhere (PLAN.md §6.4).
//!
//! `+ - * /` and [`sqrt`] are exactly rounded under IEEE-754 and are safe to
//! use directly.

/// `x` raised to the power `y`.
#[inline]
pub fn pow(x: f64, y: f64) -> f64 {
    libm::pow(x, y)
}

/// Natural exponential, `e^x`.
#[inline]
pub fn exp(x: f64) -> f64 {
    libm::exp(x)
}

/// Natural logarithm.
#[inline]
pub fn ln(x: f64) -> f64 {
    libm::log(x)
}

/// Base-10 logarithm.
#[inline]
pub fn log10(x: f64) -> f64 {
    libm::log10(x)
}

/// Square root (exactly rounded).
#[inline]
pub fn sqrt(x: f64) -> f64 {
    libm::sqrt(x)
}

/// Cube root.
#[inline]
pub fn cbrt(x: f64) -> f64 {
    libm::cbrt(x)
}

/// Sine of an angle in radians.
#[inline]
pub fn sin(x: f64) -> f64 {
    libm::sin(x)
}

/// Cosine of an angle in radians.
#[inline]
pub fn cos(x: f64) -> f64 {
    libm::cos(x)
}

/// Angle of the point `(x, y)` from the positive x axis, in `(−π, π]`.
#[inline]
pub fn atan2(y: f64, x: f64) -> f64 {
    libm::atan2(y, x)
}

/// Arc cosine, in `[0, π]`. The argument is clamped to `[−1, 1]`, so
/// rounding just outside that range gives 0 or π rather than NaN.
#[inline]
pub fn acos(x: f64) -> f64 {
    libm::acos(x.clamp(-1.0, 1.0))
}

/// Absolute value.
#[inline]
pub fn abs(x: f64) -> f64 {
    libm::fabs(x)
}

/// `x²`, by multiplication.
#[inline]
pub fn pow2(x: f64) -> f64 {
    x * x
}

/// `x³`, by multiplication (left to right).
#[inline]
pub fn pow3(x: f64) -> f64 {
    x * x * x
}

/// `x⁴`, by multiplication (left to right).
#[inline]
pub fn pow4(x: f64) -> f64 {
    x * x * x * x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_exact_values() {
        assert_eq!(pow(2.0, 10.0), 1024.0);
        assert_eq!(sqrt(144.0), 12.0);
        assert_eq!(cbrt(27.0), 3.0);
        assert_eq!(exp(0.0), 1.0);
        assert_eq!(ln(1.0), 0.0);
        assert_eq!(log10(1000.0), 3.0);
        assert_eq!(cos(0.0), 1.0);
        assert_eq!(sin(0.0), 0.0);
        assert_eq!(abs(-3.5), 3.5);
        assert_eq!(pow4(3.0), 81.0);
    }

    #[test]
    fn exp_and_ln_are_inverse() {
        for x in [1e-3, 0.5, 1.0, 7.25, 300.0] {
            assert!((ln(exp(x)) - x).abs() < 1e-12 * x.max(1.0));
        }
    }
}
