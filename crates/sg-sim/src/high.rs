//! The High tier's late stage: the massive planets left when the gas
//! clears are integrated as an N-body system (PLAN.md §5.7, D52), in place
//! of the Standard tier's stability rule for them.
//!
//! Why only the massive ones. Giant planets have long orbits and strong
//! mutual pulls: packed too closely they scatter one another within
//! thousands to millions of years, which a few seconds of integration can
//! follow, and the outcome (one thrown out, the other left on an eccentric
//! orbit) is something no merging rule gives. Small planets close to the
//! star are the opposite case: orbits of days, and spacings so wide in
//! their own Hill radii that nothing happens in any span that could be
//! integrated (PLAN.md D50). They are left out of the integration, and
//! their fate is decided as in the Standard tier, except that those lying
//! in the path of a giant are removed.
//!
//! What this does not represent: resonances between the giants (the growth
//! model's chains are held by rule, and the integration starts from random
//! phases); the effect of the small planets and leftover planetesimals on
//! the giants; a companion star; tides; relativity. A system still intact
//! at the end of the span is reported as that, not as stable for ever.

use sg_core::constants::SOLAR_MASS_IN_EARTH_MASSES;
use sg_core::math::{cbrt, ln, sqrt};
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
    /// Swallowed by a giant whose path it lay in (small planets only).
    IntoGiant,
}

/// The state of the bodies at one moment: id, semi-major axis (AU),
/// eccentricity, mass (M☉).
pub(crate) type Frame = Vec<(u32, f64, f64, f64)>;

/// What the integration found.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LateStage {
    /// The planets that were integrated.
    pub active: Vec<u32>,
    /// The orbits and masses (M☉) of those that survived.
    pub orbits: Vec<(u32, Elements, f64)>,
    /// The planets left out of the integration and not removed by it: id,
    /// semi-major axis (AU), mass (M☉).
    pub passive: Vec<(u32, f64, f64)>,
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

/// A small planet within this many of a giant's Hill radii of the range of
/// distances the giant covers is removed: `2√3`, the width of the zone a
/// planet clears on a circular orbit (Gladman 1993, Icarus 106, 247).
const SWEPT_HILL: f64 = 3.464;

/// What becomes of a small planet in a giant's path: thrown out if the
/// giant's escape speed exceeds the orbital speed there, swallowed
/// otherwise (the Safronov number, as for leftover planetesimals in
/// `sg_dynamics::late::ring_fate`).
fn swept_fate(giant_mass: f64, giant_radius_au: f64, a_au: f64, star_mass: f64) -> Lost {
    let escape_over_orbital_sq = 2.0 * (giant_mass / star_mass) * (a_au / giant_radius_au);
    if escape_over_orbital_sq > 1.0 {
        Lost::Ejected
    } else {
        Lost::IntoGiant
    }
}

