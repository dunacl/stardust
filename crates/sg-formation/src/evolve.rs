//! Integrates a set of embryos through the life of the gas disk.
//!
//! Each step, working inward from the outermost body:
//!
//! 1. **Pebbles.** The pebble flux drifts inward. Every body takes its share
//!    ([`physics::pebble_accretion_rate`]) and passes on the rest; the flux
//!    loses its ice at the ice line; and a body that has reached the pebble
//!    isolation mass lets nothing past, starving everything inside its orbit
//!    (Lambrechts et al. 2019, A&A 627, A83).
//! 2. **Gas.** A body past isolation contracts its envelope and, once the
//!    envelope outweighs the core, accretes gas in a runaway.
//! 3. **Migration.** Bodies move by the disk's torque. Neighbours that
//!    converge are caught in a mean-motion resonance and from then on move
//!    as a chain, at the rate their summed torques give; a chain stops when
//!    its innermost member reaches the disk's inner edge. Neighbours too
//!    massive for any resonance to keep apart merge.
//!
//! When the gas has gone, bodies that never entered runaway are given the
//! envelope such a core can cool and hold while embedded in the disk
//! (Ginzburg, Schlichting & Sari 2016).
//!
//! Simplifications: orbits are circular and coplanar while gas is present;
//! resonance capture is certain and only the 3:2 and 2:1 resonances are
//! used; mergers are perfect; planetesimal accretion is not included.

use crate::physics::{self, EARTH_MASS_MSUN, ICE_MASS_FRACTION};
use sg_core::math::{exp, pow, sqrt};
use sg_disk::Disk;

/// Parameters of the growth model.
#[derive(Debug, Clone, PartialEq)]
pub struct GrowthParams {
    /// Turbulent α at the midplane: sets the pebble layer's thickness, the
    /// isolation mass and gap depth. Lower than the α of the disk's global
    /// evolution, as in Johansen, Ida & Brasser (2019, A&A 622, A202).
    pub alpha_turbulent: f64,
    /// Stokes number of drift-limited pebbles.
    pub stokes_drift: f64,
    /// α that refills a planet's gap: sets gap depth, and through it the
    /// slowing of migration and of gas accretion (Kanagawa et al. 2018).
    pub alpha_gap: f64,
    /// Whether bodies migrate.
    pub migration: bool,
    /// Factor applied to the migration rate. Below 1 it stands in for
    /// torques this model leaves out, which oppose the inward drift of
    /// low-mass planets (the corotation torque, Paardekooper et al. 2011;
    /// the heating torque, Benítez-Llambay et al. 2015). Population models
    /// have long needed such a factor of 0.01–0.1 to keep planets from
    /// falling into their stars (Ida & Lin 2008, ApJ 673, 487).
    pub migration_efficiency: f64,
}

impl Default for GrowthParams {
    fn default() -> Self {
        Self {
            alpha_turbulent: 1.0e-4,
            stokes_drift: 0.05,
            alpha_gap: 1.0e-4,
            migration: true,
            migration_efficiency: 1.0,
        }
    }
}

/// Where and when an embryo appears.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seed {
    pub a_au: f64,
    pub start_yr: f64,
}

/// A growing or finished body.
#[derive(Debug, Clone, PartialEq)]
pub struct Embryo {
    /// Index of its seed.
    pub id: u32,
    pub a_au: f64,
    /// Rock, metal and ice, M☉.
    pub core_msun: f64,
    /// Hydrogen and helium, M☉.
    pub envelope_msun: f64,
    /// Part of the core that is water ice, M☉.
    pub water_msun: f64,
    /// Where its seed appeared, AU.
    pub origin_a_au: f64,
    pub start_yr: f64,
    /// Reached the pebble isolation mass.
    pub isolated: bool,
    /// Entered runaway gas accretion.
    pub runaway: bool,
    /// Opened a deep gap (gas density halved).
    pub gap: bool,
    /// Other embryos merged into it.
    pub mergers: u32,
    /// Caught behind the body with this id, at this period ratio.
    pub resonance: Option<(u32, (u8, u8))>,
}

