//! Main-sequence stellar evolution: luminosity and radius as functions of
//! mass, metallicity and age (PLAN.md §5.1).
//!
//! An implementation of published analytic fits to detailed stellar models:
//!
//! - zero-age main sequence: Tout, Pols, Eggleton & Han (1996), MNRAS 281,
//!   257, eqs. 1 and 2;
//! - main-sequence lifetime and evolution: Hurley, Pols & Tout (2000), MNRAS
//!   315, 543, §5.1, eqs. 4–24 (hereafter HPT).
//!
//! **Validity.** Masses 0.08–100 M☉ (the fits were made to models of 0.5–50
//! M☉ and extrapolate acceptably over the wider range, HPT §5); metallicity
//! Z from 10⁻⁴ to 0.03. Solar metallicity is Z = 0.02 in these papers. Only
//! the main sequence is modelled: nothing here applies after hydrogen is
//! exhausted in the core. The fits reproduce the detailed models to about 5%
//! in luminosity and radius (HPT §5.1). With Z = 0.02 a 1 M☉ star of the
//! Sun's age comes out at 0.96 L☉ and 0.98 R☉.
//!
//! The coefficients are numbered as in the authors' SSE package rather than
//! as in the papers, so the two can be compared entry by entry, and the
//! results are tested against that package (`tests/sse_reference.rs`).

mod tables;

use crate::{effective_temperature, StellarError};
use sg_core::math::{abs, log10, pow, sqrt};
use sg_core::units::{Kelvin, SolarLuminosity, SolarMass, SolarRadius};
use tables::{XL, XR, XT, XZ};

/// Metallicity of the Sun in the papers' convention.
pub const SOLAR_Z: f64 = 0.02;
/// Range of metallicity the fits cover.
pub const MIN_Z: f64 = 1.0e-4;
pub const MAX_Z: f64 = 0.03;
/// Range of mass accepted.
pub const MIN_MASS_MSUN: f64 = 0.08;
pub const MAX_MASS_MSUN: f64 = 100.0;

/// Metal mass fraction Z for an iron abundance [Fe/H] in dex, assuming
/// metals scale with iron: `Z = 0.02·10^[Fe/H]`.
pub fn z_from_feh(feh: f64) -> f64 {
    SOLAR_Z * pow(10.0, feh)
}

/// `c[0] + x·(c[1] + x·(c[2] + …))`.
fn poly(c: &[f64], x: f64) -> f64 {
    c.iter().rev().fold(0.0, |acc, k| acc * x + k)
}

/// Entries `first..first+n` of a table, numbered from 1 as in the source.
fn run(table: &'static [f64], first: usize, n: usize) -> &'static [f64] {
    &table[first - 1..first - 1 + n]
}

/// A star's state on the main sequence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MsState {
    pub luminosity: SolarLuminosity,
    pub radius: SolarRadius,
    pub teff: Kelvin,
    /// Age as a fraction of the main-sequence lifetime, 0 to 1.
    pub fraction: f64,
}

/// The fits for one metallicity.
#[derive(Debug, Clone, PartialEq)]
pub struct MainSequence {
    z: f64,
    /// Coefficients, indexed from 1; entry 0 is unused.
    a: [f64; 98],
    /// Mass above which the track has a hook at the end of the main
    /// sequence (HPT eq. 1).
    m_hook: f64,
    /// Main-sequence lifetime as a fraction of the time to the base of the
    /// giant branch, when there is no hook (HPT eq. 6).
    x_ms: f64,
    /// Hydrogen mass fraction, `0.76 − 3Z`.
    hydrogen_x: f64,
}

impl MainSequence {
    /// The fits at solar metallicity.
    pub fn solar() -> Self {
        Self::build(SOLAR_Z)
    }

    /// The fits at metallicity `z` (mass fraction of metals).
    pub fn new(z: f64) -> Result<Self, StellarError> {
        if !(MIN_Z..=MAX_Z).contains(&z) {
            return Err(StellarError::OutOfRange {
                quantity: "metallicity Z",
                value: z,
                min: MIN_Z,
                max: MAX_Z,
            });
        }
        Ok(Self::build(z))
    }

    pub fn z(&self) -> f64 {
        self.z
    }

