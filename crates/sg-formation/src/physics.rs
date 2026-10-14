//! The rates that drive planet growth: pebble accretion, gas accretion and
//! migration. Pure functions of the local disk state, each from a published
//! prescription.
//!
//! Units as in `sg-disk`: AU, solar masses, years (`G = 4π²`).

use sg_core::math::{cbrt, exp, log10, pow, sqrt};
use sg_disk::{Disk, G};

/// One Earth mass in solar masses.
pub const EARTH_MASS_MSUN: f64 = 1.0 / sg_core::constants::SOLAR_MASS_IN_EARTH_MASSES;
/// 1 m/s in AU/yr.
const M_PER_S_IN_AU_YR: f64 = 100.0 / 4.7405e5;
/// Speed above which colliding grains shatter instead of sticking: about
/// 1 m/s for bare silicates and 10 m/s for icy grains (Gundlach & Blum
/// 2015, ApJ 798, 34).
const FRAGMENTATION_SPEED_ROCK_M_S: f64 = 1.0;
const FRAGMENTATION_SPEED_ICE_M_S: f64 = 10.0;
/// Fraction of a pebble's mass that is water ice outside the ice line
/// (Lodders 2003, ApJ 591, 1220: about half of the condensable solids).
pub const ICE_MASS_FRACTION: f64 = 0.5;
/// Efficiency with which dust grows into drifting pebbles (Lambrechts &
/// Johansen 2014, A&A 572, A107, §2: ε_D = 0.05).
const PEBBLE_GROWTH_EFFICIENCY: f64 = 0.05;

/// Hill radius, AU.
pub fn hill_radius(a_au: f64, mass_msun: f64, star_mass_msun: f64) -> f64 {
    a_au * pow(mass_msun / (3.0 * star_mass_msun), 1.0 / 3.0)
}

/// Radius of the pebble production front, AU: the distance at which dust
/// has just had time to grow into pebbles and start drifting.
///
/// Lambrechts & Johansen (2014), eq. 10:
/// `r_g = (3/16)^(1/3) (G M★)^(1/3) (ε_D Z)^(2/3) t^(2/3)`.
pub fn pebble_front_radius(star_mass_msun: f64, dust_to_gas: f64, t_yr: f64) -> f64 {
    pow(3.0 / 16.0, 1.0 / 3.0)
        * pow(G * star_mass_msun, 1.0 / 3.0)
        * pow(PEBBLE_GROWTH_EFFICIENCY * dust_to_gas, 2.0 / 3.0)
        * pow(t_yr.max(0.0), 2.0 / 3.0)
}

/// Mass of pebbles drifting inward through the disk per year, M☉/yr.
///
/// Lambrechts & Johansen (2014), eq. 14: the front where dust has grown into
/// pebbles moves outward and turns the dust it reaches into pebbles,
/// `Ṁ_F = 2π r_g (dr_g/dt) Z Σ_g(r_g)`, with `dr_g/dt = (2/3) r_g/t`. Their
/// disk does not evolve. Here it spreads, and the dust that has not yet
/// grown is small enough to move with the gas (Birnstiel, Klahr & Ercolano
/// 2012), so the dust at the front is `Z` times the gas there *now*, and the
/// front gains what the gas carries across it: `Ṁ_F = Z (2π r_g (dr_g/dt)
/// Σ_g(r_g, t) + Ṁ_g(r_g, t))`, with `Ṁ_g` the inward gas flow
/// ([`Disk::mass_flow_rate`], negative where the disk spreads outward
/// ahead of the front). Dust that has not reached the front has never been
/// converted, so it keeps the initial `Z`; in total the pebbles never
/// outweigh the disk's dust. A spreading disk keeps feeding pebbles long
/// after the front has passed its initial radius, as viscous models with
/// drift find (Appelgren, Lambrechts & Johansen 2020, A&A 638, A156: 10–100
/// M⊕ of dust left at 1–3 Myr). Stops when the gas is gone. Not modelled:
/// small dust carried onto the star with the gas inside the front, which is
/// none here because everything inside the front has grown into pebbles.
pub fn pebble_flux(disk: &Disk, t_yr: f64) -> f64 {
    if t_yr <= 0.0 {
        return 0.0;
    }
    let spec = disk.spec();
    let front = pebble_front_radius(spec.star_mass_msun, spec.dust_to_gas, t_yr);
    // Past the disk's edge there is nothing left to sweep.
    let (front, speed) = if front < spec.outer_edge_au {
        (front, 2.0 / 3.0 * front / t_yr)
    } else {
        (spec.outer_edge_au, 0.0)
    };
    let swept = 2.0 * core::f64::consts::PI * front * speed * disk.gas_surface_density(front, t_yr);
    (spec.dust_to_gas * (swept + disk.mass_flow_rate(front, t_yr))).max(0.0)
}

