//! Physical and astronomical constants.
//!
//! Solar and planetary values are the IAU 2015 Resolution B3 nominal values
//! (Prša et al. 2016, AJ 152, 41); physical constants are CODATA 2018.
//!
//! The accretion model's own constants are in `sg-accrete`, as its sources give
//! them.

/// Stefan–Boltzmann constant, W m⁻² K⁻⁴ (CODATA 2018).
pub const STEFAN_BOLTZMANN_W_M2_K4: f64 = 5.670_374_419e-8;

/// Nominal solar luminosity, W (IAU 2015 B3).
pub const SOLAR_LUMINOSITY_W: f64 = 3.828e26;

/// Nominal solar radius, m (IAU 2015 B3).
pub const SOLAR_RADIUS_M: f64 = 6.957e8;

/// Nominal solar effective temperature, K (IAU 2015 B3).
pub const SOLAR_TEFF_K: f64 = 5772.0;

/// Astronomical unit, m (IAU 2012 Resolution B2, exact).
pub const AU_M: f64 = 1.495_978_707e11;

/// Solar mass in Earth masses, from the IAU 2015 B3 nominal mass parameters
/// (GM☉ = 1.3271244e20, GM⊕ = 3.986004e14 m³ s⁻²).
pub const SOLAR_MASS_IN_EARTH_MASSES: f64 = 1.327_124_4e20 / 3.986_004e14;

/// Nominal equatorial Earth radius, km (IAU 2015 B3).
pub const EARTH_RADIUS_KM: f64 = 6378.1;

/// Days in a Julian year.
pub const DAYS_PER_YEAR: f64 = 365.25;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{pow, sqrt};

    /// The IAU nominal Sun is self-consistent: L = 4πR²σT⁴ gives 5772 K.
    #[test]
    fn nominal_sun_is_self_consistent() {
        let area = 4.0 * core::f64::consts::PI * SOLAR_RADIUS_M * SOLAR_RADIUS_M;
        let teff = sqrt(sqrt(SOLAR_LUMINOSITY_W / (area * STEFAN_BOLTZMANN_W_M2_K4)));
        assert!((teff - SOLAR_TEFF_K).abs() < 0.5, "teff = {teff}");
        assert!(
            (pow(teff, 4.0) * area * STEFAN_BOLTZMANN_W_M2_K4 / SOLAR_LUMINOSITY_W - 1.0).abs()
                < 1e-12
        );
    }

    #[test]
    fn solar_mass_ratio_matches_published_value() {
        // 332 946.0 (IAU 2015 B3 nominal ratio).
        assert!((SOLAR_MASS_IN_EARTH_MASSES - 332_946.0).abs() < 0.1);
    }
}