    /// Evaluates the metallicity dependence of every coefficient
    /// (Tout et al. 1996 eq. 3; HPT appendix A).
    // The limits are written as nested min/max, as published.
    #[allow(clippy::too_many_lines, clippy::manual_clamp)]
    fn build(z: f64) -> Self {
        let lzs = log10(z / SOLAR_Z);
        let lz = log10(z);
        let mut a = [0.0f64; 98];

        // Zero-age luminosity (1–7) and radius (8–16).
        for k in 0..7 {
            a[1 + k] = poly(run(&XZ, 1 + 5 * k, 5), lzs);
        }
        for k in 0..5 {
            a[8 + k] = poly(run(&XZ, 36 + 5 * k, 5), lzs);
        }
        a[13] = XZ[60];
        for k in 0..3 {
            a[14 + k] = poly(run(&XZ, 62 + 5 * k, 5), lzs);
        }

        // Time to the base of the giant branch (17–21) and to the hook (22–26).
        for k in 0..4 {
            a[17 + k] = poly(run(&XT, 1 + 4 * k, 4), lzs);
        }
        a[21] = XT[16];
        a[22] = poly(run(&XT, 18, 4), lzs);
        a[23] = XT[21];
        a[24] = poly(run(&XT, 23, 4), lzs);
        a[25] = poly(run(&XT, 27, 4), lzs);
        a[26] = XT[30];

        // Luminosity at the end of the main sequence (27–32).
        a[27] = poly(run(&XL, 1, 5), lzs);
        a[28] = poly(run(&XL, 6, 5), lzs);
        a[29] = poly(run(&XL, 11, 4), lzs);
        a[30] = poly(run(&XL, 15, 5), lzs);
        a[27] *= a[30];
        a[28] *= a[30];
        a[31] = poly(run(&XL, 20, 4), lzs);
        a[32] = poly(run(&XL, 24, 4), lzs);

        // Luminosity α (33–42).
        for k in 0..4 {
            a[33 + k] = poly(run(&XL, 28 + 4 * k, 4), lzs);
        }
        a[37] = f64::max(0.9, 1.1064 + lzs * (0.415 + 0.18 * lzs));
        a[38] = f64::max(1.0, 1.19 + lzs * (0.377 + 0.176 * lzs));
        if z > 0.01 {
            a[37] = a[37].min(1.0);
            a[38] = a[38].min(1.1);
        }
        a[39] = f64::max(0.145, 0.0977 - lzs * (0.231 + 0.0753 * lzs));
        a[40] = f64::min(0.24 + lzs * (0.18 + 0.595 * lzs), 0.306 + 0.053 * lzs);
        a[41] = f64::min(0.33 + lzs * (0.132 + 0.218 * lzs), 0.3625 + 0.062 * lzs);
        a[42] = (a[33] + a[34] * pow(2.0, a[36])) / (pow(2.0, 0.4) + a[35] * pow(2.0, 1.9));

        // Luminosity β (43–46).
        a[43] = poly(run(&XL, 44, 5), lzs);
        a[44] = poly(run(&XL, 49, 5), lzs);
        a[45] = poly(run(&XL, 54, 3), lzs);
        a[46] = f64::min(1.4, 1.5135 + 0.3769 * lzs);
        a[46] = f64::max(0.6355 - 0.4192 * lzs, f64::max(1.25, a[46]));

        // Luminosity hook (47–51).
        for k in 0..4 {
            a[47 + k] = poly(run(&XL, 57 + 4 * k, 4), lzs);
        }
        a[51] = a[46];

        // Radius at the end of the main sequence (52–64).
        a[52] = poly(run(&XR, 1, 5), lzs);
        a[53] = poly(run(&XR, 6, 5), lzs);
        a[54] = poly(run(&XR, 11, 5), lzs);
        a[55] = poly(run(&XR, 16, 4), lzs);
        a[56] = poly(run(&XR, 20, 4), lzs);
        a[52] *= a[54];
        a[53] *= a[54];
        a[57] = XR[23];
        for k in 0..4 {
            a[58 + k] = poly(run(&XR, 25 + 4 * k, 4), lzs);
        }
        a[62] = f64::max(
            0.097 - 0.1072 * (lz + 3.0),
            f64::max(0.097, f64::min(0.1461, 0.1461 + 0.1237 * (lz + 2.0))),
        );
        a[62] = pow(10.0, a[62]);
        let m2 = a[62] + 0.1;
        a[63] = (a[52] + a[53] * pow(a[62], a[55])) / (a[54] + pow(a[62], a[56]));
        a[64] = (a[57] * pow(m2, 3.0) + a[58] * pow(m2, a[61]) + a[59] * pow(m2, a[61] + 1.5))
            / (a[60] + pow(m2, 5.0));

        // Radius α (65–76).
        for k in 0..4 {
            a[65 + k] = poly(run(&XR, 41 + 4 * k, 4), lzs);
        }
        a[69] = poly(run(&XR, 57, 5), lzs);
        a[70] = f64::max(0.9, f64::min(1.0, 1.116 + 0.166 * lzs));
        a[71] = f64::max(1.477 + 0.296 * lzs, f64::min(1.6, -0.308 - 1.046 * lzs));
        a[71] = f64::max(0.8, f64::min(0.8 - 2.0 * lzs, a[71]));
        a[72] = poly(run(&XR, 62, 3), lzs);
        a[73] = f64::max(0.065, 0.0843 - lzs * (0.0475 + 0.0352 * lzs));
        a[74] = 0.0736 + lzs * (0.0749 + 0.04426 * lzs);
        if z < 0.004 {
            a[74] = a[74].min(0.055);
        }
        a[75] = f64::max(0.091, f64::min(0.121, 0.136 + 0.0352 * lzs));
        a[76] = (a[65] * pow(a[71], a[67])) / (a[66] + pow(a[71], a[68]));
        if a[70] > a[71] {
            a[70] = a[71];
            a[75] = a[76];
        }

        // Radius β (77–82).
        for k in 0..4 {
            a[77 + k] = poly(run(&XR, 65 + 4 * k, 4), lzs);
        }
        a[81] = XR[80] + lzs * (XR[81] + lzs * lzs * XR[82]);
        if z > 0.01 {
            a[81] = a[81].max(0.95);
        }
        a[82] = f64::max(1.4, f64::min(1.6, 1.6 + lzs * (0.764 + 0.3322 * lzs)));

        // Radius γ (83–89).
        a[83] = f64::max(poly(run(&XR, 84, 4), lzs), poly(run(&XR, 96, 3), lzs));
        a[84] = f64::min(0.0, poly(run(&XR, 88, 4), lzs));
        a[84] = f64::max(a[84], poly(run(&XR, 99, 3), lzs));
        a[85] = poly(run(&XR, 92, 4), lzs);
        a[85] = f64::max(0.0, f64::min(a[85], 7.454 + 9.046 * lzs));
        a[86] = f64::min(XR[101] + lzs * XR[102], f64::max(2.0, -13.3 - 18.6 * lzs));
        a[87] = f64::min(1.5, f64::max(0.4, 2.493 + 1.1475 * lzs));
        a[88] = f64::max(1.0, f64::min(1.27, 0.8109 - 0.6282 * lzs));
        a[88] = f64::max(a[88], 0.6355 - 0.4192 * lzs);
        a[89] = f64::max(5.855_420e-2, -0.2711 - lzs * (0.5756 + 0.0838 * lzs));

        // Radius hook (90–96).
        for k in 0..4 {
            a[90 + k] = poly(run(&XR, 104 + 4 * k, 4), lzs);
        }
        a[94] = f64::min(1.25, f64::max(1.1, 1.9848 + lzs * (1.1386 + 0.3564 * lzs)));
        a[95] = 0.063 + lzs * (0.0481 + 0.00984 * lzs);
        a[96] = f64::min(1.3, f64::max(0.45, 1.2 + 2.45 * lzs));

        // Cap on the luminosity exponent η.
        a[97] = if z > 0.0009 { 10.0 } else { 20.0 };

        Self {
            z,
            a,
            m_hook: 1.0185 + lzs * (0.16015 + lzs * 0.0892),
            x_ms: f64::max(
                0.95,
                f64::max(
                    0.95 - (10.0 / 3.0) * (z - 0.01),
                    f64::min(0.99, 0.98 - (100.0 / 7.0) * (z - 0.001)),
                ),
            ),
            hydrogen_x: 0.76 - 3.0 * z,
        }
    }