/// Stokes number of the pebbles at a radius: the drift-limited value, or
/// less where collisions shatter them first.
///
/// Fragmentation limit (Birnstiel, Klahr & Ercolano 2012, A&A 539, A148,
/// eq. 8): `St = v_f² / (3 α c_s²)`, with a lower fragmentation speed
/// inside the ice line, where grains have lost their sticky ice mantles.
pub fn stokes_number(
    disk: &Disk,
    r_au: f64,
    t_yr: f64,
    drift_limited: f64,
    alpha_turbulent: f64,
    ice_line_au: f64,
) -> f64 {
    let v_f = M_PER_S_IN_AU_YR
        * if r_au < ice_line_au {
            FRAGMENTATION_SPEED_ROCK_M_S
        } else {
            FRAGMENTATION_SPEED_ICE_M_S
        };
    let cs = disk.sound_speed(r_au, t_yr);
    let fragmentation = v_f * v_f / (3.0 * alpha_turbulent * cs * cs);
    drift_limited.min(fragmentation)
}

/// Mass at which a seed starts accreting pebbles efficiently from its whole
/// Hill sphere, M☉.
///
/// Lambrechts & Johansen (2012, A&A 544, A32), eq. 33:
/// `M_t = sqrt(1/3) (η v_K)³ / (G Ω)`.
pub fn transition_mass(disk: &Disk, r_au: f64, t_yr: f64) -> f64 {
    let omega = disk.omega(r_au);
    let dv = disk.eta(r_au, t_yr) * r_au * omega;
    sqrt(1.0 / 3.0) * dv * dv * dv / (G * omega)
}

/// Fraction of the pebbles drifting past a body on a circular, coplanar
/// orbit that it accretes: the pebble accretion efficiency.
///
/// Liu & Ormel (2018, A&A 615, A138) for the 2D limit and Ormel & Liu
/// (2018, A&A 615, A178) for the 3D limit and their combination:
///
/// - approach speed `Δv = v_hw / (1 + 5.7 q_p τ_s/η³) + v_sh`, with the
///   headwind `v_hw = η v_K` and the shear `v_sh = 0.52 (q_p τ_s)^(1/3) v_K`
///   (Liu & Ormel eq. 10, `a_hw/sh = 5.7`);
/// - settling factor `f_set = exp(−½ (Δv/v*)²)`, `v* = (q_p/τ_s)^(1/3) v_K`
///   (eqs. 12–13): pebbles passing too fast to be deflected are not caught,
///   which sets the mass at which accretion starts in earnest;
/// - `ε_2D = 0.32 sqrt(q_p Δv / (τ_s η² v_K)) f_set` (eq. 8) and
///   `ε_3D = 0.39 q_p / (η h_P) f_set²` (Ormel & Liu eq. 22, `A₃` from
///   their table 1), with `h_P` the pebble layer's aspect ratio;
/// - `ε = (ε_2D⁻² + ε_3D⁻²)^(-1/2)` (Ormel & Liu eq. 40), and the 2D
///   saturation `ε → 1 − exp(−ε)` (Liu & Ormel eq. 16) so that it never
///   exceeds one.
///
/// Left out: the ballistic term, which matters only for bodies so small
/// that geometric collisions dominate (well below 10⁻⁴ M⊕ here), and the
/// stirring of pebbles by turbulence in `f_set` (Ormel & Liu eq. 37), small
/// for α ≲ 10⁻³. Valid for `τ_s ≲ 1`.
pub fn pebble_accretion_efficiency(
    mass_ratio: f64,
    stokes: f64,
    eta: f64,
    pebble_aspect_ratio: f64,
) -> f64 {
    if mass_ratio <= 0.0 || stokes <= 0.0 || eta <= 0.0 || pebble_aspect_ratio <= 0.0 {
        return 0.0;
    }
    let q = mass_ratio;
    let headwind = eta / (1.0 + 5.7 * q * stokes / (eta * eta * eta));
    let shear = 0.52 * cbrt(q * stokes);
    let dv = headwind + shear;
    let v_star = cbrt(q / stokes);
    let settling = exp(-0.5 * (dv / v_star) * (dv / v_star));
    let flat = 0.32 * sqrt(q * dv / (stokes * eta * eta)) * settling;
    let thick = 0.39 * q / (eta * pebble_aspect_ratio) * settling * settling;
    if flat <= 0.0 || thick <= 0.0 {
        return 0.0;
    }
    let combined = 1.0 / sqrt(1.0 / (flat * flat) + 1.0 / (thick * thick));
    1.0 - exp(-combined)
}

