//! The relations of the model: the cloud, the reach of a nucleus, critical mass
//! and the density of what it accretes.

use sg_core::math;

/// The parameters of a run. Field names give Dole's symbols.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AccretionParams {
    /// `A`: central dust density around 1 M☉, in M☉ AU⁻³ (D p.14).
    pub central_density_msun_au3: f64,
    /// `K`: gas/dust ratio (D p.14).
    pub gas_dust_ratio: f64,
    /// `W`: the eccentricity of all cloud particles (D p.15).
    pub cloud_eccentricity: f64,
    /// `α` in the density law (D p.14).
    pub alpha: f64,
    /// `n` in the density law (D p.14).
    pub n: f64,
    /// `B` in the critical mass, in M☉ AU^(3/4) (D p.21).
    pub critical_mass_coeff: f64,
    /// `m₀`: the injected mass, in M☉ (D p.22).
    pub nucleus_mass_msun: f64,
    /// `Q` in the eccentricity draw (D p.15).
    pub eccentricity_exponent: f64,
    /// Relative mass change below which growth stops (D p.21).
    pub growth_tolerance: f64,
    /// Nuclei to inject at most: a step budget, so a run always ends.
    pub max_nuclei: u32,
}

impl AccretionParams {
    /// Dole's Set 3 (D Table 2).
    pub const fn set3() -> Self {
        Self {
            central_density_msun_au3: 0.0015,
            gas_dust_ratio: 50.0,
            cloud_eccentricity: 0.20,
            alpha: 5.0,
            n: 3.0,
            critical_mass_coeff: 1.2e-5,
            nucleus_mass_msun: 1e-15,
            eccentricity_exponent: 0.077,
            growth_tolerance: 1e-4,
            max_nuclei: 100_000,
        }
    }

    /// Dole's Set 4 (D Table 2), the default (gap 8).
    pub const fn set4() -> Self {
        Self {
            cloud_eccentricity: 0.25,
            ..Self::set3()
        }
    }
}

impl Default for AccretionParams {
    fn default() -> Self {
        Self::set4()
    }
}

/// Dust density `A √(M/M☉) exp(−α r^(1/n))`, in M☉ AU⁻³.
///
/// Dole (1969) p.14 for 1 M☉; Fogg (1985) Eq. 8 for the scaling with stellar
/// mass, anchored at Dole's `A`. Valid wherever the cloud extends; Fogg's
/// scaling is for 0.6–1.3 M☉.
pub fn dust_density_msun_au3(p: &AccretionParams, star_mass_msun: f64, r_au: f64) -> f64 {
    p.central_density_msun_au3
        * math::sqrt(star_mass_msun)
        * math::exp(-p.alpha * math::pow(r_au, 1.0 / p.n))
}

/// Critical mass `B (r_p (L☉/L)^(1/2))^(−3/4)`, in M☉: above it a body gathers
/// gas. Fogg (1985) Eq. 7; Dole (1969) p.21 for `L = L☉`.
pub fn critical_mass_msun(p: &AccretionParams, perihelion_au: f64, luminosity_lsun: f64) -> f64 {
    p.critical_mass_coeff * math::pow(perihelion_au / math::sqrt(luminosity_lsun), -0.75)
}

/// What a body on a given orbit reaches at a given mass. Dole (1969) pp.17–20
/// and Fig. 6, with `μ` taken relative to the star.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reach {
    /// `r_p`, AU.
    pub perihelion_au: f64,
    /// `r_a`, AU.
    pub aphelion_au: f64,
    /// `x_p`, AU.
    pub x_perihelion_au: f64,
    /// `x_a`, AU.
    pub x_aphelion_au: f64,
}

impl Reach {
    /// Computes `r_p`, `r_a` and `x = r μ^(1/4)` with `μ = m/(M + m)`.
    pub fn new(a_au: f64, e: f64, mass_msun: f64, star_mass_msun: f64) -> Self {
        let mu = mass_msun / (star_mass_msun + mass_msun);
        let q = math::pow(mu, 0.25);
        let perihelion_au = a_au * (1.0 - e);
        let aphelion_au = a_au * (1.0 + e);
        Self {
            perihelion_au,
            aphelion_au,
            x_perihelion_au: perihelion_au * q,
            x_aphelion_au: aphelion_au * q,
        }
    }

    /// Inner limit of the swept band, `(r_p − x_p)/(1 + W)`.
    pub fn swept_inner_au(&self, w: f64) -> f64 {
        (self.perihelion_au - self.x_perihelion_au) / (1.0 + w)
    }

