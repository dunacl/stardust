//! Protoplanetary gas and dust disk (PLAN.md §5.2).
//!
//! A deliberately simple, analytic disk that planet formation can be run in
//! thousands of times a second:
//!
//! - **Gas**: the self-similar solution of a viscously spreading disk whose
//!   viscosity grows linearly with radius (Lynden-Bell & Pringle 1974;
//!   Hartmann et al. 1998, ApJ 495, 385, eqs. 16–20), with the
//!   Shakura & Sunyaev (1973) α viscosity.
//! - **Temperature**: the warmer of viscous heating and stellar irradiation,
//!   from the fits of Ida, Guillot & Morbidelli (2016, A&A 591, A72,
//!   eqs. 9–10).
//! - **Dispersal**: the gas is removed at a given time, over a short
//!   clearing interval. The time is an input (drawn by the caller from the
//!   observed decay of disk frequency with age); the photoevaporative wind
//!   that causes it is not modelled.
//!
//! Known inconsistency, shared with most models of this kind: the surface
//! density assumes the viscosity law of a disk with `T ∝ r^-1/2`, while the
//! temperature used for everything else follows the fits above.
//!
//! **Units** inside this crate and `sg-formation`: AU, solar masses and
//! years, so that `G = 4π²`. Temperatures are in kelvin.

use sg_core::math::{exp, pow, sqrt};

/// `G` in AU³ M☉⁻¹ yr⁻² (Kepler's third law for the Earth's orbit).
pub const G: f64 = 4.0 * core::f64::consts::PI * core::f64::consts::PI;

/// Isothermal sound speed at 1 K in AU/yr: `sqrt(k_B/(μ m_H))` with mean
/// molecular weight μ = 2.34 (molecular hydrogen and helium), which is
/// 5939 cm/s, divided by 1 AU/yr = 4.7405×10⁵ cm/s.
pub const SOUND_SPEED_AT_1K_AU_YR: f64 = 5939.0 / 4.7405e5;

/// Temperature at which water ice sublimates in the disk midplane, K
/// (Hayashi 1981; the value commonly adopted, e.g. Ida et al. 2016).
pub const WATER_ICE_TEMPERATURE_K: f64 = 170.0;

/// Fraction of the dispersal time over which the gas is cleared once
/// dispersal begins. Observed "transition disks" are about 10% of all
/// disks, which suggests clearing takes about that share of a lifetime
/// (e.g. Owen, Ercolano & Clarke 2011, MNRAS 412, 13).
const CLEARING_FRACTION: f64 = 0.1;

/// What defines a disk.
#[derive(Debug, Clone, PartialEq)]
pub struct DiskSpec {
    /// Mass of the star (or stars) at the centre, M☉.
    pub star_mass_msun: f64,
    /// Their luminosity, L☉.
    pub star_luminosity_lsun: f64,
    /// Initial gas mass, M☉.
    pub initial_mass_msun: f64,
    /// Characteristic radius `R₁`, beyond which the surface density falls
    /// off exponentially at the start, AU.
    pub characteristic_radius_au: f64,
    /// Shakura–Sunyaev α for the disk's global evolution.
    pub alpha: f64,
    /// When the gas starts to clear, years.
    pub dispersal_time_yr: f64,
    /// Dust-to-gas mass ratio at the start.
    pub dust_to_gas: f64,
    /// Inner edge of the gas, AU.
    pub inner_edge_au: f64,
    /// Outer edge, AU: a companion star's truncation, or simply far out.
    pub outer_edge_au: f64,
}

/// A disk that can be asked for its state at any radius and time.
#[derive(Debug, Clone, PartialEq)]
pub struct Disk {
    spec: DiskSpec,
    /// Viscous time at `R₁`, `R₁²/(3ν₁)`, years.
    viscous_time_yr: f64,
}

/// Why a disk could not be built.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DiskError {
    #[error("disk parameter `{name}` must be positive and finite, got {value}")]
    Invalid { name: &'static str, value: f64 },
    #[error("the disk's inner edge ({inner} AU) is not inside its outer edge ({outer} AU)")]
    NoRoom { inner: f64, outer: f64 },
}