/// Rate at which a body accretes pebbles, M☉/yr, given the pebble flux
/// reaching its orbit: the flux times [`pebble_accretion_efficiency`].
/// The pebble layer's thickness is `H_peb = H sqrt(α/(α + St))` (Youdin &
/// Lithwick 2007, as Ormel & Liu 2018 eq. 15).
pub fn pebble_accretion_rate(
    disk: &Disk,
    r_au: f64,
    t_yr: f64,
    mass_msun: f64,
    flux_msun_yr: f64,
    stokes: f64,
    alpha_turbulent: f64,
) -> f64 {
    if flux_msun_yr <= 0.0 || stokes <= 0.0 {
        return 0.0;
    }
    let q = mass_msun / disk.spec().star_mass_msun;
    let h_peb = disk.aspect_ratio(r_au, t_yr) * sqrt(alpha_turbulent / (alpha_turbulent + stokes));
    flux_msun_yr * pebble_accretion_efficiency(q, stokes, disk.eta(r_au, t_yr), h_peb)
}

/// Mass at which a planet carves a gap deep enough to stop pebbles
/// reaching it, M☉.
///
/// Bitsch et al. (2018, A&A 612, A30), eqs. 10–11:
/// `M_iso = 25 M⊕ · (h/0.05)³ · [0.34 (log α₃ / log α)⁴ + 0.66] ·
/// [1 − (d ln P/d ln r + 2.5)/6]`, with `α₃ = 10⁻³`. Fitted to 3D
/// hydrodynamical simulations.
pub fn pebble_isolation_mass(disk: &Disk, r_au: f64, t_yr: f64, alpha_turbulent: f64) -> f64 {
    let h = disk.aspect_ratio(r_au, t_yr);
    let turbulence = 0.34 * pow(log10(1.0e-3) / log10(alpha_turbulent), 4.0) + 0.66;
    let pressure = 1.0 - (disk.pressure_slope(r_au, t_yr) + 2.5) / 6.0;
    25.0 * EARTH_MASS_MSUN * pow(h / 0.05, 3.0) * turbulence * pressure
}

/// Gap-opening parameter `K = q² h⁻⁵ α⁻¹` (Kanagawa et al. 2015). The gas
/// surface density in the gap is `1/(1 + 0.04 K)` of its unperturbed value
/// (Kanagawa et al. 2018, ApJ 861, 140, eq. 11).
pub fn gap_parameter(
    disk: &Disk,
    r_au: f64,
    t_yr: f64,
    mass_msun: f64,
    alpha_turbulent: f64,
) -> f64 {
    let q = mass_msun / disk.spec().star_mass_msun;
    q * q / (pow(disk.aspect_ratio(r_au, t_yr), 5.0) * alpha_turbulent)
}

/// Rate of change of a planet's orbital radius by disk migration, AU/yr
/// (negative: inward).
///
/// Type I torque for a locally isothermal disk (D'Angelo & Lubow 2010,
/// ApJ 724, 730, eq. 49): `Γ = −(1.36 + 0.62 β_Σ + 0.43 β_T) (q/h)² Σ a⁴ Ω²`,
/// giving `da/dt = 2Γ/(M_p a Ω)`. As the planet opens a gap the torque
/// falls with the gas density there, by `1/(1 + 0.04 K)`, which carries the
/// rate smoothly into the slow type II regime (Kanagawa, Tanaka & Szuszkiewicz
/// 2018). Not included: corotation torques, which can halt or reverse
/// migration in parts of the disk (Paardekooper et al. 2011).
pub fn migration_rate(
    disk: &Disk,
    a_au: f64,
    t_yr: f64,
    mass_msun: f64,
    alpha_turbulent: f64,
) -> f64 {
    let sigma = disk.gas_surface_density(a_au, t_yr);
    if sigma <= 0.0 {
        return 0.0;
    }
    let star = disk.spec().star_mass_msun;
    let q = mass_msun / star;
    let h = disk.aspect_ratio(a_au, t_yr);
    let coefficient = 1.36
        + 0.62 * disk.surface_density_slope(a_au, t_yr)
        + 0.43 * disk.temperature_slope(a_au, t_yr);
    let gap = 1.0 + 0.04 * gap_parameter(disk, a_au, t_yr, mass_msun, alpha_turbulent);
    -2.0 * coefficient * q * (sigma * a_au * a_au / star) / (h * h) * a_au * disk.omega(a_au) / gap
}