    /// Outer limit of the swept band, `(r_a + x_a)/(1 − W)`.
    pub fn swept_outer_au(&self, w: f64) -> f64 {
        (self.aphelion_au + self.x_aphelion_au) / (1.0 - w)
    }

    /// Cross-section area factor of the swept region, `2π a (x_a + x_p)`, in
    /// AU²: multiplied by a width and a density it gives a mass (D p.20).
    pub fn area_au2(&self, a_au: f64) -> f64 {
        2.0 * core::f64::consts::PI * a_au * (self.x_aphelion_au + self.x_perihelion_au)
    }
}

/// Density of gas gathered above the critical mass, in M☉ AU⁻³: Dole's (1969,
/// p.21) effective density `Kρ₁ / (1 + (m_c/m)^(1/2) (K − 1))` less the dust
/// `ρ₁`, since dust and gas are tracked apart here. Zero at or below `m_c`.
pub fn gas_density_msun_au3(rho1: f64, gas_dust_ratio: f64, mass_msun: f64, crit_msun: f64) -> f64 {
    if mass_msun <= crit_msun {
        return 0.0;
    }
    let k = gas_dust_ratio;
    k * rho1 / (1.0 + math::sqrt(crit_msun / mass_msun) * (k - 1.0)) - rho1
}

#[cfg(test)]
mod tests {
    use super::*;

    const EARTH_MSUN: f64 = 3e-6; // Dole's value, p.21

    /// `M = 4πKAnΓ(3n)/α^(3n)`, with `Γ(9) = 8!`: 0.0584 M☉ (D p.14).
    #[test]
    fn cloud_mass_is_doles() {
        let p = AccretionParams::set3();
        let gamma = 40320.0;
        let m = 4.0
            * core::f64::consts::PI
            * p.gas_dust_ratio
            * p.central_density_msun_au3
            * p.n
            * gamma
            / math::pow(p.alpha, 3.0 * p.n);
        assert!((m - 0.0584).abs() < 5e-5, "{m}");
    }

    /// `r³ρ₁` peaks at `(3n/α)^n` = 5.83 AU (D p.14).
    #[test]
    fn dust_peak_is_doles() {
        let p = AccretionParams::set3();
        let f = |r: f64| r * r * r * dust_density_msun_au3(&p, 1.0, r);
        let peak = 5.832;
        assert!(f(peak) > f(peak - 0.01) && f(peak) > f(peak + 0.01));
    }

    /// About four Earth masses at 1 AU (D p.21).
    #[test]
    fn critical_mass_at_one_au() {
        let p = AccretionParams::set3();
        let m = critical_mass_msun(&p, 1.0, 1.0) / EARTH_MSUN;
        assert!((m - 4.0).abs() < 0.01, "{m}");
    }

    /// A brighter star pushes the same `m_c` out as `L^(1/2)` (F Eq. 7).
    #[test]
    fn critical_mass_scales_with_luminosity() {
        let p = AccretionParams::set3();
        let a = critical_mass_msun(&p, 2.0, 4.0);
        let b = critical_mass_msun(&p, 1.0, 1.0);
        assert!((a - b).abs() < 1e-15);
    }

    /// `A ∝ (M/M☉)^(1/2)` (F Eq. 8).
    #[test]
    fn density_scales_with_stellar_mass() {
        let p = AccretionParams::set3();
        let ratio = dust_density_msun_au3(&p, 4.0, 1.0) / dust_density_msun_au3(&p, 1.0, 1.0);
        assert!((ratio - 2.0).abs() < 1e-12);
    }

    /// The limits span Dole's band width (D p.20).
    #[test]
    fn limits_span_the_band_width() {
        let (a, e, w) = (3.0, 0.1, 0.25);
        let r = Reach::new(a, e, 1e-4, 1.0);
        let (xa, xp, ra, rp) = (
            r.x_aphelion_au,
            r.x_perihelion_au,
            r.aphelion_au,
            r.perihelion_au,
        );
        let width = 2.0 * a * e + xa + xp + w * (ra + xa) / (1.0 - w) + w * (rp - xp) / (1.0 + w);
        assert!((r.swept_outer_au(w) - r.swept_inner_au(w) - width).abs() < 1e-12);
    }

    /// For large masses the effective density approaches `Kρ₁` (D p.21).
    #[test]
    fn gas_density_limits() {
        assert_eq!(gas_density_msun_au3(1.0, 50.0, 1.0, 2.0), 0.0);
        let far = gas_density_msun_au3(1.0, 50.0, 1e12, 1.0);
        assert!(far > 48.0 && far < 49.0, "{far}");
    }
}