/// Keplerian angular frequency, rad/yr.
pub fn omega(star_mass_msun: f64, r_au: f64) -> f64 {
    sqrt(G * star_mass_msun / (r_au * r_au * r_au))
}

/// Midplane temperature where stellar irradiation dominates, K.
///
/// Ida, Guillot & Morbidelli (2016), eq. 10:
/// `T = 150 K (L/L☉)^(2/7) (M/M☉)^(-1/7) (r/AU)^(-3/7)`. A fit to an
/// optically thick, flared disk; valid outside the viscously heated region.
pub fn irradiation_temperature(luminosity_lsun: f64, star_mass_msun: f64, r_au: f64) -> f64 {
    150.0
        * pow(luminosity_lsun, 2.0 / 7.0)
        * pow(star_mass_msun, -1.0 / 7.0)
        * pow(r_au, -3.0 / 7.0)
}

/// Midplane temperature where viscous heating dominates, K.
///
/// Ida, Guillot & Morbidelli (2016), eq. 9:
/// `T = 200 K (M/M☉)^(3/10) (α/10⁻³)^(-1/5) (Ṁ/10⁻⁸ M☉/yr)^(2/5) (r/AU)^(-9/10)`.
/// A fit to an optically thick disk heated at the midplane; valid in the
/// inner few AU while the disk is accreting strongly.
pub fn viscous_temperature(
    star_mass_msun: f64,
    alpha: f64,
    accretion_rate_msun_yr: f64,
    r_au: f64,
) -> f64 {
    200.0
        * pow(star_mass_msun, 0.3)
        * pow(alpha / 1.0e-3, -0.2)
        * pow(accretion_rate_msun_yr / 1.0e-8, 0.4)
        * pow(r_au, -0.9)
}

/// Shape of the self-similar gas profile: surface density at `r/R₁` and
/// dimensionless time `T = 1 + t/t_s`, relative to `M₀/(2π R₁²)`
/// (Hartmann et al. 1998, eq. 20 for γ = 1): `(r/R₁)⁻¹ · T^(-3/2) ·
/// exp(−(r/R₁)/T)`. For anything that needs the profile without the whole
/// [`Disk`], such as drawing it; valid wherever the solution is, `r > 0`
/// and `T ≥ 1`.
pub fn relative_surface_density(r_over_r1: f64, big_t: f64) -> f64 {
    pow(big_t, -1.5) * exp(-r_over_r1 / big_t) / r_over_r1
}

/// Fraction of the gas still present at `t_yr` in a disk whose dispersal
/// starts at `dispersal_time_yr`: 1 until then, falling linearly to 0 over
/// the clearing interval (PLAN.md D36: dispersal is imposed, not modelled).
pub fn gas_fraction(t_yr: f64, dispersal_time_yr: f64) -> f64 {
    (1.0 - (t_yr - dispersal_time_yr) / (CLEARING_FRACTION * dispersal_time_yr)).clamp(0.0, 1.0)
}

impl Disk {
    pub fn new(spec: DiskSpec) -> Result<Self, DiskError> {
        let positive = [
            ("star_mass_msun", spec.star_mass_msun),
            ("star_luminosity_lsun", spec.star_luminosity_lsun),
            ("initial_mass_msun", spec.initial_mass_msun),
            ("characteristic_radius_au", spec.characteristic_radius_au),
            ("alpha", spec.alpha),
            ("dispersal_time_yr", spec.dispersal_time_yr),
            ("dust_to_gas", spec.dust_to_gas),
            ("inner_edge_au", spec.inner_edge_au),
            ("outer_edge_au", spec.outer_edge_au),
        ];
        for (name, value) in positive {
            if !(value.is_finite() && value > 0.0) {
                return Err(DiskError::Invalid { name, value });
            }
        }
        if spec.inner_edge_au >= spec.outer_edge_au {
            return Err(DiskError::NoRoom {
                inner: spec.inner_edge_au,
                outer: spec.outer_edge_au,
            });
        }
        // ν = α c_s²/Ω at R₁, with the irradiation temperature there: the
        // outer disk, which sets the evolution, is irradiation-heated.
        let r1 = spec.characteristic_radius_au;
        let t1 = irradiation_temperature(spec.star_luminosity_lsun, spec.star_mass_msun, r1);
        let cs = SOUND_SPEED_AT_1K_AU_YR * sqrt(t1);
        let nu1 = spec.alpha * cs * cs / omega(spec.star_mass_msun, r1);
        let viscous_time_yr = r1 * r1 / (3.0 * nu1);
        Ok(Self {
            spec,
            viscous_time_yr,
        })
    }