/// Kelvin–Helmholtz time of a 1 M⊕ planet in Ida & Lin's (2004) nominal
/// fit, years.
pub const NOMINAL_COOLING_TIME_YR: f64 = 1.0e9;

/// Rate at which a planet's envelope cools, contracts and draws in gas,
/// M☉/yr.
///
/// Ida & Lin (2004, ApJ 604, 388), eqs. 9–10: `dM/dt = M/τ_KH` with the
/// Kelvin–Helmholtz time `τ_KH = τ₁ (M/M⊕)⁻³`, a fit to the envelope
/// calculations of Ikoma, Nakazawa & Emori (2000). Slow for a few Earth
/// masses, a runaway above about ten: the classical critical core mass.
/// Their nominal `τ₁` is 10⁹ years ([`NOMINAL_COOLING_TIME_YR`]); it scales
/// with the opacity of the envelope, which depends on how much dust it
/// holds and is uncertain by more than an order of magnitude (Ikoma et al.
/// 2000 find `τ ∝ κ`; Ida & Lin 2008 explore 10⁹–10¹⁰ years), so it is a
/// parameter here.
pub fn envelope_contraction_rate(mass_msun: f64, cooling_time_1mearth_yr: f64) -> f64 {
    let mass = mass_msun / EARTH_MASS_MSUN;
    let kelvin_helmholtz_yr = cooling_time_1mearth_yr / (mass * mass * mass);
    mass_msun / kelvin_helmholtz_yr
}

/// Rate of runaway gas accretion, M☉/yr: the smallest of three limits.
///
/// - What a low-mass planet's Hill sphere can capture:
///   `0.83 Ω Σ H² (r_H/H)^(9/2)` (Machida et al. 2010, MNRAS 405, 1227, as
///   used by Bitsch et al. 2015, eq. 18).
/// - What flows into the planet from the gap it has opened:
///   `Ṁ = D Σ_gap` with `D = 0.29 h⁻² q^(4/3) a² Ω` (Tanigawa & Tanaka 2016,
///   ApJ 823, 48, eqs. 2–3) and `Σ_gap = Σ/(1 + 0.04 K)` (Kanagawa et al.
///   2018). As the planet grows the gap deepens faster than `D` rises, so
///   the planet starves itself.
/// - 80% of the gas flowing through the disk (Lubow & D'Angelo 2006,
///   ApJ 641, 526: most, not all, of the flow is captured).
pub fn runaway_accretion_rate(
    disk: &Disk,
    a_au: f64,
    t_yr: f64,
    mass_msun: f64,
    alpha_gap: f64,
) -> f64 {
    let sigma = disk.gas_surface_density(a_au, t_yr);
    let h = disk.aspect_ratio(a_au, t_yr);
    let h_gas = h * a_au;
    let omega = disk.omega(a_au);
    let star = disk.spec().star_mass_msun;
    let r_h = hill_radius(a_au, mass_msun, star);
    let hill_capture = 0.83 * omega * sigma * h_gas * h_gas * pow(r_h / h_gas, 4.5);
    let gap = 1.0 + 0.04 * gap_parameter(disk, a_au, t_yr, mass_msun, alpha_gap);
    let through_gap =
        0.29 / (h * h) * pow(mass_msun / star, 4.0 / 3.0) * a_au * a_au * omega * sigma / gap;
    hill_capture
        .min(through_gap)
        .min(0.8 * disk.accretion_rate(t_yr))
}

/// 1 M☉/AU² in g/cm².
pub const MSUN_PER_AU2_IN_G_PER_CM2: f64 = 1.989e33 / (1.496e13 * 1.496e13);

/// Mass of a planetesimal as streaming instability forms it, M☉: a body
/// 100 km across of density 1.2 g/cm³, 6.28×10²⁰ g (Lenz, Klahr & Birnstiel
/// 2019, ApJ 874, 36, eq. 4).
pub const PLANETESIMAL_MASS_MSUN: f64 = 6.28e20 / 1.989e33;