impl Embryo {
    pub fn mass_msun(&self) -> f64 {
        self.core_msun + self.envelope_msun
    }
}

/// Something that happened, with its time in years.
#[derive(Debug, Clone, PartialEq)]
pub enum GrowthEvent {
    Seeded {
        id: u32,
        a_au: f64,
    },
    IsolationReached {
        id: u32,
    },
    RunawayGas {
        id: u32,
    },
    GapOpened {
        id: u32,
    },
    ResonanceCaptured {
        inner: u32,
        outer: u32,
        ratio: (u8, u8),
    },
    Merged {
        survivor: u32,
        absorbed: u32,
    },
    ReachedInnerEdge {
        id: u32,
    },
}

/// State of every body at a requested time.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub t_yr: f64,
    /// `(id, a_au, mass_msun)`, ordered by id.
    pub bodies: Vec<(u32, f64, f64)>,
    pub ice_line_au: f64,
}

/// The result of [`evolve`].
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// Surviving bodies, ordered outward.
    pub planets: Vec<Embryo>,
    pub events: Vec<(f64, GrowthEvent)>,
    pub frames: Vec<Frame>,
    /// Integration steps taken.
    pub steps: u64,
}

/// Longest step, years.
const MAX_STEP_YR: f64 = 1.0e4;
/// Shortest step, years. With the fraction below it bounds the work.
const MIN_STEP_YR: f64 = 20.0;
/// A step changes no mass or orbit by more than this fraction.
const STEP_FRACTION: f64 = 0.03;
/// Smallest seed, M☉: a tenth of a lunar mass.
const MIN_SEED_MSUN: f64 = 1.0e-3 * EARTH_MASS_MSUN;
/// Largest envelope, as a fraction of the core, that a core holds without
/// having entered runaway: at equal masses the runaway begins.
const MAX_EMBEDDED_ENVELOPE: f64 = 0.5;
/// A gap is called open when the gas density in it has halved: `K = 25`.
const GAP_OPEN_K: f64 = 25.0;
/// Neighbours closer than this many mutual Hill radii cannot stay apart
/// (Gladman 1993, Icarus 106, 247: `2√3`).
const HILL_STABILITY: f64 = 3.464;
/// A resonance holds a pair only if it keeps them at least this many
/// mutual Hill radii apart; otherwise the next wider one is used.
const RESONANCE_MIN_HILL: f64 = 5.0;

/// First-order resonances a converging pair can be caught in, widest
/// first: `(a_outer/a_inner, (p, q))` for the p:q period ratio.
fn resonances() -> [(f64, (u8, u8)); 4] {
    [
        (pow(2.0, 2.0 / 3.0), (2, 1)),
        (pow(1.5, 2.0 / 3.0), (3, 2)),
        (pow(4.0 / 3.0, 2.0 / 3.0), (4, 3)),
        (pow(1.25, 2.0 / 3.0), (5, 4)),
    ]
}

fn mutual_hill(a_in: f64, a_out: f64, m_in: f64, m_out: f64, star: f64) -> f64 {
    0.5 * (a_in + a_out) * pow((m_in + m_out) / (3.0 * star), 1.0 / 3.0)
}

/// Spacing, in mutual Hill radii, at which a resonance would hold a pair.
fn spacing_at(ratio: f64, a_in: f64, m_in: f64, m_out: f64, star: f64) -> f64 {
    let a_out = a_in * ratio;
    (a_out - a_in) / mutual_hill(a_in, a_out, m_in, m_out, star)
}

/// The resonances that can catch a pair, in the order tried.
///
/// Light pairs slip through the 2:1 and are caught in the 3:2, the commonest
/// outcome for super-Earths migrating at type I rates (Ogihara & Kobayashi
/// 2013); a pair too massive for the 3:2 to keep `RESONANCE_MIN_HILL` apart
/// is caught in the 2:1 instead. The tighter 4:3 and 5:4 catch pairs that
/// start out inside the 3:2.
fn capture_candidates(a_in: f64, m_in: f64, m_out: f64, star: f64) -> Vec<(f64, (u8, u8))> {
    let [two_one, three_two, four_three, five_four] = resonances();
    let holds =
        |r: &(f64, (u8, u8))| spacing_at(r.0, a_in, m_in, m_out, star) >= RESONANCE_MIN_HILL;
    let mut order = Vec::with_capacity(4);
    if !holds(&three_two) {
        order.push(two_one);
    }
    for resonance in [three_two, four_three, five_four] {
        if holds(&resonance) {
            order.push(resonance);
        }
    }
    order
}