    fn check_mass(mass: SolarMass) -> Result<f64, StellarError> {
        if (MIN_MASS_MSUN..=MAX_MASS_MSUN).contains(&mass.0) {
            Ok(mass.0)
        } else {
            Err(StellarError::OutOfRange {
                quantity: "stellar mass (M☉)",
                value: mass.0,
                min: MIN_MASS_MSUN,
                max: MAX_MASS_MSUN,
            })
        }
    }

    /// Zero-age main-sequence luminosity (Tout et al. 1996, eq. 1).
    fn l_zams(&self, m: f64) -> f64 {
        let a = &self.a;
        (a[1] * pow(m, 5.5) + a[2] * pow(m, 11.0))
            / (a[3]
                + pow(m, 3.0)
                + a[4] * pow(m, 5.0)
                + a[5] * pow(m, 7.0)
                + a[6] * pow(m, 8.0)
                + a[7] * pow(m, 9.5))
    }

    /// Zero-age main-sequence radius (Tout et al. 1996, eq. 2).
    fn r_zams(&self, m: f64) -> f64 {
        let a = &self.a;
        (a[8] * pow(m, 2.5)
            + a[9] * pow(m, 6.5)
            + a[10] * pow(m, 11.0)
            + a[11] * pow(m, 19.0)
            + a[12] * pow(m, 19.5))
            / (a[13]
                + a[14] * pow(m, 2.0)
                + a[15] * pow(m, 8.5)
                + pow(m, 18.5)
                + a[16] * pow(m, 19.5))
    }

