//! The High tier's late stage: the planets left when the gas clears are
//! integrated as an N-body system (PLAN.md §5.7), in place of the Standard
//! tier's stability rule.
//!
//! What this does and does not represent:
//!
//! - The integration starts from the growth model's planets on nearly
//!   circular, nearly coplanar orbits with random phases. The growth
//!   model holds neighbours in resonant chains by rule, not by dynamics,
//!   so the chains are **not** in true resonance here: their fate is that
//!   of closely packed planets without resonant protection. Simulations
//!   that do follow the resonances find that most chains break after the
//!   gas goes anyway (Izidoro et al. 2017, MNRAS 470, 1750: about nine in
//!   ten), so the outcome is in the observed direction, for a cruder
//!   reason.
//! - The span integrated is limited by a step budget. Close-in planets
//!   complete an orbit in days, so a million years of them is far beyond
//!   an interactive run; what is integrated is the first part of the late
//!   stage, where closely packed systems go unstable (the time to
//!   instability grows exponentially with spacing: Chambers, Wetherill &
//!   Boss 1996, Icarus 119, 261). Systems still intact at the end are
//!   reported as such, not as stable for ever.
//! - Only the central mass's gravity and the planets' own: no companion
//!   star, no tides, no relativity, no leftover planetesimals.

use sg_core::math::{ln, sqrt};
use sg_core::rng::RngStream;
use sg_core::units::Au;
use sg_dynamics::kepler::Elements;
use sg_dynamics::nbody::{self, Body, Event, Integration, Settings, G};
use sg_schema::HighParams;
use std::f64::consts::TAU;

/// A planet handed to the integration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Start {
    pub id: u32,
    pub a_au: f64,
    pub mass_msun: f64,
    pub radius_au: f64,
}

/// How a body left the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lost {
    IntoStar,
    Ejected,
}

/// The state of the bodies at one moment: id, semi-major axis (AU),
/// eccentricity, mass (M☉).
pub(crate) type Frame = Vec<(u32, f64, f64, f64)>;

/// What the integration found.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LateStage {
    /// The survivors' orbits.
    pub orbits: Vec<(u32, Elements)>,
    /// Mergers in order, with the time since the start, years.
    pub mergers: Vec<(f64, u32, u32)>,
    pub lost: Vec<(f64, u32, Lost)>,
    /// The system at evenly spaced times across the span asked for.
    pub frames: Vec<Frame>,
    /// Years actually integrated, and asked for.
    pub integrated_yr: f64,
    pub span_yr: f64,
    pub steps: u64,
    /// Largest relative energy error seen, after allowing for what
    /// mergers and losses removed.
    pub energy_error: f64,
}

fn rayleigh(rng: &mut RngStream, sigma: f64) -> f64 {
    sigma * sqrt(-2.0 * ln(1.0 - rng.uniform()))
}

fn frame_of(integration: &Integration) -> Frame {
    let mut frame: Frame = integration
        .bodies
        .iter()
        .filter_map(|b| {
            let mu = G * (integration.star_mass_msun + b.mass_msun);
            nbody::elements_from_state(b.pos, b.vel, mu).map(|el| (b.id, el.a.0, el.e, b.mass_msun))
        })
        .collect();
    frame.sort_by_key(|b| b.0);
    frame
}