    pub fn spec(&self) -> &DiskSpec {
        &self.spec
    }

    /// Viscous time at the characteristic radius, years.
    pub fn viscous_time_yr(&self) -> f64 {
        self.viscous_time_yr
    }

    /// Dimensionless time `T = 1 + t/t_s` of the self-similar solution.
    fn big_t(&self, t_yr: f64) -> f64 {
        1.0 + t_yr.max(0.0) / self.viscous_time_yr
    }

    /// Fraction of the gas still present: 1 until dispersal starts, then
    /// falling linearly to 0 over the clearing interval.
    pub fn gas_fraction(&self, t_yr: f64) -> f64 {
        let start = self.spec.dispersal_time_yr;
        (1.0 - (t_yr - start) / (CLEARING_FRACTION * start)).clamp(0.0, 1.0)
    }

    /// When the last gas is gone, years.
    pub fn gas_gone_yr(&self) -> f64 {
        self.spec.dispersal_time_yr * (1.0 + CLEARING_FRACTION)
    }

    /// Rate at which gas flows through the inner disk onto the star, M☉/yr
    /// (Hartmann et al. 1998, eq. 35 for γ = 1):
    /// `Ṁ = M₀/(2 t_s) · T^(-3/2)`.
    pub fn accretion_rate(&self, t_yr: f64) -> f64 {
        self.spec.initial_mass_msun / (2.0 * self.viscous_time_yr)
            * pow(self.big_t(t_yr), -1.5)
            * self.gas_fraction(t_yr)
    }

    /// Gas mass left in the disk, M☉: `M₀ T^(-1/2)`.
    pub fn gas_mass(&self, t_yr: f64) -> f64 {
        self.spec.initial_mass_msun * pow(self.big_t(t_yr), -0.5) * self.gas_fraction(t_yr)
    }

    /// Gas surface density, M☉/AU² (Hartmann et al. 1998, eq. 20 for γ = 1):
    /// `Σ = M₀/(2π R₁²) · (r/R₁)⁻¹ · T^(-3/2) · exp(−(r/R₁)/T)`. Zero outside
    /// the disk's edges.
    pub fn gas_surface_density(&self, r_au: f64, t_yr: f64) -> f64 {
        if r_au < self.spec.inner_edge_au || r_au > self.spec.outer_edge_au {
            return 0.0;
        }
        let r1 = self.spec.characteristic_radius_au;
        let big_t = self.big_t(t_yr);
        let x = r_au / r1;
        self.spec.initial_mass_msun / (2.0 * core::f64::consts::PI * r1 * r1) / x
            * pow(big_t, -1.5)
            * exp(-x / big_t)
            * self.gas_fraction(t_yr)
    }

    /// `−d ln Σ / d ln r` of the gas: `1 + (r/R₁)/T`.
    pub fn surface_density_slope(&self, r_au: f64, t_yr: f64) -> f64 {
        1.0 + r_au / self.spec.characteristic_radius_au / self.big_t(t_yr)
    }

    fn t_viscous(&self, r_au: f64, t_yr: f64) -> f64 {
        viscous_temperature(
            self.spec.star_mass_msun,
            self.spec.alpha,
            self.accretion_rate(t_yr),
            r_au,
        )
    }

    fn t_irradiation(&self, r_au: f64) -> f64 {
        irradiation_temperature(
            self.spec.star_luminosity_lsun,
            self.spec.star_mass_msun,
            r_au,
        )
    }

