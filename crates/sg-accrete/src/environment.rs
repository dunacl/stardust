//! Fogg's (1985) planetary relations, with Hart's (1978) water, ice and
//! greenhouse. Order-of-magnitude fits calibrated on the Solar System, for
//! planets of stars of 0.6–1.3 M☉; kept as the Classic model's own, and used
//! only where `sg-planet` has nothing to offer (spec gap 6).
//!
//! Temperatures are in kelvin, pressures in mb, distances in AU.

use sg_core::math::{exp, ln, pow, sqrt};

/// Which of Fogg's three formation zones an orbit lies in (F p.505).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    /// Refractory material.
    One,
    /// Hydrogen-rich, with ices.
    Two,
    /// Ices, with hydrogen and helium depleted.
    Three,
}

impl Zone {
    /// Fogg's boundaries at 1 M☉ are about 4 AU and 14–16 AU, scaling as
    /// `(L/L☉)^(1/2)` (F p.505). 15 AU is the middle of his second range.
    pub fn of(a_au: f64, luminosity_lsun: f64) -> Self {
        let scaled = a_au / sqrt(luminosity_lsun);
        if scaled < 4.0 {
            Zone::One
        } else if scaled < 15.0 {
            Zone::Two
        } else {
            Zone::Three
        }
    }
}

/// Inner edge of the zone free of a runaway greenhouse: `0.93 (L/L☉)^(1/2)`
/// AU (F Eqs 5, 6). Fogg's value is, in his word, optimistic.
pub fn runaway_limit_au(luminosity_lsun: f64) -> f64 {
    0.93 * sqrt(luminosity_lsun)
}

/// Density of a body below the critical mass, g/cm³:
/// `5.5 (m/m⊕)^(1/8) (r_ecos/R)^(1/4)`. Fogg (1985) Eq. 10, stated to give
/// the terrestrial planets, Titan and Pluto to ±10%.
pub fn terrestrial_density_g_cm3(mass_mearth: f64, a_au: f64, luminosity_lsun: f64) -> f64 {
    5.5 * pow(mass_mearth, 0.125) * pow(sqrt(luminosity_lsun) / a_au, 0.25)
}

/// Initial rotation, hours, from `ω = (j m / (k₂/2 · r²))^(1/2)`. Fogg (1985)
/// Eq. 12: `j = 1.46e-19` cm²/s²g, `k₂` about 0.33 for a terrestrial planet
/// and 0.24 for a giant. Good for planets not slowed by tides.
pub fn rotation_period_hours(mass_g: f64, radius_cm: f64, k2: f64) -> f64 {
    const J: f64 = 1.46e-19;
    let omega = sqrt(J * mass_g / (k2 / 2.0 * radius_cm * radius_cm));
    2.0 * core::f64::consts::PI / omega / 3600.0
}

/// Ratio of a resonant spin period to the orbital period, `(1 − e)/(1 + e)`.
/// Fogg (1985) Eq. 14, for tidally despun planets with `e > 0.1`.
pub fn resonance_ratio(e: f64) -> f64 {
    (1.0 - e) / (1.0 + e)
}

/// Whether a gas of molecular mass `m_u` stays for good: `V_e/V₀ ≥ 5`, with
/// `V₀ = √(2kT/m)`. Fogg (1985) Eqs 15, 16; `T` is the exosphere's.
pub fn retains(escape_km_s: f64, exosphere_k: f64, molecular_mass_u: f64) -> bool {
    const K_OVER_U_M2_S2_K: f64 = 8314.46; // Boltzmann constant / atomic mass unit
    let thermal_km_s = sqrt(2.0 * K_OVER_U_M2_S2_K * exosphere_k / molecular_mass_u) / 1000.0;
    escape_km_s >= 5.0 * thermal_km_s
}

/// Exospheric temperature: about 1000 °C, varying inversely with the square
/// root of perihelion distance (F p.507).
pub fn exosphere_k(perihelion_au: f64) -> f64 {
    1273.0 / sqrt(perihelion_au)
}

/// Volatile inventory `q (m/m⊕)(M/M☉)⁻¹ × scatter` (Fogg 1985, Eq. 17), with
/// `q` by zone (F p.507). In zone 1, outside the runaway limit, a planet
/// keeps 1% of it in the atmosphere, the rest in surface reservoirs.
/// `scatter` is his ±20%.
pub fn volatile_inventory(
    mass_mearth: f64,
    star_mass_msun: f64,
    zone: Zone,
    runaway: bool,
    scatter: f64,
) -> f64 {
    let q = match zone {
        Zone::One => 100_000.0,
        Zone::Two => 75_000.0,
        Zone::Three => 250.0,
    };
    let v = q * mass_mearth / star_mass_msun * scatter;
    if zone == Zone::One && !runaway {
        v / 100.0
    } else {
        v
    }
}