/// Fraction of the pebble flux turned into planetesimals as it crosses a
/// ring of radial width `dr`.
///
/// Lenz, Klahr & Birnstiel (2019), eq. 1: pebbles are caught in pressure
/// traps a distance `d = 5 H` apart and a fraction `ε` of what enters a trap
/// becomes planetesimals, so `Σ̇_p = (ε/d) Ṁ_peb/(2πr)` and the flux falls by
/// `ε·dr/d` across the ring. Their fiducial `ε` is 0.1. They form
/// planetesimals only from pebbles of Stokes number 0.01–10; that limit is
/// not applied here (see `evolve`).
pub fn planetesimal_conversion_fraction(efficiency: f64, dr_au: f64, scale_height_au: f64) -> f64 {
    (efficiency * dr_au / (5.0 * scale_height_au)).clamp(0.0, 1.0)
}

/// Pebble flux below which a trap cannot gather one planetesimal's mass in
/// its lifetime, M☉/yr, so none form.
///
/// Lenz et al. (2019), eqs. 5–6: `Ṁ_cr = m_p/(ε τ_l)` with a trap lifetime
/// `τ_l` of 100 orbits.
pub fn critical_pebble_flux(efficiency: f64, orbital_period_yr: f64) -> f64 {
    PLANETESIMAL_MASS_MSUN / (efficiency * 100.0 * orbital_period_yr)
}

/// Time for an embryo to double its mass by sweeping up planetesimals,
/// years.
///
/// Ida & Lin (2004, ApJ 604, 388), eq. 6, for oligarchic growth with the
/// planetesimals' random motions damped by gas drag:
/// `τ = 1.2×10⁵ yr (Σ_d/10 g cm⁻²)⁻¹ (a/AU)^(1/2) (M/M⊕)^(1/3) (M★/M☉)^(-1/6)
/// × [(Σ_g/2400 g cm⁻²)^(-1/5) (a/AU)^(1/20) (m/10¹⁸ g)^(1/15)]²`, with `m` the
/// planetesimal mass ([`PLANETESIMAL_MASS_MSUN`]). Infinite where there are
/// no planetesimals or no gas (without gas, growth is far slower and is
/// left to the stage after the disk).
pub fn planetesimal_accretion_time_yr(
    solids_g_cm2: f64,
    gas_g_cm2: f64,
    a_au: f64,
    mass_msun: f64,
    star_mass_msun: f64,
) -> f64 {
    if solids_g_cm2 <= 0.0 || gas_g_cm2 <= 0.0 {
        return f64::INFINITY;
    }
    let drag = pow(gas_g_cm2 / 2.4e3, -0.2)
        * pow(a_au, 0.05)
        * pow(PLANETESIMAL_MASS_MSUN * 1.989e33 / 1.0e18, 1.0 / 15.0);
    1.2e5 / (solids_g_cm2 / 10.0)
        * sqrt(a_au)
        * pow(mass_msun / EARTH_MASS_MSUN, 1.0 / 3.0)
        * pow(star_mass_msun, -1.0 / 6.0)
        * drag
        * drag
}

#[cfg(test)]
mod tests {
    use super::*;
    use sg_disk::DiskSpec;

    const ALPHA_T: f64 = 1.0e-4;

    fn disk() -> Disk {
        Disk::new(DiskSpec {
            star_mass_msun: 1.0,
            star_luminosity_lsun: 1.0,
            initial_mass_msun: 0.05,
            characteristic_radius_au: 40.0,
            alpha: 5.0e-3,
            dispersal_time_yr: 3.0e6,
            dust_to_gas: 0.0149,
            inner_edge_au: 0.05,
            outer_edge_au: 1000.0,
        })
        .unwrap()
    }

    fn earths(msun: f64) -> f64 {
        msun / EARTH_MASS_MSUN
    }

    /// Lambrechts & Johansen (2014): the front reaches roughly 100 AU in a
    /// million years, and the flux is of order 10⁻⁴ M⊕/yr.
    #[test]
    fn pebble_front_and_flux_have_the_published_scale() {
        let front = pebble_front_radius(1.0, 0.01, 1.0e6);
        assert!((80.0..160.0).contains(&front), "{front}");
        // t^(2/3): eight times later, four times farther.
        assert!((pebble_front_radius(1.0, 0.01, 8.0e6) / front - 4.0).abs() < 1e-9);
        let flux = earths(pebble_flux(&disk(), 3.0e5));
        assert!((1.0e-5..1.0e-3).contains(&flux), "{flux} M⊕/yr");
        assert_eq!(pebble_flux(&disk(), 0.0), 0.0);
        assert_eq!(pebble_flux(&disk(), 5.0e6), 0.0);
    }