    /// Midplane temperature, K: the larger of the viscous and irradiation
    /// temperatures (Ida et al. 2016, §2.1).
    pub fn temperature(&self, r_au: f64, t_yr: f64) -> f64 {
        self.t_viscous(r_au, t_yr).max(self.t_irradiation(r_au))
    }

    /// `−d ln T / d ln r`: 9/10 where viscous heating dominates, 3/7
    /// elsewhere.
    pub fn temperature_slope(&self, r_au: f64, t_yr: f64) -> f64 {
        if self.t_viscous(r_au, t_yr) > self.t_irradiation(r_au) {
            0.9
        } else {
            3.0 / 7.0
        }
    }

    /// Isothermal sound speed, AU/yr.
    pub fn sound_speed(&self, r_au: f64, t_yr: f64) -> f64 {
        SOUND_SPEED_AT_1K_AU_YR * sqrt(self.temperature(r_au, t_yr))
    }

    /// Keplerian angular frequency, rad/yr.
    pub fn omega(&self, r_au: f64) -> f64 {
        omega(self.spec.star_mass_msun, r_au)
    }

    /// Aspect ratio `h = H/r = c_s/(r Ω)`.
    pub fn aspect_ratio(&self, r_au: f64, t_yr: f64) -> f64 {
        self.sound_speed(r_au, t_yr) / (r_au * self.omega(r_au))
    }

    /// `d ln P / d ln r` of the midplane pressure. With `P ∝ Σ T^(1/2) r^(-3/2)`
    /// (midplane density `Σ/(√(2π) H)` times `c_s²`) this is
    /// `−β_Σ − β_T/2 − 3/2`.
    pub fn pressure_slope(&self, r_au: f64, t_yr: f64) -> f64 {
        -self.surface_density_slope(r_au, t_yr) - 0.5 * self.temperature_slope(r_au, t_yr) - 1.5
    }

    /// Fraction by which the gas orbits slower than Keplerian,
    /// `η = −½ h² d ln P/d ln r` (Nakagawa, Sekiya & Hayashi 1986).
    pub fn eta(&self, r_au: f64, t_yr: f64) -> f64 {
        let h = self.aspect_ratio(r_au, t_yr);
        -0.5 * h * h * self.pressure_slope(r_au, t_yr)
    }