    /// Time to the base of the giant branch, Myr (HPT eq. 4).
    fn t_bgb(&self, m: f64) -> f64 {
        let a = &self.a;
        (a[17] + a[18] * pow(m, 4.0) + a[19] * pow(m, 5.5) + pow(m, 7.0))
            / (a[20] * pow(m, 2.0) + a[21] * pow(m, 7.0))
    }

    /// Time of the hook as a fraction of [`Self::t_bgb`] (HPT eqs. 5 and 7).
    fn hook_fraction(&self, m: f64) -> f64 {
        let a = &self.a;
        let mu = 1.0 - 0.01 * f64::max(a[22] / pow(m, a[23]), a[24] + a[25] / pow(m, a[26]));
        mu.max(0.5)
    }

    /// Luminosity at the end of the main sequence (HPT eq. 8).
    fn l_tms(&self, m: f64) -> f64 {
        let a = &self.a;
        (a[27] * pow(m, 3.0) + a[28] * pow(m, 4.0) + a[29] * pow(m, a[32] + 1.8))
            / (a[30] + a[31] * pow(m, 5.0) + pow(m, a[32]))
    }

    /// Luminosity coefficient α_L (HPT eq. 19).
    fn l_alpha(&self, m: f64) -> f64 {
        let a = &self.a;
        let line = |x0: f64, y0: f64, x1: f64, y1: f64| y0 + (y1 - y0) / (x1 - x0) * (m - x0);
        if m >= 2.0 {
            (a[33] + a[34] * pow(m, a[36])) / (pow(m, 0.4) + a[35] * pow(m, 1.9))
        } else if m <= 0.5 {
            a[39]
        } else if m <= 0.7 {
            line(0.5, a[39], 0.7, 0.3)
        } else if m <= a[37] {
            line(0.7, 0.3, a[37], a[40])
        } else if m <= a[38] {
            line(a[37], a[40], a[38], a[41])
        } else {
            line(a[38], a[41], 2.0, a[42])
        }
    }

    /// Luminosity coefficient β_L (HPT eq. 20).
    fn l_beta(&self, m: f64) -> f64 {
        let a = &self.a;
        let mut beta = f64::max(a[43] - a[44] * pow(m, a[45]), 0.0);
        if m > a[46] && beta > 0.0 {
            let at_cut = a[43] - a[44] * pow(a[46], a[45]);
            beta = f64::max(at_cut - 10.0 * at_cut * (m - a[46]), 0.0);
        }
        beta
    }