    /// Pebbles drawn from the whole disk cannot outweigh its dust.
    #[test]
    fn pebble_flux_does_not_exceed_the_dust_supply() {
        let disk = disk();
        let n = 30_000;
        let dt = 3.0e6 / f64::from(n);
        let total: f64 = (0..n)
            .map(|k| pebble_flux(&disk, (f64::from(k) + 0.5) * dt) * dt)
            .sum();
        let dust = disk.spec().dust_to_gas * disk.spec().initial_mass_msun;
        assert!(total < dust, "{} vs {} M⊕", earths(total), earths(dust));
        assert!(total > 0.2 * dust);
    }

    #[test]
    fn pebbles_are_smaller_inside_the_ice_line() {
        let disk = disk();
        let ice = disk.water_ice_line(1.0e6);
        let inside = stokes_number(&disk, 0.5 * ice, 1.0e6, 0.05, ALPHA_T, ice);
        let outside = stokes_number(&disk, 3.0 * ice, 1.0e6, 0.05, ALPHA_T, ice);
        assert!(inside < outside, "{inside} {outside}");
        assert!(outside <= 0.05 && inside > 0.0);
    }

    /// Seeds start at 10⁻⁴–10⁻² M⊕ (Bitsch et al. 2015, §2.3).
    #[test]
    fn transition_mass_is_a_fraction_of_the_moon_to_mars() {
        let disk = disk();
        for r in [1.0, 5.0, 20.0] {
            let m = earths(transition_mass(&disk, r, 1.0e6));
            assert!((1.0e-5..5.0e-2).contains(&m), "{r} AU: {m} M⊕");
        }
        assert!(transition_mass(&disk, 20.0, 1.0e6) > transition_mass(&disk, 1.0, 1.0e6));
    }

    /// Liu et al. (2019, A&A 632, A7), eq. 20, quoting Liu & Ormel (2018):
    /// in the shear regime `ε_2D ≈ 3×10⁻³ (M/0.01 M⊕)^(2/3) (τ_s/0.01)^(-1/3)
    /// (η/3×10⁻³)^(-1)` around a solar-mass star; and the 3D limit is
    /// `0.39 q/(η h_P)` (Ormel & Liu 2018, eq. 22).
    #[test]
    fn pebble_accretion_efficiency_has_the_published_limits() {
        let q = |m_earth: f64| m_earth * EARTH_MASS_MSUN;
        // A thin pebble layer (2D) for a planet well past the onset mass.
        // At 10 M⊕ the shear dominates the approach speed; compare before
        // the saturation `1 − exp(−ε)`. The quoted 3×10⁻³ is rounded from
        // 0.32·√0.52 (q_p/0.01 M⊕)^(2/3)… = 3.45×10⁻³.
        let saturated = pebble_accretion_efficiency(q(10.0), 0.01, 3.0e-3, 1.0e-9);
        let flat = -sg_core::math::ln(1.0 - saturated);
        let expected = 3.0e-3 * pow(1000.0, 2.0 / 3.0);
        assert!((flat / expected - 1.0).abs() < 0.2, "{flat} vs {expected}");
        // A thick layer, where the 3D limit applies.
        // (with the settling factor, 0.95 here, squared).
        let h_p = 1.0e-2;
        let thick = pebble_accretion_efficiency(q(0.01), 0.01, 3.0e-3, h_p);
        let limit = 0.39 * q(0.01) / (3.0e-3 * h_p);
        assert!(thick < limit && thick > 0.9 * limit, "{thick} vs {limit}");
        // Far below the onset mass pebbles pass by untouched.
        assert!(pebble_accretion_efficiency(q(1.0e-7), 0.01, 3.0e-3, h_p) < 1.0e-12);
        // Never more than all of them.
        for m in [1.0e-4, 1.0, 100.0, 1.0e4] {
            let e = pebble_accretion_efficiency(q(m), 0.1, 1.0e-3, 1.0e-3);
            assert!((0.0..=1.0).contains(&e), "{m}: {e}");
        }
        assert_eq!(pebble_accretion_efficiency(0.0, 0.01, 3.0e-3, h_p), 0.0);
    }

    /// The flux is drawn from the disk as it spreads, so it lasts longer
    /// than from a disk frozen at its start, and its total stays below the
    /// dust there is.
    #[test]
    fn a_spreading_disk_feeds_pebbles_for_longer() {
        let disk = disk();
        let late = pebble_flux(&disk, 2.0e6);
        let spec = disk.spec();
        let front = pebble_front_radius(spec.star_mass_msun, spec.dust_to_gas, 2.0e6);
        let frozen = 2.0
            * core::f64::consts::PI
            * front
            * (2.0 / 3.0 * front / 2.0e6)
            * spec.dust_to_gas
            * disk.gas_surface_density(front, 0.0);
        assert!(
            late > frozen,
            "{} vs {} M⊕/yr",
            earths(late),
            earths(frozen)
        );
    }