/// Integrates the massive planets of one disk for the span and step
/// budget of `params`, and removes the small planets left in their path.
/// `None` if fewer than two planets are massive enough: there is then
/// nothing to integrate. `frame_count` states are recorded on the way.
pub(crate) fn integrate(
    all: &[Start],
    star_mass_msun: f64,
    star_radius_au: f64,
    rng: &mut RngStream,
    params: &HighParams,
    frame_count: u32,
) -> Option<LateStage> {
    let threshold = params.active_mass_mearth / SOLAR_MASS_IN_EARTH_MASSES;
    let starts: Vec<Start> = all
        .iter()
        .copied()
        .filter(|s| s.mass_msun >= threshold)
        .collect();
    if starts.len() < 2 {
        return None;
    }
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
    let mut orbits: Vec<(u32, Elements, f64)> = integration
        .bodies
        .iter()
        .filter_map(|b| {
            let mu = G * (star_mass_msun + b.mass_msun);
            nbody::elements_from_state(b.pos, b.vel, mu).map(|el| (b.id, el, b.mass_msun))
        })
        .collect();
    orbits.sort_by(|x, y| x.1.a.0.total_cmp(&y.1.a.0));

    // The small planets: gone if a giant's orbit now passes their way.
    let mut passive = Vec::new();
    for small in all.iter().filter(|s| s.mass_msun < threshold) {
        let sweeper = orbits.iter().find(|(_, el, mass)| {
            // The Hill radius scales with distance from the star: it is
            // taken at the giant's nearest and farthest points.
            let hill = SWEPT_HILL * cbrt(mass / (3.0 * star_mass_msun));
            let nearest = el.a.0 * (1.0 - el.e);
            let farthest = el.a.0 * (1.0 + el.e);
            small.a_au > nearest * (1.0 - hill) && small.a_au < farthest * (1.0 + hill)
        });
        match sweeper {
            Some((giant, _, mass)) => {
                let radius = integration
                    .bodies
                    .iter()
                    .find(|b| b.id == *giant)
                    .map_or(small.radius_au, |b| b.radius_au);
                match swept_fate(*mass, radius, small.a_au, star_mass_msun) {
                    Lost::IntoGiant => mergers.push((span_yr, *giant, small.id)),
                    fate => lost.push((span_yr, small.id, fate)),
                }
            }
            None => passive.push((small.id, small.a_au, small.mass_msun)),
        }
    }
    Some(LateStage {
        active: starts.iter().map(|s| s.id).collect(),
        passive,
        orbits,
        mergers,
        lost,
        frames,
        integrated_yr: integration.t_yr.min(span_yr),
        span_yr,
        steps: integration.work.map_steps + integration.work.hermite_steps,
        energy_error,
    })
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
            // Giants are far less dense than rock: Jupiter is 11 Earth radii.
            radius_au: EARTH_RADIUS_AU
                * if mass_mearth > 50.0 {
                    11.0
                } else {
                    mass_mearth.cbrt()
                },
        }
    }

    fn params(integration_myr: f64, max_steps: u64) -> HighParams {
        HighParams {
            integration_myr,
            max_steps,
            ..HighParams::default()
        }
    }

    fn run(starts: &[Start], seed: u64, p: &HighParams, frames: u32) -> Option<LateStage> {
        let mut rng = RngStream::new(seed, ModuleId::DYNAMICS);
        integrate(starts, 1.0, 0.00465, &mut rng, p, frames)
    }

    #[test]
    fn nothing_is_integrated_without_two_massive_planets() {
        // Small planets only, and a lone giant among them.
        let small = [start(1, 0.1, 0.5), start(2, 0.2, 1.0), start(3, 1.0, 3.0)];
        assert!(run(&small, 1, &params(0.01, u64::MAX), 4).is_none());
        let one_giant = [start(1, 0.1, 0.5), start(2, 5.0, 300.0)];
        assert!(run(&one_giant, 1, &params(0.01, u64::MAX), 4).is_none());
    }

    #[test]
    fn jupiter_and_saturn_stay_and_the_terrestrial_planets_are_left_alone() {
        let starts = [
            start(1, 0.387, 0.055),
            start(2, 0.723, 0.815),
            start(3, 1.0, 1.0),
            start(4, 1.524, 0.107),
            start(5, 5.203, 317.8),
            start(6, 9.537, 95.2),
        ];
        let out = run(&starts, 7, &params(0.02, u64::MAX), 4).expect("two giants");
        assert_eq!(out.active, vec![5, 6]);
        assert_eq!(out.orbits.len(), 2);
        assert!(out.mergers.is_empty() && out.lost.is_empty());
        assert_eq!(out.integrated_yr, 20_000.0);
        for ((id, el, _), s) in out.orbits.iter().zip(&starts[4..]) {
            assert_eq!(*id, s.id);
            assert!((el.a.0 / s.a_au - 1.0).abs() < 0.02 && el.e < 0.12);
        }
        // The four small planets are passed through untouched.
        let ids: Vec<u32> = out.passive.iter().map(|p| p.0).collect();
        assert_eq!(ids, vec![1, 2, 3, 4]);
        assert!(out.energy_error < 1e-5, "{}", out.energy_error);
        assert_eq!(out.frames.len(), 4);
    }

    #[test]
    fn giants_packed_too_closely_scatter() {
        // Three Jupiters four mutual Hill radii apart: unstable within a
        // few thousand years (Chambers et al. 1996; Chatterjee et al.
        // 2008, ApJ 686, 580 for the outcome).
        let hill = (2.0 * 318.0 * MSUN_PER_MEARTH / 3.0f64).cbrt();
        let ratio = (1.0 + 2.0 * hill) / (1.0 - 2.0 * hill);
        let starts = [
            start(1, 5.0, 318.0),
            start(2, 5.0 * ratio, 318.0),
            start(3, 5.0 * ratio * ratio, 318.0),
            // An Earth between where the giants roam, and one far inside.
            start(4, 4.0, 1.0),
            start(5, 0.3, 1.0),
        ];
        let out = run(&starts, 3, &params(1.0, 30_000_000), 6).expect("giants");
        // Something gave: a giant lost or merged, or orbits made eccentric.
        let eccentric = out.orbits.iter().any(|(_, el, _)| el.e > 0.1);
        assert!(
            out.orbits.len() < 3 || eccentric,
            "nothing happened: {:?}",
            out.orbits
        );
        // The Earth among the giants is gone; the one at 0.3 AU is not
        // (unless a giant was thrown that far in).
        assert!(!out.passive.iter().any(|p| p.0 == 4));
        let innermost_reach = out
            .orbits
            .iter()
            .map(|(_, el, _)| el.a.0 * (1.0 - el.e))
            .fold(f64::INFINITY, f64::min);
        if innermost_reach > 0.5 {
            assert!(out.passive.iter().any(|p| p.0 == 5));
        }
        // A million years of three Jupiters throwing one another about,
        // through to an ejection: the energy error measured here is
        // 1.4–3.5 thousandths whatever the integrator's settings (tried:
        // encounter radius 3 and 4 Hill radii, Hermite accuracy 0.005 and
        // 0.01, 20 and 30 steps per orbit). That is the level at which
        // scattering experiments are usually accepted, not a high-accuracy
        // result.
        assert!(out.energy_error < 5e-3, "{}", out.energy_error);
    }

    #[test]
    fn a_small_planet_in_a_giants_path_is_thrown_out_or_swallowed() {
        // Jupiter at 5 AU throws things out; the same planet at 0.05 AU,
        // where orbits are fast, swallows them.
        let jupiter_radius = 11.0 * EARTH_RADIUS_AU;
        let jupiter_mass = 318.0 * MSUN_PER_MEARTH;
        assert_eq!(
            swept_fate(jupiter_mass, jupiter_radius, 5.0, 1.0),
            Lost::Ejected
        );
        assert_eq!(
            swept_fate(jupiter_mass, jupiter_radius, 0.05, 1.0),
            Lost::IntoGiant
        );
    }

    #[test]
    fn the_step_budget_stops_the_run_and_is_reported() {
        let starts = [start(1, 0.05, 300.0), start(2, 0.3, 300.0)];
        let out = run(&starts, 3, &params(1.0, 10_000), 5).expect("giants");
        assert!(out.steps <= 10_000 + 5);
        assert!(out.integrated_yr < 0.01 * out.span_yr);
        // Frames are still recorded, repeating the last state reached.
        assert_eq!(out.frames.len(), 5);
        assert_eq!(out.frames[3], out.frames[4]);
    }

    #[test]
    fn the_same_seed_gives_the_same_late_stage() {
        let starts = [
            start(1, 2.0, 200.0),
            start(2, 2.6, 300.0),
            start(3, 3.3, 100.0),
        ];
        let p = params(0.001, 2_000_000);
        assert_eq!(run(&starts, 5, &p, 3), run(&starts, 5, &p, 3));
    }
}