    /// Luminosity exponent η (HPT eq. 18).
    fn l_eta(&self, m: f64) -> f64 {
        let eta = if m <= 1.0 {
            10.0
        } else if m >= 1.1 {
            20.0
        } else {
            10.0 + 100.0 * (m - 1.0)
        };
        eta.min(self.a[97])
    }

    /// Size of the luminosity hook, ΔL (HPT eq. 16).
    fn l_hook(&self, m: f64) -> f64 {
        let a = &self.a;
        let above = |m: f64| f64::min(a[47] / pow(m, a[48]), a[49] / pow(m, a[50]));
        if m <= self.m_hook {
            0.0
        } else if m >= a[51] {
            above(m)
        } else {
            above(a[51]) * pow((m - self.m_hook) / (a[51] - self.m_hook), 0.4)
        }
    }

    /// Radius at the end of the main sequence (HPT eq. 9). Never less than
    /// 1.5 times the zero-age radius at low mass, where the fit is
    /// extrapolated.
    fn r_tms(&self, m: f64) -> f64 {
        let a = &self.a;
        let m2 = a[62] + 0.1;
        if m <= a[62] {
            f64::max(
                1.5 * self.r_zams(m),
                (a[52] + a[53] * pow(m, a[55])) / (a[54] + pow(m, a[56])),
            )
        } else if m >= m2 {
            (a[57] * pow(m, 3.0) + a[58] * pow(m, a[61]) + a[59] * pow(m, a[61] + 1.5))
                / (a[60] + pow(m, 5.0))
        } else {
            a[63] + (a[64] - a[63]) / 0.1 * (m - a[62])
        }
    }

    /// Radius coefficient α_R (HPT eq. 21).
    fn r_alpha(&self, m: f64) -> f64 {
        let a = &self.a;
        let line = |x0: f64, y0: f64, x1: f64, y1: f64| y0 + (y1 - y0) / (x1 - x0) * (m - x0);
        let fit = |m: f64| (a[65] * pow(m, a[67])) / (a[66] + pow(m, a[68]));
        if m <= 0.5 {
            a[73]
        } else if m <= 0.65 {
            line(0.5, a[73], 0.65, a[74])
        } else if m <= a[70] {
            line(0.65, a[74], a[70], a[75])
        } else if m <= a[71] {
            line(a[70], a[75], a[71], a[76])
        } else if m <= a[72] {
            fit(m)
        } else {
            fit(a[72]) + a[69] * (m - a[72])
        }
    }

    /// Radius coefficient β_R (HPT eq. 22).
    fn r_beta(&self, m: f64) -> f64 {
        let a = &self.a;
        let fit = |m: f64| (a[77] * pow(m, 3.5)) / (a[78] + pow(m, a[79]));
        let beta = if m <= 1.0 {
            1.06
        } else if m <= a[82] {
            1.06 + (a[81] - 1.06) / (a[82] - 1.0) * (m - 1.0)
        } else if m <= 2.0 {
            a[81] + (fit(2.0) - a[81]) / (2.0 - a[82]) * (m - a[82])
        } else if m <= 16.0 {
            fit(m)
        } else {
            fit(16.0) + a[80] * (m - 16.0)
        };
        beta - 1.0
    }

    /// Radius coefficient γ (HPT eq. 23).
    fn r_gamma(&self, m: f64) -> f64 {
        let a = &self.a;
        if m > a[88] + 0.1 {
            return 0.0;
        }
        let mut b1 = f64::max(0.0, a[83] + a[84] * pow(1.0 - a[85], a[86]));
        let gamma = if m <= 1.0 {
            a[83] + a[84] * pow(abs(m - a[85]), a[86])
        } else if m <= a[88] {
            b1 + (a[89] - b1) * pow((m - 1.0) / (a[88] - 1.0), a[87])
        } else {
            if a[88] > 1.0 {
                b1 = a[89];
            }
            b1 - 10.0 * b1 * (m - a[88])
        };
        gamma.max(0.0)
    }