    #[test]
    fn pebble_accretion_grows_with_mass_and_never_exceeds_the_flux() {
        let disk = disk();
        let (r, t) = (5.0, 5.0e5);
        let flux = pebble_flux(&disk, t);
        let rate = |m_earth: f64| {
            pebble_accretion_rate(&disk, r, t, m_earth * EARTH_MASS_MSUN, flux, 0.05, ALPHA_T)
        };
        assert!(rate(0.01) < rate(0.1) && rate(0.1) < rate(1.0));
        for m in [1.0e-3, 0.1, 10.0, 1000.0] {
            assert!(rate(m) <= flux);
        }
        // An Earth-mass core doubles in well under a million years.
        let doubling_yr = EARTH_MASS_MSUN / rate(1.0);
        assert!((1.0e3..1.0e6).contains(&doubling_yr), "{doubling_yr}");
        assert_eq!(
            pebble_accretion_rate(&disk, r, t, EARTH_MASS_MSUN, 0.0, 0.05, ALPHA_T),
            0.0
        );
    }

    /// Bitsch et al. (2018): 25 M⊕ at h = 0.05, α = 10⁻³ and
    /// d ln P/d ln r = −2.5; lower in a quieter disk.
    #[test]
    fn isolation_mass_matches_the_published_normalisation() {
        let disk = disk();
        let (r, t) = (5.0, 1.0e6);
        let h = disk.aspect_ratio(r, t);
        let slope = disk.pressure_slope(r, t);
        let expected = 25.0 * (h / 0.05).powi(3) * (1.0 - (slope + 2.5) / 6.0);
        let got = earths(pebble_isolation_mass(&disk, r, t, 1.0e-3));
        assert!((got / expected - 1.0).abs() < 1e-12, "{got} vs {expected}");
        assert!(
            pebble_isolation_mass(&disk, r, t, 1.0e-4) < pebble_isolation_mass(&disk, r, t, 1.0e-3)
        );
        // A few to a few tens of Earth masses across the planet-forming region.
        for r in [0.5, 5.0, 30.0] {
            let m = earths(pebble_isolation_mass(&disk, r, t, ALPHA_T));
            assert!((0.5..80.0).contains(&m), "{r} AU: {m}");
        }
    }

    /// A 10 M⊕ core at 5 AU migrates inward on a 10⁴–10⁶ year timescale
    /// (Tanaka et al. 2002), and a Jupiter, having opened a gap, far slower.
    #[test]
    fn migration_is_inward_and_slows_when_a_gap_opens() {
        let disk = disk();
        let (a, t) = (5.0, 1.0e6);
        let core = 10.0 * EARTH_MASS_MSUN;
        let rate = migration_rate(&disk, a, t, core, ALPHA_T);
        assert!(rate < 0.0);
        let timescale = a / -rate;
        assert!((1.0e4..2.0e6).contains(&timescale), "{timescale}");
        // Without a gap the rate is proportional to mass.
        let no_gap = |m: f64| -migration_rate(&disk, a, t, m, 1.0e9);
        assert!((no_gap(2.0 * core) / no_gap(core) - 2.0).abs() < 1e-9);
        let jupiter = 318.0 * EARTH_MASS_MSUN;
        assert!(gap_parameter(&disk, a, t, jupiter, ALPHA_T) > 1.0e3);
        let slow = a / -migration_rate(&disk, a, t, jupiter, ALPHA_T);
        assert!(slow > 10.0 * timescale, "{slow}");
        assert_eq!(migration_rate(&disk, a, 5.0e6, core, ALPHA_T), 0.0);
    }