/// Integrates the planets of one disk for the span and step budget of
/// `params`. `frame_count` states are recorded on the way.
pub(crate) fn integrate(
    starts: &[Start],
    star_mass_msun: f64,
    star_radius_au: f64,
    rng: &mut RngStream,
    params: &HighParams,
    frame_count: u32,
) -> LateStage {
    // Orbits as the gas leaves them: damped, so nearly circular and flat.
    // Eccentricities and inclinations are Rayleigh distributed, as for a
    // relaxed swarm (Ida & Makino 1992, Icarus 96, 107).
    let bodies: Vec<Body> = starts
        .iter()
        .map(|s| {
            let elements = Elements {
                a: Au(s.a_au),
                e: rayleigh(rng, params.initial_eccentricity).min(0.3),
                i: rayleigh(rng, params.initial_inclination_deg.to_radians()).min(0.3),
                long_asc_node: rng.uniform_range(0.0, TAU),
                arg_periapsis: rng.uniform_range(0.0, TAU),
                mean_anomaly: rng.uniform_range(0.0, TAU),
            };
            let (pos, vel) =
                nbody::state_from_elements(elements, G * (star_mass_msun + s.mass_msun));
            Body {
                id: s.id,
                mass_msun: s.mass_msun,
                radius_au: s.radius_au,
                pos,
                vel,
            }
        })
        .collect();
    let span_yr = params.integration_myr * 1.0e6;
    let mut integration =
        Integration::new(star_mass_msun, star_radius_au, bodies, Settings::default());
    let energy_start = integration.energy();
    let mut energy_error = 0.0f64;
    let mut frames = Vec::with_capacity(frame_count as usize);
    let mut budget = params.max_steps;
    for k in 1..=frame_count {
        let until = span_yr * f64::from(k) / f64::from(frame_count);
        let before = integration.work.map_steps + integration.work.hermite_steps;
        integration.advance(until, budget);
        let used = integration.work.map_steps + integration.work.hermite_steps - before;
        budget = budget.saturating_sub(used);
        if energy_start != 0.0 {
            let now = integration.energy() + integration.energy_removed;
            energy_error = energy_error.max((now / energy_start - 1.0).abs());
        }
        frames.push(frame_of(&integration));
    }

    let mut mergers = Vec::new();
    let mut lost = Vec::new();
    for event in &integration.events {
        match *event {
            Event::Merged {
                t_yr,
                survivor,
                absorbed,
                ..
            } => mergers.push((t_yr, survivor, absorbed)),
            Event::HitStar { t_yr, id } => lost.push((t_yr, id, Lost::IntoStar)),
            Event::Ejected { t_yr, id } => lost.push((t_yr, id, Lost::Ejected)),
        }
    }
    let mut orbits: Vec<(u32, Elements)> = integration
        .bodies
        .iter()
        .filter_map(|b| {
            let mu = G * (star_mass_msun + b.mass_msun);
            nbody::elements_from_state(b.pos, b.vel, mu).map(|el| (b.id, el))
        })
        .collect();
    orbits.sort_by(|x, y| x.1.a.0.total_cmp(&y.1.a.0));
    LateStage {
        orbits,
        mergers,
        lost,
        frames,
        integrated_yr: integration.t_yr.min(span_yr),
        span_yr,
        steps: integration.work.map_steps + integration.work.hermite_steps,
        energy_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sg_core::rng::ModuleId;

    const MSUN_PER_MEARTH: f64 = 3.003e-6;
    const EARTH_RADIUS_AU: f64 = 4.2635e-5;

    fn start(id: u32, a_au: f64, mass_mearth: f64) -> Start {
        Start {
            id,
            a_au,
            mass_msun: mass_mearth * MSUN_PER_MEARTH,
            radius_au: EARTH_RADIUS_AU * mass_mearth.cbrt(),
        }
    }

    fn params(integration_myr: f64, max_steps: u64) -> HighParams {
        HighParams {
            integration_myr,
            max_steps,
            ..HighParams::default()
        }
    }

    #[test]
    fn well_spaced_planets_survive_unchanged() {
        // Venus, Earth and Mars: tens of Hill radii apart.
        let starts = [
            start(1, 0.723, 0.815),
            start(2, 1.0, 1.0),
            start(3, 1.524, 0.107),
        ];
        let mut rng = RngStream::new(7, ModuleId::DYNAMICS);
        let out = integrate(&starts, 1.0, 0.00465, &mut rng, &params(0.002, u64::MAX), 4);
        assert_eq!(out.orbits.len(), 3);
        assert!(out.mergers.is_empty() && out.lost.is_empty());
        assert_eq!(out.integrated_yr, 2000.0);
        assert_eq!(out.frames.len(), 4);
        for ((id, el), s) in out.orbits.iter().zip(&starts) {
            assert_eq!(*id, s.id);
            assert!((el.a.0 / s.a_au - 1.0).abs() < 0.01 && el.e < 0.1);
        }
        assert!(out.energy_error < 1e-6, "{}", out.energy_error);
    }

    #[test]
    fn a_packed_chain_of_small_planets_merges_into_fewer() {
        // Eight half-Earths five mutual Hill radii apart near 0.1 AU:
        // unstable within thousands of orbits (Chambers et al. 1996).
        let hill = (2.0 * 0.5 * MSUN_PER_MEARTH / 3.0f64).cbrt();
        let ratio = (1.0 + 2.5 * hill) / (1.0 - 2.5 * hill);
        let starts: Vec<Start> = (0..8)
            .map(|k| start(k, 0.1 * ratio.powi(k as i32), 0.5))
            .collect();
        let mass_before: f64 = starts.iter().map(|s| s.mass_msun).sum();
        let mut rng = RngStream::new(11, ModuleId::DYNAMICS);
        let out = integrate(
            &starts,
            1.0,
            0.00465,
            &mut rng,
            &params(0.02, 60_000_000),
            8,
        );
        assert!(!out.mergers.is_empty(), "nothing merged: {out:?}");
        assert!(out.orbits.len() < 8);
        // Mass is conserved among survivors and what was lost.
        let survivors: f64 = out.frames.last().unwrap().iter().map(|b| b.3).sum();
        assert!(survivors <= mass_before * (1.0 + 1e-12));
        if out.lost.is_empty() {
            assert!((survivors / mass_before - 1.0).abs() < 1e-12);
        }
        // Merger times are in order and within the span.
        for pair in out.mergers.windows(2) {
            assert!(pair[0].0 <= pair[1].0);
        }
        assert!(out.energy_error < 1e-3, "{}", out.energy_error);
    }

    #[test]
    fn the_step_budget_stops_the_run_and_is_reported() {
        let starts = [start(1, 0.05, 1.0), start(2, 0.3, 1.0)];
        let mut rng = RngStream::new(3, ModuleId::DYNAMICS);
        let out = integrate(&starts, 1.0, 0.00465, &mut rng, &params(1.0, 10_000), 5);
        assert!(out.steps <= 10_000 + 5);
        assert!(out.integrated_yr < 0.01 * out.span_yr);
        // Frames are still recorded, repeating the last state reached.
        assert_eq!(out.frames.len(), 5);
        assert_eq!(out.frames[3], out.frames[4]);
    }

    #[test]
    fn the_same_seed_gives_the_same_late_stage() {
        let starts = [start(1, 0.2, 2.0), start(2, 0.26, 3.0), start(3, 0.33, 1.0)];
        let run = || {
            let mut rng = RngStream::new(5, ModuleId::DYNAMICS);
            integrate(&starts, 0.8, 0.004, &mut rng, &params(0.001, 2_000_000), 3)
        };
        assert_eq!(run(), run());
    }
}