/// Surface pressure `V_inv (r/r⊕)⁻² (g/g⊕)`, mb. Fogg (1985) Eq. 18.
pub fn surface_pressure_mb(inventory: f64, radius_rearth: f64, gravity_gees: f64) -> f64 {
    inventory * gravity_gees / (radius_rearth * radius_rearth)
}

/// Boiling point of water, K: `(ln(P/1000)/(−5050.5) + 1/373)⁻¹`. Fogg
/// (1985) Eq. 21, which subtracts 273 for °C.
pub fn boiling_point_k(pressure_mb: f64) -> f64 {
    1.0 / (ln(pressure_mb / 1000.0) / -5050.5 + 1.0 / 373.0)
}

/// Fraction of the surface under water, `(0.75 V_inv/1000)(r/r⊕)⁻²`, at most
/// 1. Fogg (1985) Eq. 22.
pub fn ocean_fraction(inventory: f64, radius_rearth: f64) -> f64 {
    (0.75 * inventory / 1000.0 / (radius_rearth * radius_rearth)).min(1.0)
}

/// Cloud cover, proportional to the water vapour `Q₁ exp[Q₂(T − 288)]`
/// (Hart 1978, Eq. 3, `Q₂ = 0.0698` K⁻¹; Fogg 1985, Eq. 23), scaled to
/// Hart's present value of 0.472 (his Table I). At most 1.
pub fn cloud_fraction(surface_k: f64) -> f64 {
    (0.472 * exp(0.0698 * (surface_k - 288.0))).min(1.0)
}

/// Ice cover `((328 − T)/70)⁵` (Hart 1978, p.28; Fogg 1985, Eq. 24), at most
/// 1.5 times the ocean's fraction (F p.509) and at most all of the surface.
pub fn ice_fraction(surface_k: f64, ocean: f64) -> f64 {
    if surface_k >= 328.0 {
        return 0.0;
    }
    pow((328.0 - surface_k) / 70.0, 5.0)
        .min(1.5 * ocean)
        .min(1.0)
}

/// Albedo of a world with oceans: cloud 0.52, ice 0.70, ocean 0.04, rock 0.15
/// (Hart 1978, Table I; Fogg 1985, Table 3). Ice and open water share what
/// the clouds leave uncovered.
pub fn surface_albedo(cloud: f64, ocean: f64, ice: f64) -> f64 {
    let ice = ice.min(1.0);
    let water = (ocean - ice).max(0.0);
    let rock = (1.0 - ice - water).max(0.0);
    cloud * 0.52 + (1.0 - cloud) * (ice * 0.70 + water * 0.04 + rock * 0.15)
}

/// Effective temperature `(r_ecos/R)^(1/2) ((1 − A)/0.7)^(1/4) × 255` K.
/// Fogg (1985) Eq. 19.
pub fn effective_temperature_k(a_au: f64, luminosity_lsun: f64, albedo: f64) -> f64 {
    sqrt(sqrt(luminosity_lsun) / a_au) * pow((1.0 - albedo) / 0.7, 0.25) * 255.0
}

/// Convection factor (Fogg 1985, p.509).
const F_CONV: f64 = 0.43;

/// Earth's optical depth in this scheme. Neither paper prints one, but
/// `ΔT = [(1 + ¾τ)^(1/4) − 1] T_eff F_conv` with 33 K of greenhouse on a 255 K
/// planet fixes it: `τ = ((33/(255·0.43) + 1)⁴ − 1)/0.75`.
const EARTH_TAU: f64 = 2.4862;