    /// Ida & Lin (2004): a 10 M⊕ planet doubles in 10⁶ years, a 1 M⊕ one
    /// in 10⁹.
    #[test]
    fn envelope_contraction_follows_the_kelvin_helmholtz_time() {
        let rate = |earth_masses: f64| {
            envelope_contraction_rate(earth_masses * EARTH_MASS_MSUN, NOMINAL_COOLING_TIME_YR)
        };
        assert!((10.0 * EARTH_MASS_MSUN / rate(10.0) - 1.0e6).abs() < 1e-3);
        assert!((EARTH_MASS_MSUN / rate(1.0) - 1.0e9).abs() < 1.0);
        // dM/dt ∝ M⁴, and inversely as the cooling time.
        assert!((rate(20.0) / rate(10.0) - 16.0).abs() < 1e-9);
        let fast = envelope_contraction_rate(10.0 * EARTH_MASS_MSUN, 1.0e8);
        assert!((fast / rate(10.0) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn runaway_accretion_is_capped_by_the_disk_and_the_gap() {
        let disk = disk();
        let (a, t) = (5.0, 1.0e6);
        let earth = EARTH_MASS_MSUN;
        // With no gap (a very viscous disk) a Saturn takes all the disk gives.
        let supplied = runaway_accretion_rate(&disk, a, t, 95.0 * earth, 1.0e9);
        assert!((supplied / (0.8 * disk.accretion_rate(t)) - 1.0).abs() < 1e-12);
        // Deep in its gap, a heavier planet accretes more slowly: with
        // K ≫ 25 the rate falls as q^(-2/3).
        let rate = |m: f64| runaway_accretion_rate(&disk, a, t, m * earth, ALPHA_T);
        let (one, three) = (rate(318.0), rate(954.0));
        assert!(one < supplied);
        assert!(
            (three / one - 3f64.powf(-2.0 / 3.0)).abs() < 0.02,
            "{}",
            three / one
        );
        // A light planet in a shallow gap is limited by what its Hill
        // sphere captures, which rises as mass^(3/2): four times the mass,
        // eight times the rate.
        let shallow = |m: f64| runaway_accretion_rate(&disk, a, t, m * earth, 1.0);
        assert!((shallow(0.04) / shallow(0.01) - 8.0).abs() < 1e-6);
        assert_eq!(
            runaway_accretion_rate(&disk, a, 5.0e6, 95.0 * earth, ALPHA_T),
            0.0
        );
    }

    /// Lenz et al. (2019): with ε = 0.1 and traps 5 H apart, the flux falls
    /// by a tenth across one trap spacing; a 6.28×10²⁰ g planetesimal needs
    /// a flux of 10⁻⁸ M⊕/yr at 1 AU.
    #[test]
    fn planetesimal_formation_follows_lenz() {
        assert!((planetesimal_conversion_fraction(0.1, 0.25, 0.05) - 0.1).abs() < 1e-12);
        assert_eq!(planetesimal_conversion_fraction(0.8, 10.0, 0.05), 1.0);
        let critical = earths(critical_pebble_flux(0.1, 1.0));
        assert!((critical - 1.05e-8).abs() < 2e-10, "{critical}");
        assert!((earths(PLANETESIMAL_MASS_MSUN) - 1.05e-7).abs() < 1e-9);
        // 1 M☉/AU² is 8.89×10⁶ g/cm².
        assert!((MSUN_PER_AU2_IN_G_PER_CM2 / 8.887e6 - 1.0).abs() < 1e-3);
    }

    /// Ida & Lin (2004), eq. 6: 1.2×10⁵ years for an Earth-mass core at 1 AU
    /// in 10 g/cm² of 10¹⁸ g planetesimals and 2400 g/cm² of gas; with the
    /// heavier planetesimals used here, (628)^(2/15) = 2.36 times longer.
    #[test]
    fn planetesimal_accretion_matches_ida_and_lin() {
        let t = planetesimal_accretion_time_yr(10.0, 2.4e3, 1.0, EARTH_MASS_MSUN, 1.0);
        assert!(
            (t / (1.2e5 * 628f64.powf(2.0 / 15.0)) - 1.0).abs() < 1e-3,
            "{t}"
        );
        // Twice the solids, half the time; eight times the mass, twice as long.
        let double = planetesimal_accretion_time_yr(20.0, 2.4e3, 1.0, EARTH_MASS_MSUN, 1.0);
        assert!((t / double - 2.0).abs() < 1e-9);
        let heavy = planetesimal_accretion_time_yr(10.0, 2.4e3, 1.0, 8.0 * EARTH_MASS_MSUN, 1.0);
        assert!((heavy / t - 2.0).abs() < 1e-9);
        // Slower farther out and in thinner gas; never without solids or gas.
        assert!(planetesimal_accretion_time_yr(10.0, 2.4e3, 5.0, EARTH_MASS_MSUN, 1.0) > 2.0 * t);
        assert!(planetesimal_accretion_time_yr(10.0, 24.0, 1.0, EARTH_MASS_MSUN, 1.0) > 6.0 * t);
        assert!(
            planetesimal_accretion_time_yr(0.0, 2.4e3, 1.0, EARTH_MASS_MSUN, 1.0).is_infinite()
        );
        assert!(planetesimal_accretion_time_yr(10.0, 0.0, 1.0, EARTH_MASS_MSUN, 1.0).is_infinite());
    }
}