/// Envelope a core that never reached isolation holds when the gas clears,
/// as a fraction of its mass.
///
/// Ginzburg, Schlichting & Sari (2016, ApJ 825, 29), eq. 24:
/// `f ≈ 0.02 (M_c/M⊕)^0.8 (T/1000 K)^-0.25 (t_disk/1 Myr)^0.5`, for a core
/// embedded in the disk for `t_disk`. Capped well short of the equal
/// masses at which runaway begins.
///
/// The relation was derived for super-Earth cores. Smaller bodies gather
/// only wisps of gas that they do not keep, so the fraction is tapered to
/// zero between 1 and 0.5 M⊕; that taper is this model's, not the paper's.
pub fn embedded_envelope_fraction(core_msun: f64, temperature_k: f64, disk_life_yr: f64) -> f64 {
    let core = core_msun / EARTH_MASS_MSUN;
    let f = 0.02 * pow(core, 0.8) * pow(temperature_k / 1000.0, -0.25) * sqrt(disk_life_yr / 1.0e6);
    let taper = ((core - 0.5) / 0.5).clamp(0.0, 1.0);
    taper * f.min(MAX_EMBEDDED_ENVELOPE)
}

/// Runs the growth model from the disk's formation until its gas is gone.
/// `frame_times_yr` must be ascending.
pub fn evolve(
    disk: &Disk,
    params: &GrowthParams,
    seeds: &[Seed],
    frame_times_yr: &[f64],
) -> Outcome {
    let star = disk.spec().star_mass_msun;
    let inner_edge = disk.spec().inner_edge_au;
    let end = disk.gas_gone_yr();
    let alpha = params.alpha_turbulent;

    let mut bodies: Vec<Embryo> = Vec::new();
    let mut pending: Vec<(u32, Seed)> = seeds
        .iter()
        .copied()
        .zip(0u32..)
        .map(|(s, i)| (i, s))
        .collect();
    pending.sort_by(|x, y| x.1.start_yr.total_cmp(&y.1.start_yr).then(x.0.cmp(&y.0)));
    let mut next_seed = 0usize;
    let mut events: Vec<(f64, GrowthEvent)> = Vec::new();
    let mut frames = Vec::with_capacity(frame_times_yr.len());
    let mut next_frame = 0usize;
    let mut steps = 0u64;
    let mut t = 0.0f64;

    let frame = |t_yr: f64, bodies: &[Embryo]| {
        let mut list: Vec<(u32, f64, f64)> = bodies
            .iter()
            .map(|b| (b.id, b.a_au, b.mass_msun()))
            .collect();
        list.sort_by_key(|b| b.0);
        Frame {
            t_yr,
            bodies: list,
            ice_line_au: disk.water_ice_line(t_yr),
        }
    };

    while t < end {
        // Frames due at or before this time.
        while next_frame < frame_times_yr.len() && frame_times_yr[next_frame] <= t {
            frames.push(frame(frame_times_yr[next_frame], &bodies));
            next_frame += 1;
        }
        // Seeds due.
        while next_seed < pending.len() && pending[next_seed].1.start_yr <= t {
            let (id, seed) = pending[next_seed];
            next_seed += 1;
            if seed.a_au <= inner_edge || seed.a_au >= disk.spec().outer_edge_au {
                continue;
            }
            let mass = physics::transition_mass(disk, seed.a_au, t).max(MIN_SEED_MSUN);
            bodies.push(Embryo {
                id,
                a_au: seed.a_au,
                core_msun: mass,
                envelope_msun: 0.0,
                water_msun: 0.0,
                origin_a_au: seed.a_au,
                start_yr: t,
                isolated: false,
                runaway: false,
                gap: false,
                mergers: 0,
                resonance: None,
            });
            events.push((
                t,
                GrowthEvent::Seeded {
                    id,
                    a_au: seed.a_au,
                },
            ));
        }
        bodies.sort_by(|x, y| x.a_au.total_cmp(&y.a_au).then(x.id.cmp(&y.id)));

        // Rates, outermost body first so the pebble flux is passed inward.
        let ice_line = disk.water_ice_line(t);
        let mut flux = physics::pebble_flux(disk, t);
        let mut past_ice_line = false;
        let n = bodies.len();
        let mut solid_rate = vec![0.0f64; n];
        let mut gas_rate = vec![0.0f64; n];
        let mut drift_rate = vec![0.0f64; n];
        for i in (0..n).rev() {
            let b = &bodies[i];
            if !past_ice_line && b.a_au < ice_line {
                flux *= 1.0 - ICE_MASS_FRACTION;
                past_ice_line = true;
            }
            if b.isolated {
                // Its pressure bump holds back all pebbles from outside.
                flux = 0.0;
                // Contraction first, a runaway once the envelope outweighs
                // the core; either way no faster than the gap lets gas in.
                let supply = physics::runaway_accretion_rate(
                    disk,
                    b.a_au,
                    t,
                    b.mass_msun(),
                    params.alpha_gap,
                );
                gas_rate[i] = if b.envelope_msun < b.core_msun {
                    physics::envelope_contraction_rate(b.mass_msun()).min(supply)
                } else {
                    supply
                };
            } else {
                let stokes =
                    physics::stokes_number(disk, b.a_au, t, params.stokes_drift, alpha, ice_line);
                let rate = physics::pebble_accretion_rate(
                    disk,
                    b.a_au,
                    t,
                    b.mass_msun(),
                    flux,
                    stokes,
                    alpha,
                );
                solid_rate[i] = rate;
                flux -= rate;
            }
            if params.migration && b.a_au > inner_edge {
                drift_rate[i] = params.migration_efficiency
                    * physics::migration_rate(disk, b.a_au, t, b.mass_msun(), params.alpha_gap)
                    / b.a_au;
            }
        }

        // Chains: neighbours held in resonance move together, at the rate
        // their summed torques give (weights are orbital angular momenta).
        let mut chain_of = vec![0usize; n];
        let mut chains = 0usize;
        for i in 0..n {
            let linked = i > 0
                && bodies[i].resonance.map(|(inner, _)| inner) == Some(bodies[i - 1].id)
                && drift_rate[i] <= drift_rate[i - 1];
            if !linked {
                chains += 1;
            }
            chain_of[i] = chains - 1;
        }
        let mut chain_rate = vec![0.0f64; chains];
        let mut chain_weight = vec![0.0f64; chains];
        let mut chain_anchored = vec![false; chains];
        for i in 0..n {
            let weight = bodies[i].mass_msun() * sqrt(bodies[i].a_au);
            chain_rate[chain_of[i]] += weight * drift_rate[i];
            chain_weight[chain_of[i]] += weight;
            if bodies[i].a_au <= inner_edge {
                chain_anchored[chain_of[i]] = true;
            }
        }
        for c in 0..chains {
            chain_rate[c] = if chain_anchored[c] || chain_weight[c] <= 0.0 {
                0.0
            } else {
                chain_rate[c] / chain_weight[c]
            };
        }

        // Step: small enough that nothing changes much, and landing exactly
        // on the next seed, frame and the end.
        let mut dt = MAX_STEP_YR;
        for i in 0..n {
            let growth = (solid_rate[i] + gas_rate[i]) / bodies[i].mass_msun();
            let motion = chain_rate[chain_of[i]].abs();
            let fastest = growth.max(motion);
            if fastest > 0.0 {
                dt = dt.min(STEP_FRACTION / fastest);
            }
        }
        dt = dt.max(MIN_STEP_YR);
        if next_seed < pending.len() {
            dt = dt.min((pending[next_seed].1.start_yr - t).max(MIN_STEP_YR.min(1.0)));
        }
        if next_frame < frame_times_yr.len() {
            dt = dt.min((frame_times_yr[next_frame] - t).max(1.0));
        }
        dt = dt.min(end - t).max(1.0e-6);

        // Apply growth and motion.
        for i in 0..n {
            let b = &mut bodies[i];
            let solids = solid_rate[i] * dt;
            b.core_msun += solids;
            if b.a_au >= ice_line {
                b.water_msun += ICE_MASS_FRACTION * solids;
            }
            if b.isolated {
                let was_contracting = b.envelope_msun < b.core_msun;
                b.envelope_msun += gas_rate[i] * dt;
                if was_contracting && b.envelope_msun >= b.core_msun && !b.runaway {
                    b.runaway = true;
                    events.push((t, GrowthEvent::RunawayGas { id: b.id }));
                }
            } else if b.mass_msun() >= physics::pebble_isolation_mass(disk, b.a_au, t, alpha) {
                b.isolated = true;
                events.push((t, GrowthEvent::IsolationReached { id: b.id }));
            }
            if !b.gap
                && physics::gap_parameter(disk, b.a_au, t, b.mass_msun(), params.alpha_gap)
                    > GAP_OPEN_K
            {
                b.gap = true;
                events.push((t, GrowthEvent::GapOpened { id: b.id }));
            }
            let was_outside = b.a_au > inner_edge;
            b.a_au = (b.a_au * exp(chain_rate[chain_of[i]] * dt)).max(inner_edge);
            if was_outside && b.a_au <= inner_edge {
                events.push((t, GrowthEvent::ReachedInnerEdge { id: b.id }));
            }
        }

        // Encounters between neighbours, inner pair first.
        bodies.sort_by(|x, y| x.a_au.total_cmp(&y.a_au).then(x.id.cmp(&y.id)));
        let mut i = 1;
        while i < bodies.len() {
            let (m_in, m_out) = (bodies[i - 1].mass_msun(), bodies[i].mass_msun());
            let (a_in, a_out) = (bodies[i - 1].a_au, bodies[i].a_au);
            let spacing = (a_out - a_in) / mutual_hill(a_in, a_out, m_in, m_out, star);
            let held = bodies[i]
                .resonance
                .filter(|(inner, _)| *inner == bodies[i - 1].id);
            // Too close for anything to keep them apart.
            let merge = spacing < HILL_STABILITY;
            if merge {
                let absorbed = bodies.remove(i);
                let survivor = &mut bodies[i - 1];
                let total = survivor.mass_msun() + absorbed.mass_msun();
                // Orbital angular momentum of two circular orbits is kept.
                let l = survivor.mass_msun() * sqrt(survivor.a_au)
                    + absorbed.mass_msun() * sqrt(absorbed.a_au);
                survivor.a_au = pow(l / total, 2.0).max(inner_edge);
                // The identity of the heavier body survives.
                let mut gone = absorbed.id;
                if absorbed.mass_msun() > survivor.mass_msun() {
                    gone = survivor.id;
                    survivor.id = absorbed.id;
                    survivor.origin_a_au = absorbed.origin_a_au;
                    survivor.start_yr = absorbed.start_yr;
                }
                survivor.core_msun += absorbed.core_msun;
                survivor.envelope_msun += absorbed.envelope_msun;
                survivor.water_msun += absorbed.water_msun;
                survivor.isolated |= absorbed.isolated;
                survivor.runaway |= absorbed.runaway;
                survivor.gap |= absorbed.gap;
                survivor.mergers += absorbed.mergers + 1;
                survivor.resonance = None;
                let survivor_id = survivor.id;
                events.push((
                    t,
                    GrowthEvent::Merged {
                        survivor: survivor_id,
                        absorbed: gone,
                    },
                ));
                // The body outside, if it was held by either, is released.
                if let Some(next) = bodies.get_mut(i) {
                    next.resonance = None;
                }
                continue;
            }
            // A pair is caught as it converges across a resonance that can
            // hold it: only if it has just crossed (a step moves an orbit by
            // at most `STEP_FRACTION`), so bodies that start out closer than
            // a resonance are not thrown outward onto it.
            if held.is_none() {
                let room = bodies.get(i + 1).map_or(f64::INFINITY, |next| next.a_au);
                for (ratio, pq) in capture_candidates(a_in, m_in, m_out, star) {
                    let just_crossed =
                        a_out < a_in * ratio && a_out >= a_in * ratio * (1.0 - 2.0 * STEP_FRACTION);
                    if just_crossed && a_in * ratio < room {
                        bodies[i].a_au = a_in * ratio;
                        bodies[i].resonance = Some((bodies[i - 1].id, pq));
                        events.push((
                            t,
                            GrowthEvent::ResonanceCaptured {
                                inner: bodies[i - 1].id,
                                outer: bodies[i].id,
                                ratio: pq,
                            },
                        ));
                        break;
                    }
                }
            }
            i += 1;
        }

        t += dt;
        steps += 1;
    }
    while next_frame < frame_times_yr.len() {
        frames.push(frame(frame_times_yr[next_frame].min(end), &bodies));
        next_frame += 1;
    }

    // A core that never entered runaway keeps only the envelope a core can
    // cool and hold while embedded. (Until runaway the envelope tracked
    // above is a clock for its onset, not a prediction of its mass: the
    // contraction fit is calibrated for cores near 5 AU and overestimates
    // what small, hot, close-in cores retain.)
    for b in &mut bodies {
        if !b.runaway {
            let embedded_yr = (disk.spec().dispersal_time_yr - b.start_yr).max(0.0);
            let temperature = disk.temperature(b.a_au, disk.spec().dispersal_time_yr);
            b.envelope_msun =
                b.core_msun * embedded_envelope_fraction(b.core_msun, temperature, embedded_yr);
        }
    }
    bodies.sort_by(|x, y| x.a_au.total_cmp(&y.a_au).then(x.id.cmp(&y.id)));

    Outcome {
        planets: bodies,
        events,
        frames,
        steps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sg_disk::DiskSpec;

    fn disk_with(mass: f64, dispersal_yr: f64) -> Disk {
        Disk::new(DiskSpec {
            star_mass_msun: 1.0,
            star_luminosity_lsun: 1.0,
            initial_mass_msun: mass,
            characteristic_radius_au: 40.0,
            alpha: 5.0e-3,
            dispersal_time_yr: dispersal_yr,
            dust_to_gas: 0.0149,
            inner_edge_au: 0.05,
            outer_edge_au: 1000.0,
        })
        .unwrap()
    }

    fn earths(msun: f64) -> f64 {
        msun / EARTH_MASS_MSUN
    }

    fn seeds(list: &[(f64, f64)]) -> Vec<Seed> {
        list.iter()
            .map(|(a_au, start_yr)| Seed {
                a_au: *a_au,
                start_yr: *start_yr,
            })
            .collect()
    }

    #[test]
    fn no_seeds_no_planets() {
        let outcome = evolve(
            &disk_with(0.05, 3.0e6),
            &GrowthParams::default(),
            &[],
            &[1.0e6],
        );
        assert!(outcome.planets.is_empty() && outcome.events.is_empty());
        assert_eq!(outcome.frames.len(), 1);
    }

    #[test]
    fn the_run_is_repeatable_and_bounded() {
        let disk = disk_with(0.08, 3.0e6);
        let s = seeds(&[(1.0, 1.0e5), (5.0, 2.0e5), (12.0, 1.0e5), (25.0, 4.0e5)]);
        let frames: Vec<f64> = (1..=10).map(|k| f64::from(k) * 3.3e5).collect();
        let a = evolve(&disk, &GrowthParams::default(), &s, &frames);
        assert_eq!(a, evolve(&disk, &GrowthParams::default(), &s, &frames));
        assert_eq!(a.frames.len(), 10);
        assert!(a.steps < 300_000, "{}", a.steps);
        for planet in &a.planets {
            assert!(planet.a_au >= 0.05 && planet.a_au.is_finite());
            assert!(planet.core_msun > 0.0 && planet.envelope_msun >= 0.0);
            assert!(planet.water_msun >= 0.0 && planet.water_msun <= planet.core_msun);
        }
        assert!(a.planets.windows(2).all(|w| w[0].a_au <= w[1].a_au));
    }

    /// Without migration a seed stays where it was put, grows, and stops at
    /// the pebble isolation mass if the disk lives long enough.
    #[test]
    fn a_lone_seed_grows_to_isolation_in_place() {
        let disk = disk_with(0.08, 3.0e6);
        let params = GrowthParams {
            migration: false,
            ..GrowthParams::default()
        };
        let outcome = evolve(&disk, &params, &seeds(&[(8.0, 1.0e5)]), &[]);
        let planet = &outcome.planets[0];
        assert_eq!(planet.a_au, 8.0);
        assert!(planet.isolated);
        assert!(
            earths(planet.core_msun) > 3.0,
            "{}",
            earths(planet.core_msun)
        );
        assert!(planet.water_msun > 0.3 * planet.core_msun);
        assert!(outcome
            .events
            .iter()
            .any(|(_, e)| matches!(e, GrowthEvent::IsolationReached { id: 0 })));
    }

    /// The same seed in a disk that clears early stays small, with only a
    /// thin envelope.
    #[test]
    fn a_short_lived_disk_leaves_a_small_core() {
        let params = GrowthParams {
            migration: false,
            ..GrowthParams::default()
        };
        let long = evolve(
            &disk_with(0.03, 3.0e6),
            &params,
            &seeds(&[(3.0, 2.0e5)]),
            &[],
        );
        let short = evolve(
            &disk_with(0.03, 3.0e5),
            &params,
            &seeds(&[(3.0, 2.0e5)]),
            &[],
        );
        let (long, short) = (&long.planets[0], &short.planets[0]);
        assert!(short.mass_msun() < long.mass_msun());
        assert!(!short.isolated);
        assert!(short.envelope_msun <= 0.5 * short.core_msun);
    }

    /// A massive disk turns a well-placed seed into a gas giant.
    #[test]
    fn a_massive_disk_makes_a_gas_giant() {
        let disk = disk_with(0.15, 4.0e6);
        let outcome = evolve(
            &disk,
            &GrowthParams::default(),
            &seeds(&[(15.0, 1.0e5)]),
            &[],
        );
        let planet = &outcome.planets[0];
        assert!(planet.runaway, "{planet:?}");
        assert!(
            earths(planet.mass_msun()) > 50.0,
            "{}",
            earths(planet.mass_msun())
        );
        assert!(planet.envelope_msun > planet.core_msun);
        assert!(planet.a_au < 15.0);
        let kinds: Vec<_> = outcome
            .events
            .iter()
            .map(|(_, e)| std::mem::discriminant(e))
            .collect();
        assert!(kinds.contains(&std::mem::discriminant(&GrowthEvent::RunawayGas { id: 0 })));
        assert!(kinds.contains(&std::mem::discriminant(&GrowthEvent::GapOpened { id: 0 })));
    }

    /// A body at isolation starves the embryos inside its orbit: the same
    /// inner seed ends up smaller when an outer one grows first.
    #[test]
    fn an_isolated_outer_body_starves_the_inner_disk() {
        let disk = disk_with(0.08, 3.0e6);
        let params = GrowthParams {
            migration: false,
            ..GrowthParams::default()
        };
        let alone = evolve(&disk, &params, &seeds(&[(1.0, 3.0e5)]), &[]);
        let shadowed = evolve(&disk, &params, &seeds(&[(1.0, 3.0e5), (6.0, 5.0e4)]), &[]);
        let inner_alone = alone.planets[0].mass_msun();
        let inner_shadowed = shadowed
            .planets
            .iter()
            .find(|p| p.id == 0)
            .unwrap()
            .mass_msun();
        assert!(
            inner_shadowed < 0.7 * inner_alone,
            "{} vs {}",
            earths(inner_shadowed),
            earths(inner_alone)
        );
    }

    /// Converging bodies end in resonance or merged, never closer than the
    /// Hill-stability limit, and nothing passes the inner edge.
    #[test]
    fn neighbours_end_in_resonance_or_merge() {
        let disk = disk_with(0.1, 3.0e6);
        let s = seeds(&[
            (0.6, 1.0e5),
            (1.2, 1.0e5),
            (2.5, 1.0e5),
            (5.0, 1.0e5),
            (9.0, 1.0e5),
            (16.0, 1.0e5),
        ]);
        let outcome = evolve(&disk, &GrowthParams::default(), &s, &[]);
        assert!(outcome.planets.len() <= 6 && !outcome.planets.is_empty());
        for pair in outcome.planets.windows(2) {
            let spacing = (pair[1].a_au - pair[0].a_au)
                / mutual_hill(
                    pair[0].a_au,
                    pair[1].a_au,
                    pair[0].mass_msun(),
                    pair[1].mass_msun(),
                    1.0,
                );
            assert!(spacing >= HILL_STABILITY * 0.999, "{spacing}");
        }
        let captures = outcome
            .events
            .iter()
            .filter(|(_, e)| matches!(e, GrowthEvent::ResonanceCaptured { .. }))
            .count();
        let merges = outcome
            .events
            .iter()
            .filter(|(_, e)| matches!(e, GrowthEvent::Merged { .. }))
            .count();
        assert!(captures + merges > 0);
        assert_eq!(outcome.planets.len() + merges, 6);
        assert!(outcome.planets.iter().all(|p| p.a_au >= 0.05));
        // Mass is conserved through mergers: every survivor's mergers are counted.
        assert_eq!(
            outcome
                .planets
                .iter()
                .map(|p| p.mergers as usize)
                .sum::<usize>(),
            merges
        );
    }

    #[test]
    fn resonances_are_chosen_by_mass() {
        // Two Earths are caught by the 3:2 first; two Jupiters by the 2:1
        // only; two bodies of 10 Jupiter masses by nothing.
        let earth = EARTH_MASS_MSUN;
        let first = |m: f64| capture_candidates(1.0, m, m, 1.0).first().map(|r| r.1);
        assert_eq!(first(earth), Some((3, 2)));
        assert_eq!(capture_candidates(1.0, earth, earth, 1.0).len(), 3);
        assert_eq!(
            capture_candidates(1.0, 318.0 * earth, 318.0 * earth, 1.0),
            vec![resonances()[0]]
        );
        assert_eq!(first(3180.0 * earth), Some((2, 1)));
        assert!(
            spacing_at(resonances()[0].0, 1.0, 3180.0 * earth, 3180.0 * earth, 1.0)
                < HILL_STABILITY
        );
        // Kepler's third law: period ratios 2:1 and 3:2 are ratios of 1.5874
        // and 1.3104 in distance.
        assert!((resonances()[0].0 - 1.5874).abs() < 1e-4);
        assert!((resonances()[1].0 - 1.3104).abs() < 1e-4);
    }

    /// Ginzburg et al. (2016): a 5 M⊕ core at 1000 K embedded for 1 Myr
    /// holds 0.02·5^0.8 = 7.2% of its mass in gas.
    #[test]
    fn embedded_envelopes_follow_ginzburg() {
        let f = embedded_envelope_fraction(5.0 * EARTH_MASS_MSUN, 1000.0, 1.0e6);
        assert!((f - 0.02 * 5f64.powf(0.8)).abs() < 1e-12);
        // Mars-sized bodies keep none; the taper ends at 1 M⊕.
        assert_eq!(
            embedded_envelope_fraction(0.1 * EARTH_MASS_MSUN, 1000.0, 1.0e6),
            0.0
        );
        let at_one = embedded_envelope_fraction(EARTH_MASS_MSUN, 1000.0, 1.0e6);
        assert!((at_one - 0.02).abs() < 1e-12);
        let below = embedded_envelope_fraction(0.75 * EARTH_MASS_MSUN, 1000.0, 1.0e6);
        assert!(below > 0.0 && below < 0.5 * at_one + 1e-3);
        assert_eq!(
            embedded_envelope_fraction(50.0 * EARTH_MASS_MSUN, 300.0, 3.0e6),
            0.5
        );
    }
}