    /// Size of the radius hook, ΔR (HPT eq. 17).
    fn r_hook(&self, m: f64) -> f64 {
        let a = &self.a;
        let fit =
            |m: f64| (a[90] + a[91] * pow(m, 3.5)) / (a[92] * pow(m, 3.0) + pow(m, a[93])) - 1.0;
        if m <= self.m_hook {
            0.0
        } else if m <= a[94] {
            a[95] * sqrt((m - self.m_hook) / (a[94] - self.m_hook))
        } else if m <= 2.0 {
            a[95] + (fit(2.0) - a[95]) * pow((m - a[94]) / (2.0 - a[94]), a[96])
        } else {
            fit(m)
        }
    }

    /// Luminosity of a star that has just reached the main sequence.
    pub fn zams_luminosity(&self, mass: SolarMass) -> Result<SolarLuminosity, StellarError> {
        Ok(SolarLuminosity(self.l_zams(Self::check_mass(mass)?)))
    }

    /// Radius of a star that has just reached the main sequence.
    pub fn zams_radius(&self, mass: SolarMass) -> Result<SolarRadius, StellarError> {
        Ok(SolarRadius(self.r_zams(Self::check_mass(mass)?)))
    }

    /// Main-sequence lifetime in Myr (HPT eq. 5): the later of the hook and
    /// a fixed fraction of the time to the base of the giant branch.
    pub fn lifetime_myr(&self, mass: SolarMass) -> Result<f64, StellarError> {
        let m = Self::check_mass(mass)?;
        Ok(f64::max(self.x_ms, self.hook_fraction(m)) * self.t_bgb(m))
    }