/// Greenhouse warming `[(1 + ¾τ)^(1/4) − 1] T_eff F_conv`, K (Hart 1978,
/// Eq. 20; Fogg 1985, Eq. 20). An optical depth goes as the square root of a
/// gas's quantity (F p.509), so `τ` scales here as `√(P/1013)` from Earth's.
pub fn greenhouse_k(effective_k: f64, pressure_mb: f64) -> f64 {
    let tau = EARTH_TAU * sqrt(pressure_mb / 1013.0);
    (pow(1.0 + 0.75 * tau, 0.25) - 1.0) * effective_k * F_CONV
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fig. 11: 1.8 m⊕ at 0.965 AU has density 5.98.
    #[test]
    fn density_of_foggs_planet_three() {
        let d = terrestrial_density_g_cm3(1.8, 0.965, 1.0);
        assert!((d - 5.98).abs() < 0.02, "{d}");
    }

    /// Fig. 10: 4.9e23 kg (0.082 m⊕) at 0.34 AU has density 5.26.
    #[test]
    fn density_of_foggs_planet_one() {
        let d = terrestrial_density_g_cm3(0.082, 0.34, 1.0);
        assert!((d - 5.26).abs() < 0.03, "{d}");
    }

    /// The Earth rotates in about 15 hours at the start (F p.506).
    #[test]
    fn earth_starts_at_about_fifteen_hours() {
        let h = rotation_period_hours(5.97e27, 6.378e8, 0.33);
        assert!((h - 15.0).abs() < 1.0, "{h}");
    }

    /// Mercury's ratio is 2/3 at Fogg's rounded e = 0.2 (F p.506).
    #[test]
    fn mercury_resonance() {
        assert!((resonance_ratio(0.2) - 2.0 / 3.0).abs() < 1e-12);
    }

    /// Fig. 11: water boils at 112.58 °C under 1555 mb.
    #[test]
    fn boiling_point_at_foggs_pressure() {
        let c = boiling_point_k(1555.0) - 273.0;
        assert!((c - 112.58).abs() < 0.05, "{c}");
    }

    #[test]
    fn water_boils_at_373_under_one_bar() {
        assert!((boiling_point_k(1000.0) - 373.0).abs() < 1e-9);
    }

    /// Earth keeps 1000 mb in the air and about three quarters ocean (F Eq. 17
    /// normalises to it).
    #[test]
    fn earth_is_the_normal_case() {
        let v = volatile_inventory(1.0, 1.0, Zone::One, false, 1.0);
        assert_eq!(v, 1000.0);
        assert_eq!(surface_pressure_mb(v, 1.0, 1.0), 1000.0);
        assert!((ocean_fraction(v, 1.0) - 0.75).abs() < 1e-12);
    }

    /// Venus keeps its volatiles in the air: about 90 Earth atmospheres (F p.507).
    #[test]
    fn a_runaway_keeps_its_volatiles_aloft() {
        let v = volatile_inventory(0.82, 1.0, Zone::One, true, 1.0);
        let p = surface_pressure_mb(v, 0.95, 0.9);
        assert!((70_000.0..100_000.0).contains(&p), "{p}");
    }

    /// Fig. 11: at 1.8 m⊕ and 7568 km, Fogg prints 1555 mb and 90% ocean. The
    /// relations give those within his ±20%.
    #[test]
    fn foggs_planet_three_is_within_his_scatter() {
        let r = 7568.0 / 6378.1;
        let v = volatile_inventory(1.8, 1.0, Zone::One, false, 1.0);
        let p = surface_pressure_mb(v, r, 1.27);
        assert!((p / 1555.0 - 1.0).abs() < 0.2, "{p}");
        let o = ocean_fraction(v, r);
        assert!((o / 0.90 - 1.0).abs() < 0.2, "{o}");
    }

    /// Fig. 11: 20.12 °C gives 3.1% ice.
    #[test]
    fn ice_at_foggs_temperature() {
        let f = ice_fraction(293.12, 1.0);
        assert!((f - 0.031).abs() < 0.001, "{f}");
    }

    #[test]
    fn ice_is_limited_by_the_ocean() {
        assert_eq!(ice_fraction(100.0, 0.1), 0.15000000000000002);
        assert_eq!(ice_fraction(330.0, 1.0), 0.0);
    }

    /// Hart's Table I sums to the observed 0.30 with his present fractions.
    #[test]
    fn hart_albedo_table_gives_earth() {
        let sum: f64 = 0.472 * 0.52 + 0.021 * 0.15 + 0.353 * 0.04 + 0.032 * 0.70 + 0.122 * 0.13;
        assert!((sum - 0.30).abs() < 0.005, "{sum}");
    }

    #[test]
    fn earth_clouds_and_temperature() {
        assert!((cloud_fraction(288.0) - 0.472).abs() < 1e-12);
        let t = effective_temperature_k(1.0, 1.0, 0.3);
        assert!((t - 255.0).abs() < 1e-9);
        assert!((greenhouse_k(255.0, 1013.0) - 33.0).abs() < 0.05);
    }

    #[test]
    fn zones_follow_luminosity() {
        assert_eq!(Zone::of(3.9, 1.0), Zone::One);
        assert_eq!(Zone::of(5.0, 1.0), Zone::Two);
        assert_eq!(Zone::of(20.0, 1.0), Zone::Three);
        assert_eq!(Zone::of(7.0, 4.0), Zone::One);
    }

    /// Earth holds nitrogen for good but not hydrogen (F p.507).
    #[test]
    fn earth_keeps_nitrogen_and_loses_hydrogen() {
        assert!(retains(11.19, exosphere_k(1.0), 28.0));
        assert!(!retains(11.19, exosphere_k(1.0), 2.0));
    }

    #[test]
    fn runaway_limit_scales() {
        assert!((runaway_limit_au(1.0) - 0.93).abs() < 1e-12);
    }
}