    /// Radius of the water ice line, AU: where the midplane is at
    /// [`WATER_ICE_TEMPERATURE_K`]. It moves inward as accretion wanes.
    /// Found by bisection between the disk's edges; returns an edge if the
    /// whole disk is on one side.
    pub fn water_ice_line(&self, t_yr: f64) -> f64 {
        let (mut lo, mut hi) = (self.spec.inner_edge_au, self.spec.outer_edge_au);
        if self.temperature(lo, t_yr) <= WATER_ICE_TEMPERATURE_K {
            return lo;
        }
        if self.temperature(hi, t_yr) >= WATER_ICE_TEMPERATURE_K {
            return hi;
        }
        // Temperature falls monotonically outward, so bisection in log r.
        for _ in 0..60 {
            let mid = sqrt(lo * hi);
            if self.temperature(mid, t_yr) > WATER_ICE_TEMPERATURE_K {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        sqrt(lo * hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn spec() -> DiskSpec {
        DiskSpec {
            star_mass_msun: 1.0,
            star_luminosity_lsun: 1.0,
            initial_mass_msun: 0.05,
            characteristic_radius_au: 40.0,
            alpha: 5.0e-3,
            dispersal_time_yr: 3.0e6,
            dust_to_gas: 0.0149,
            inner_edge_au: 0.05,
            outer_edge_au: 1000.0,
        }
    }

    fn disk() -> Disk {
        Disk::new(spec()).unwrap()
    }

    /// The stand-alone profile and clearing functions are the disk's own.
    #[test]
    fn free_functions_agree_with_the_disk() {
        let disk = disk();
        let spec = disk.spec();
        let r1 = spec.characteristic_radius_au;
        let scale = spec.initial_mass_msun / (2.0 * core::f64::consts::PI * r1 * r1);
        for t in [0.0, 5.0e5, 2.9e6, 3.1e6, 3.2e6] {
            let big_t = 1.0 + t / disk.viscous_time_yr();
            assert_eq!(
                gas_fraction(t, spec.dispersal_time_yr),
                disk.gas_fraction(t)
            );
            for r in [0.1, 1.0, 40.0, 300.0] {
                let free = scale
                    * relative_surface_density(r / r1, big_t)
                    * gas_fraction(t, spec.dispersal_time_yr);
                let own = disk.gas_surface_density(r, t);
                assert!((free - own).abs() <= 1e-12 * own.abs(), "{r} {t}");
            }
        }
        // At R₁ and t = 0 the relative density is 1/e.
        assert!((relative_surface_density(1.0, 1.0) - (-1.0_f64).exp()).abs() < 1e-15);
    }

    /// With G = 4π², a body at 1 AU from a solar-mass star takes one year.
    #[test]
    fn units_give_a_one_year_orbit_at_one_au() {
        let period = 2.0 * core::f64::consts::PI / omega(1.0, 1.0);
        assert!((period - 1.0).abs() < 1e-12);
        // 280 K gas: 0.99 km/s, the familiar value for the solar nebula.
        let km_s = SOUND_SPEED_AT_1K_AU_YR * 280.0f64.sqrt() * 4.7405;
        assert!((km_s - 0.994).abs() < 0.01, "{km_s}");
    }

    /// Ida et al. (2016): 150 K and 200 K at 1 AU for the reference disk.
    #[test]
    fn temperatures_match_the_published_normalisations() {
        assert!((irradiation_temperature(1.0, 1.0, 1.0) - 150.0).abs() < 1e-9);
        assert!((viscous_temperature(1.0, 1.0e-3, 1.0e-8, 1.0) - 200.0).abs() < 1e-9);
        // 3/7 and 9/10 slopes.
        let ratio = irradiation_temperature(1.0, 1.0, 10.0) / 150.0;
        assert!((ratio - 10f64.powf(-3.0 / 7.0)).abs() < 1e-12);
        let ratio = viscous_temperature(1.0, 1.0e-3, 1.0e-8, 10.0) / 200.0;
        assert!((ratio - 10f64.powf(-0.9)).abs() < 1e-12);
    }

    /// The surface density integrates to the gas mass, at every time.
    #[test]
    fn surface_density_integrates_to_the_disk_mass() {
        let disk = disk();
        for t in [0.0, 3.0e5, 1.0e6, 2.5e6] {
            // ∫ 2π r Σ dr by the midpoint rule in log r.
            let (lo, hi, n) = (1.0e-3f64, 1.0e5f64, 20_000);
            let step = (hi / lo).ln() / f64::from(n);
            let wide = Disk::new(DiskSpec {
                inner_edge_au: lo,
                outer_edge_au: hi,
                ..spec()
            })
            .unwrap();
            let mut mass = 0.0;
            for k in 0..n {
                let r = lo * ((f64::from(k) + 0.5) * step).exp();
                mass += 2.0 * core::f64::consts::PI * r * r * wide.gas_surface_density(r, t) * step;
            }
            let expected = disk.gas_mass(t);
            assert!(
                (mass / expected - 1.0).abs() < 1e-3,
                "t = {t}: {mass} vs {expected}"
            );
        }
    }

    /// Mass is conserved: what leaves the disk is what the star accretes.
    #[test]
    fn mass_lost_equals_mass_accreted() {
        let disk = disk();
        let (t0, t1, n) = (0.0, 2.0e6, 200_000);
        let dt = (t1 - t0) / f64::from(n);
        let accreted: f64 = (0..n)
            .map(|k| disk.accretion_rate(t0 + (f64::from(k) + 0.5) * dt) * dt)
            .sum();
        let lost = disk.gas_mass(t0) - disk.gas_mass(t1);
        assert!((accreted / lost - 1.0).abs() < 1e-6, "{accreted} vs {lost}");
    }

    /// A typical disk: accretion of order 10⁻⁸ M☉/yr at 1 Myr, a few hundred
    /// g/cm² at 5 AU, an aspect ratio of a few percent, and the ice line
    /// moving in from several AU to about 1 AU.
    #[test]
    fn reference_disk_is_plausible() {
        let disk = disk();
        let mdot = disk.accretion_rate(1.0e6);
        assert!((1.0e-9..1.0e-7).contains(&mdot), "{mdot}");
        // M☉/AU² to g/cm²: 1.989e33 g / (1.496e13 cm)².
        let g_cm2 = disk.gas_surface_density(5.0, 1.0e6) * 1.989e33 / (1.496e13f64 * 1.496e13);
        assert!((10.0..1000.0).contains(&g_cm2), "{g_cm2}");
        let h = disk.aspect_ratio(5.0, 1.0e6);
        assert!((0.03..0.07).contains(&h), "{h}");
        let (early, late) = (disk.water_ice_line(1.0e5), disk.water_ice_line(2.5e6));
        assert!(early > late, "{early} {late}");
        assert!(
            (0.5..10.0).contains(&early) && (0.4..3.0).contains(&late),
            "{early} {late}"
        );
        assert!((disk.temperature(disk.water_ice_line(1.0e6), 1.0e6) - 170.0).abs() < 0.01);
        // Gas orbits a little slower than Keplerian: η of a few 10⁻³.
        let eta = disk.eta(5.0, 1.0e6);
        assert!((1.0e-3..1.0e-2).contains(&eta), "{eta}");
    }

    #[test]
    fn gas_clears_after_the_dispersal_time() {
        let disk = disk();
        assert_eq!(disk.gas_fraction(2.9e6), 1.0);
        assert!((disk.gas_fraction(3.15e6) - 0.5).abs() < 1e-9);
        assert_eq!(disk.gas_fraction(3.4e6), 0.0);
        assert!((disk.gas_gone_yr() - 3.3e6).abs() < 1e-3);
        assert_eq!(disk.gas_surface_density(5.0, 4.0e6), 0.0);
        assert_eq!(disk.accretion_rate(4.0e6), 0.0);
        // Nothing outside the edges.
        assert_eq!(disk.gas_surface_density(0.01, 1.0e6), 0.0);
        assert_eq!(disk.gas_surface_density(2000.0, 1.0e6), 0.0);
    }

    #[test]
    fn invalid_disks_are_rejected() {
        let bad = DiskSpec {
            alpha: 0.0,
            ..spec()
        };
        assert!(matches!(
            Disk::new(bad),
            Err(DiskError::Invalid { name: "alpha", .. })
        ));
        let no_room = DiskSpec {
            inner_edge_au: 5.0,
            outer_edge_au: 2.0,
            ..spec()
        };
        assert!(matches!(Disk::new(no_room), Err(DiskError::NoRoom { .. })));
    }

    proptest! {
        /// The disk drains monotonically and cools outward.
        #[test]
        fn disk_evolves_monotonically(
            mass in 0.2f64..2.0, m0 in 0.005f64..0.2, r1 in 10.0f64..150.0,
            alpha in 1.0e-4f64..2.0e-2, r in 0.1f64..50.0, t in 0.0f64..2.0e6,
        ) {
            let disk = Disk::new(DiskSpec {
                star_mass_msun: mass,
                star_luminosity_lsun: mass.powf(3.5),
                initial_mass_msun: m0,
                characteristic_radius_au: r1,
                alpha,
                ..spec()
            }).unwrap();
            prop_assert!(disk.gas_mass(t + 1.0e5) < disk.gas_mass(t));
            prop_assert!(disk.accretion_rate(t + 1.0e5) < disk.accretion_rate(t));
            prop_assert!(disk.temperature(r * 1.5, t) < disk.temperature(r, t));
            prop_assert!(disk.pressure_slope(r, t) < 0.0 && disk.eta(r, t) > 0.0);
            let sigma = disk.gas_surface_density(r, t);
            prop_assert!(sigma.is_finite() && sigma > 0.0);
        }
    }
}