    /// Luminosity, radius and temperature at `age_myr` (HPT eqs. 11–15).
    ///
    /// Fails if the mass is out of range or the age is negative or past the
    /// end of the main sequence.
    pub fn state(&self, mass: SolarMass, age_myr: f64) -> Result<MsState, StellarError> {
        let m = Self::check_mass(mass)?;
        let t_ms = self.lifetime_myr(mass)?;
        if !(0.0..=t_ms).contains(&age_myr) {
            return Err(StellarError::OutOfRange {
                quantity: "age on the main sequence (Myr)",
                value: age_myr,
                min: 0.0,
                max: t_ms,
            });
        }
        let tau = age_myr / t_ms;
        let t_hook = self.hook_fraction(m) * self.t_bgb(m);
        // The hook is confined to the last 1% of the time before `t_hook`.
        const EPSILON: f64 = 0.01;
        let tau1 = f64::min(1.0, age_myr / t_hook);
        let tau2 = ((age_myr - (1.0 - EPSILON) * t_hook) / (EPSILON * t_hook)).clamp(0.0, 1.0);

        let l_zams = self.l_zams(m);
        let alpha = self.l_alpha(m);
        let beta = self.l_beta(m);
        let lx = log10(self.l_tms(m) / l_zams);
        let log_l = alpha * tau + beta * pow(tau, self.l_eta(m)) + (lx - alpha - beta) * tau * tau
            - self.l_hook(m) * (tau1 * tau1 - tau2 * tau2);
        let luminosity = l_zams * pow(10.0, log_l);

        let r_zams = self.r_zams(m);
        let alpha = self.r_alpha(m);
        let beta = self.r_beta(m);
        let gamma = self.r_gamma(m);
        let rx = log10(self.r_tms(m) / r_zams);
        let log_r = alpha * tau
            + beta * pow(tau, 10.0)
            + gamma * pow(tau, 40.0)
            + (rx - alpha - beta - gamma) * pow(tau, 3.0)
            - self.r_hook(m) * (pow(tau1, 3.0) - pow(tau2, 3.0));
        let mut radius = r_zams * pow(10.0, log_r);
        if m < self.m_hook - 0.3 {
            // Low-mass stars are partly degenerate and cannot be smaller
            // than this (HPT eq. 24).
            let degenerate = 0.0258 * pow(1.0 + self.hydrogen_x, 5.0 / 3.0) * pow(m, -1.0 / 3.0);
            radius = radius.max(degenerate);
        }

        let luminosity = SolarLuminosity(luminosity);
        let radius = SolarRadius(radius);
        Ok(MsState {
            luminosity,
            radius,
            teff: effective_temperature(luminosity, radius),
            fraction: tau,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polynomial_is_horner() {
        assert_eq!(poly(&[1.0, 2.0, 3.0], 2.0), 17.0);
        assert_eq!(poly(&[5.0], 9.0), 5.0);
    }

    /// Tout et al. (1996) at Z = 0.02 and 1 M☉: 0.698 L☉ and 0.888 R☉.
    #[test]
    fn zero_age_sun() {
        let ms = MainSequence::solar();
        let l = ms.zams_luminosity(SolarMass(1.0)).unwrap().0;
        let r = ms.zams_radius(SolarMass(1.0)).unwrap().0;
        assert!((l - 0.698).abs() < 1e-3, "{l}");
        assert!((r - 0.888).abs() < 1e-3, "{r}");
    }

    /// A 1 M☉ star lives about 11 Gyr on the main sequence and has roughly
    /// the Sun's luminosity and radius at the Sun's age.
    #[test]
    fn sun_today() {
        let ms = MainSequence::solar();
        let t = ms.lifetime_myr(SolarMass(1.0)).unwrap();
        assert!((t - 11_003.0).abs() < 1.0, "{t}");
        let now = ms.state(SolarMass(1.0), 4570.0).unwrap();
        assert!((now.luminosity.0 - 1.0).abs() < 0.05, "{:?}", now);
        assert!((now.radius.0 - 1.0).abs() < 0.02, "{:?}", now);
        assert!((now.teff.0 - 5772.0).abs() < 60.0, "{:?}", now);
    }

    #[test]
    fn stars_brighten_and_swell_with_age() {
        let ms = MainSequence::solar();
        for mass in [0.3, 0.7, 1.0, 1.4, 2.5, 8.0] {
            let t = ms.lifetime_myr(SolarMass(mass)).unwrap();
            let mut last = ms.state(SolarMass(mass), 0.0).unwrap();
            for k in 1..=20 {
                // Stop short of the hook, where the radius briefly shrinks.
                let now = ms
                    .state(SolarMass(mass), 0.9 * t * f64::from(k) / 20.0)
                    .unwrap();
                assert!(now.luminosity.0 >= last.luminosity.0, "{mass} M☉ step {k}");
                assert!(now.radius.0 >= last.radius.0 * 0.999, "{mass} M☉ step {k}");
                last = now;
            }
        }
    }

    #[test]
    fn heavier_stars_are_brighter_and_shorter_lived() {
        let ms = MainSequence::solar();
        let masses = [0.08, 0.2, 0.5, 1.0, 2.0, 5.0, 20.0, 100.0];
        for pair in masses.windows(2) {
            let (lo, hi) = (SolarMass(pair[0]), SolarMass(pair[1]));
            assert!(ms.zams_luminosity(hi).unwrap().0 > ms.zams_luminosity(lo).unwrap().0);
            assert!(ms.lifetime_myr(hi).unwrap() < ms.lifetime_myr(lo).unwrap());
        }
    }

    /// Metal-poor stars are hotter and brighter at the same mass, and
    /// shorter-lived.
    #[test]
    fn metallicity_dependence_has_the_right_sign() {
        let solar = MainSequence::solar();
        let poor = MainSequence::new(0.001).unwrap();
        let m = SolarMass(1.0);
        assert!(poor.zams_luminosity(m).unwrap().0 > solar.zams_luminosity(m).unwrap().0);
        assert!(poor.lifetime_myr(m).unwrap() < solar.lifetime_myr(m).unwrap());
        let (a, b) = (poor.state(m, 0.0).unwrap(), solar.state(m, 0.0).unwrap());
        assert!(a.teff.0 > b.teff.0);
        assert!((z_from_feh(0.0) - SOLAR_Z).abs() < 1e-15);
        assert!((z_from_feh(-1.0) - 0.002).abs() < 1e-15);
    }

    #[test]
    fn out_of_range_inputs_are_rejected() {
        let ms = MainSequence::solar();
        assert!(MainSequence::new(0.05).is_err());
        assert!(MainSequence::new(1.0e-5).is_err());
        assert!(ms.zams_luminosity(SolarMass(0.05)).is_err());
        assert!(ms.lifetime_myr(SolarMass(150.0)).is_err());
        assert!(ms.state(SolarMass(1.0), -1.0).is_err());
        // A Sun-like star has left the main sequence by 12 Gyr.
        assert!(ms.state(SolarMass(1.0), 12_000.0).is_err());
    }
}
