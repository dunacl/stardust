//! Integrates a set of embryos through the life of the gas disk.
//!
//! Each step, working inward from the outer disk:
//!
//! 1. **Pebbles and planetesimals.** The pebble flux drifts inward from the
//!    front where dust has just grown into pebbles. In every ring it crosses,
//!    a fraction is caught in pressure traps and becomes planetesimals
//!    ([`physics::planetesimal_conversion_fraction`]); every body takes its
//!    share ([`physics::pebble_accretion_rate`]); the flux loses its ice at
//!    the ice line; and a body that has reached the pebble isolation mass
//!    lets nothing past, starving everything inside its orbit (Lambrechts et
//!    al. 2019, A&A 627, A83).
//! 2. **Embryos.** An embryo appears at one of the given sites once the
//!    planetesimals around it amount to its seed mass, and grows by sweeping
//!    them up ([`physics::planetesimal_accretion_time_yr`]). When it
//!    outgrows the pebble transition mass it accretes pebbles as well. So
//!    how soon embryos appear, and how fast they grow, follows from how much
//!    solid material the disk has.
//! 3. **Gas.** A body past isolation contracts its envelope and, once the
//!    envelope outweighs the core, accretes gas in a runaway.
//! 4. **Migration.** Bodies move by the disk's torque. Neighbours that
//!    converge are caught in a mean-motion resonance and from then on move
//!    as a chain, at the rate their summed torques give; a chain stops when
//!    its innermost member reaches the disk's inner edge. Neighbours too
//!    close for anything to keep apart merge.
//!
//! When the gas has gone, bodies that never entered runaway are left with
//! the part of their envelope that survives the disk's dispersal (Ginzburg,
//! Schlichting & Sari 2016). The planetesimals not swept up are returned,
//! ring by ring, for the stage after the gas.
//!
//! Simplifications: orbits are circular and coplanar while gas is present;
//! resonance capture is certain, in first-order resonances only; mergers
//! are perfect; a body accretes
//! planetesimals only from the ring it is in; pebble accretion below the
//! transition mass is neglected; the sites where embryos can appear are
//! given, not predicted. Lenz et al. form planetesimals only from pebbles
//! with Stokes numbers above 0.01, which with the fragmentation limit used
//! here would leave the region inside the ice line without any; that limit
//! is not applied, on the grounds that silicate grains released by
//! sublimating pebbles pile up and form planetesimals there (Ida & Guillot
//! 2016, A&A 596, L3).

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
    /// Fraction of the pebbles entering a pressure trap that become
    /// planetesimals: Lenz et al.'s `ε`.
    pub planetesimal_efficiency: f64,
    /// Kelvin–Helmholtz time of a 1 M⊕ planet's envelope, years: see
    /// [`physics::envelope_contraction_rate`].
    pub cooling_time_1mearth_yr: f64,
    /// Whether bodies migrate.
    pub migration: bool,
    /// Factor applied to the migration rate of a planet that has not opened
    /// a gap. Below 1 it stands in for torques this model leaves out, which
    /// oppose the inward drift of low-mass planets (the corotation torque,
    /// Paardekooper et al. 2011; the heating torque, Benítez-Llambay et al.
    /// 2015). Population models have long needed such a factor of 0.01–0.1
    /// to keep planets from falling into their stars (Ida & Lin 2008,
    /// ApJ 673, 487). Those torques need gas in the planet's co-orbital
    /// region, so the factor rises to 1 as the gap deepens: gap-opening
    /// planets migrate at the full type II rate.
    pub migration_efficiency: f64,
}

impl Default for GrowthParams {
    fn default() -> Self {
        Self {
            alpha_turbulent: 1.0e-4,
            stokes_drift: 0.05,
            alpha_gap: 1.0e-4,
            planetesimal_efficiency: 0.1,
            cooling_time_1mearth_yr: physics::NOMINAL_COOLING_TIME_YR,
            migration: true,
            migration_efficiency: 1.0,
        }
    }
}

/// A site where an embryo can appear.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seed {
    pub a_au: f64,
}

/// The planetesimals in one ring of the disk.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ring {
    pub inner_au: f64,
    pub outer_au: f64,
    /// Planetesimals not swept up by any body, M☉.
    pub mass_msun: f64,
    /// Part of that mass that is water ice, M☉.
    pub water_msun: f64,
}

impl Ring {
    /// Surface density of planetesimals, M☉/AU².
    pub fn surface_density(&self) -> f64 {
        self.mass_msun
            / (core::f64::consts::PI
                * (self.outer_au * self.outer_au - self.inner_au * self.inner_au))
    }
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
    /// Planetesimals left when the gas cleared, in rings ordered outward.
    pub planetesimals: Vec<Ring>,
    /// Integration steps taken.
    pub steps: u64,
}

/// Longest step, years.
const MAX_STEP_YR: f64 = 1.0e4;
/// Shortest step, years. With the fraction below it bounds the work.
const MIN_STEP_YR: f64 = 20.0;
/// A step changes no mass or orbit by more than this fraction.
const STEP_FRACTION: f64 = 0.03;
/// Mass of an embryo when it appears, M☉: 10⁻⁴ M⊕, about the mass of
/// Ceres, the largest bodies streaming instability forms in simulations
/// (Schäfer, Yang & Johansen 2017, A&A 597, A69).
pub const SEED_MASS_MSUN: f64 = 1.0e-4 * EARTH_MASS_MSUN;
/// Rings the disk is divided into for its planetesimals, spaced evenly in
/// log radius.
const RING_COUNT: usize = 64;
/// Planetesimals are not followed beyond this, AU.
const RING_OUTER_LIMIT_AU: f64 = 200.0;
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

/// Closest first-order resonance considered: 10:9.
const MAX_RESONANCE_INDEX: u8 = 9;

/// First-order resonances a converging pair can be caught in, widest
/// first: `(a_outer/a_inner, (j + 1, j))` for the period ratio (j+1):j,
/// from 2:1 to 10:9. By Kepler's third law the distance ratio is the period
/// ratio to the power 2/3.
fn resonances() -> Vec<(f64, (u8, u8))> {
    (1..=MAX_RESONANCE_INDEX)
        .map(|j| {
            let period_ratio = f64::from(j + 1) / f64::from(j);
            (pow(period_ratio, 2.0 / 3.0), (j + 1, j))
        })
        .collect()
}

fn mutual_hill(a_in: f64, a_out: f64, m_in: f64, m_out: f64, star: f64) -> f64 {
    0.5 * (a_in + a_out) * pow((m_in + m_out) / (3.0 * star), 1.0 / 3.0)
}

/// Spacing, in mutual Hill radii, at which a resonance would hold a pair.
fn spacing_at(ratio: f64, a_in: f64, m_in: f64, m_out: f64, star: f64) -> f64 {
    let a_out = a_in * ratio;
    (a_out - a_in) / mutual_hill(a_in, a_out, m_in, m_out, star)
}

/// The resonances that can catch a pair, widest first.
///
/// A resonance can hold a pair only if it keeps them `RESONANCE_MIN_HILL`
/// mutual Hill radii apart. Light pairs slip through the 2:1 and are caught
/// in the 3:2, the commonest outcome for super-Earths migrating at type I
/// rates (Ogihara & Kobayashi 2013, ApJ 775, 34); the 2:1 is used only by
/// pairs too massive for the 3:2. Neighbouring embryos form closer together
/// than the 3:2 and are caught in the tighter resonances, 4:3 to 10:9, as
/// in the compact chains of N-body simulations (Izidoro et al. 2017,
/// MNRAS 470, 1750).
fn capture_candidates(a_in: f64, m_in: f64, m_out: f64, star: f64) -> Vec<(f64, (u8, u8))> {
    let all = resonances();
    let holds =
        |r: &(f64, (u8, u8))| spacing_at(r.0, a_in, m_in, m_out, star) >= RESONANCE_MIN_HILL;
    let three_two_holds = holds(&all[1]);
    all.into_iter()
        .enumerate()
        .filter(|(index, r)| {
            if *index == 0 {
                !three_two_holds
            } else {
                holds(r)
            }
        })
        .map(|(_, r)| r)
        .collect()
}

/// Envelope a core gathers while embedded in the gas disk, as a fraction
/// of its mass, before it loses any.
///
/// Ginzburg, Schlichting & Sari (2016, ApJ 825, 29), eq. 18:
/// `f ≈ 0.02 (M_c/M⊕)^0.8 (T_eq/1000 K)^-0.25 (t_disk/1 Myr)^0.5`, where
/// `T_eq` is the planet's equilibrium temperature and `t_disk` the time it
/// spent embedded.
pub fn accreted_envelope_fraction(
    core_msun: f64,
    equilibrium_temp_k: f64,
    embedded_yr: f64,
) -> f64 {
    0.02 * pow(core_msun / EARTH_MASS_MSUN, 0.8)
        * pow(equilibrium_temp_k / 1000.0, -0.25)
        * sqrt(embedded_yr / 1.0e6)
}

/// Envelope a core that never entered runaway still holds once the gas has
/// cleared, as a fraction of its mass.
///
/// When the disk's pressure is removed, the loosely bound outer layers of
/// the envelope flow away, driven by the heat of the layers beneath; about
/// 70% of it goes. Ginzburg, Schlichting & Sari (2016), eq. 24:
/// `f ≈ 0.01 (M_c/M⊕)^0.44 (T_eq/1000 K)^0.25 (t_disk/1 Myr)^0.5`. Never
/// more than was accreted ([`accreted_envelope_fraction`]).
///
/// The relation was derived for super-Earth cores. Smaller bodies gather
/// only wisps of gas that they do not keep, so the fraction is tapered to
/// zero between 1 and 0.5 M⊕; that taper is this model's, not the paper's.
pub fn retained_envelope_fraction(
    core_msun: f64,
    equilibrium_temp_k: f64,
    embedded_yr: f64,
) -> f64 {
    let core = core_msun / EARTH_MASS_MSUN;
    let retained =
        0.01 * pow(core, 0.44) * pow(equilibrium_temp_k / 1000.0, 0.25) * sqrt(embedded_yr / 1.0e6);
    let accreted = accreted_envelope_fraction(core_msun, equilibrium_temp_k, embedded_yr);
    let taper = ((core - 0.5) / 0.5).clamp(0.0, 1.0);
    taper * retained.min(accreted).min(MAX_EMBEDDED_ENVELOPE)
}

/// Edges of the planetesimal rings of a disk: `RING_COUNT + 1` radii from
/// the inner edge outward, evenly spaced in log radius.
fn ring_edges(disk: &Disk) -> Vec<f64> {
    let inner = disk.spec().inner_edge_au;
    let outer = disk
        .spec()
        .outer_edge_au
        .min(RING_OUTER_LIMIT_AU)
        .max(inner * 1.01);
    let ratio = pow(outer / inner, 1.0 / RING_COUNT as f64);
    let mut edges = Vec::with_capacity(RING_COUNT + 1);
    let mut edge = inner;
    for _ in 0..=RING_COUNT {
        edges.push(edge);
        edge *= ratio;
    }
    edges
}

/// One site per planetesimal ring, at its (geometric) middle: where
/// embryos appear when nothing more is known. A ring is about as wide as
/// the spacing oligarchic growth gives embryos of a few tenths of an Earth
/// mass, some 10 Hill radii (Kokubo & Ida 1998, Icarus 131, 171), so each
/// ring's planetesimals feed one embryo.
pub fn ring_sites(disk: &Disk) -> Vec<Seed> {
    ring_edges(disk)
        .windows(2)
        .map(|w| Seed {
            a_au: sqrt(w[0] * w[1]),
        })
        .collect()
}

/// Runs the growth model from the disk's formation until its gas is gone.
/// `frame_times_yr` must be ascending.
#[allow(clippy::too_many_lines)]
pub fn evolve(
    disk: &Disk,
    params: &GrowthParams,
    seeds: &[Seed],
    frame_times_yr: &[f64],
) -> Outcome {
    let star = disk.spec().star_mass_msun;
    let inner_edge = disk.spec().inner_edge_au;
    let outer_edge = disk.spec().outer_edge_au;
    let end = disk.gas_gone_yr();
    let alpha = params.alpha_turbulent;
    let efficiency = params.planetesimal_efficiency;

    // Rings for the planetesimals, evenly spaced in log radius.
    let edges = ring_edges(disk);
    let ring_outer = edges[RING_COUNT];
    let ring_ratio = edges[1] / edges[0];
    let mut rings: Vec<Ring> = edges
        .windows(2)
        .map(|w| Ring {
            inner_au: w[0],
            outer_au: w[1],
            mass_msun: 0.0,
            water_msun: 0.0,
        })
        .collect();
    let log_ratio = sg_core::math::ln(ring_ratio);
    let ring_of = |a_au: f64| -> Option<usize> {
        if a_au < inner_edge || a_au >= ring_outer {
            return None;
        }
        let index = (sg_core::math::ln(a_au / inner_edge) / log_ratio) as usize;
        Some(index.min(RING_COUNT - 1))
    };

    let mut bodies: Vec<Embryo> = Vec::new();
    // Sites still waiting for enough planetesimals, by seed index.
    let mut pending: Vec<(u32, Seed)> = seeds
        .iter()
        .copied()
        .zip(0u32..)
        .map(|(s, i)| (i, s))
        .filter(|(_, seed)| seed.a_au > inner_edge && seed.a_au < outer_edge)
        .collect();
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
        // Embryos appear where the planetesimals now amount to a seed.
        let mut k = 0;
        while k < pending.len() {
            let (id, seed) = pending[k];
            let ready = ring_of(seed.a_au).filter(|c| rings[*c].mass_msun >= SEED_MASS_MSUN);
            let Some(c) = ready else {
                k += 1;
                continue;
            };
            pending.remove(k);
            let water = SEED_MASS_MSUN * rings[c].water_msun / rings[c].mass_msun;
            rings[c].mass_msun -= SEED_MASS_MSUN;
            rings[c].water_msun -= water;
            bodies.push(Embryo {
                id,
                a_au: seed.a_au,
                core_msun: SEED_MASS_MSUN,
                envelope_msun: 0.0,
                water_msun: water,
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

        // Rates. The pebble flux is followed inward from the front, ring by
        // ring, so each body and each ring sees what is left for it.
        let ice_line = disk.water_ice_line(t);
        let front = physics::pebble_front_radius(star, disk.spec().dust_to_gas, t).min(outer_edge);
        let mut flux = physics::pebble_flux(disk, t);
        let mut past_ice_line = false;
        let n = bodies.len();
        let mut solid_rate = vec![0.0f64; n];
        let mut sweep_rate = vec![0.0f64; n];
        let mut gas_rate = vec![0.0f64; n];
        let mut drift_rate = vec![0.0f64; n];
        // Planetesimal formation per ring, M☉/yr, and whether it is icy.
        let mut ring_rate = [0.0f64; RING_COUNT];
        let mut ring_icy = [false; RING_COUNT];

        let mut next_body = n;
        // One pass per ring from the outside, plus a last pass (`None`) for
        // any body inside the innermost ring's inner edge.
        for ring in (0..RING_COUNT).rev().map(Some).chain(std::iter::once(None)) {
            let lower = ring.map_or(0.0, |c| rings[c].inner_au);
            while next_body > 0 && bodies[next_body - 1].a_au >= lower {
                next_body -= 1;
                let i = next_body;
                let b = &bodies[i];
                if !past_ice_line && b.a_au < ice_line {
                    flux *= 1.0 - ICE_MASS_FRACTION;
                    past_ice_line = true;
                }
                if b.isolated {
                    // Its pressure bump holds back all pebbles from outside.
                    flux = 0.0;
                }
                // A body halted at the disk's inner edge sits where the gas
                // ends, and draws none in.
                if b.isolated && b.a_au > inner_edge {
                    // Contraction first, a runaway once the envelope
                    // outweighs the core; either way no faster than the gap
                    // lets gas in.
                    let supply = physics::runaway_accretion_rate(
                        disk,
                        b.a_au,
                        t,
                        b.mass_msun(),
                        params.alpha_gap,
                    );
                    gas_rate[i] = if b.envelope_msun < b.core_msun {
                        physics::envelope_contraction_rate(
                            b.mass_msun(),
                            params.cooling_time_1mearth_yr,
                        )
                        .min(supply)
                    } else {
                        supply
                    };
                } else if b.a_au <= front
                    && b.mass_msun() >= physics::transition_mass(disk, b.a_au, t)
                {
                    let stokes = physics::stokes_number(
                        disk,
                        b.a_au,
                        t,
                        params.stokes_drift,
                        alpha,
                        ice_line,
                    );
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
                // Planetesimals from the ring it is in.
                if let Some(c) = ring_of(b.a_au) {
                    let time = physics::planetesimal_accretion_time_yr(
                        rings[c].surface_density() * physics::MSUN_PER_AU2_IN_G_PER_CM2,
                        disk.gas_surface_density(b.a_au, t) * physics::MSUN_PER_AU2_IN_G_PER_CM2,
                        b.a_au,
                        b.mass_msun(),
                        star,
                    );
                    sweep_rate[i] = b.mass_msun() / time;
                }
                if params.migration && b.a_au > inner_edge {
                    // The efficiency stands in for the corotation torque,
                    // which fades as a gap opens: it applies in full to a
                    // planet with no gap and not at all to one deep in its
                    // gap.
                    let k =
                        physics::gap_parameter(disk, b.a_au, t, b.mass_msun(), params.alpha_gap);
                    let opened = 0.04 * k / (1.0 + 0.04 * k);
                    let factor =
                        params.migration_efficiency + (1.0 - params.migration_efficiency) * opened;
                    drift_rate[i] = factor
                        * physics::migration_rate(disk, b.a_au, t, b.mass_msun(), params.alpha_gap)
                        / b.a_au;
                }
            }
            // Planetesimals form from the flux crossing this ring.
            let Some(c) = ring else { continue };
            let mid = sqrt(rings[c].inner_au * rings[c].outer_au);
            if mid >= front || flux <= 0.0 {
                continue;
            }
            if !past_ice_line && mid < ice_line {
                flux *= 1.0 - ICE_MASS_FRACTION;
                past_ice_line = true;
            }
            let period = 2.0 * core::f64::consts::PI / disk.omega(mid);
            if flux > physics::critical_pebble_flux(efficiency, period) {
                let fraction = physics::planetesimal_conversion_fraction(
                    efficiency,
                    rings[c].outer_au - rings[c].inner_au,
                    disk.aspect_ratio(mid, t) * mid,
                );
                ring_rate[c] = fraction * flux;
                ring_icy[c] = !past_ice_line;
                flux -= ring_rate[c];
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
            let growth = (solid_rate[i] + sweep_rate[i] + gas_rate[i]) / bodies[i].mass_msun();
            let motion = chain_rate[chain_of[i]].abs();
            let fastest = growth.max(motion);
            if fastest > 0.0 {
                dt = dt.min(STEP_FRACTION / fastest);
            }
        }
        dt = dt.max(MIN_STEP_YR);
        if next_frame < frame_times_yr.len() {
            dt = dt.min((frame_times_yr[next_frame] - t).max(1.0));
        }
        dt = dt.min(end - t).max(1.0e-6);

        // New planetesimals.
        for c in 0..RING_COUNT {
            let formed = ring_rate[c] * dt;
            rings[c].mass_msun += formed;
            if ring_icy[c] {
                rings[c].water_msun += ICE_MASS_FRACTION * formed;
            }
        }

        // Apply growth and motion.
        for i in 0..n {
            let b = &mut bodies[i];
            let solids = solid_rate[i] * dt;
            b.core_msun += solids;
            if b.a_au >= ice_line {
                b.water_msun += ICE_MASS_FRACTION * solids;
            }
            // Planetesimals swept up: no more than its ring holds.
            if let Some(c) = ring_of(b.a_au) {
                let swept = (sweep_rate[i] * dt).min(rings[c].mass_msun);
                if swept > 0.0 {
                    let water = swept * rings[c].water_msun / rings[c].mass_msun;
                    rings[c].mass_msun -= swept;
                    rings[c].water_msun -= water;
                    b.core_msun += swept;
                    b.water_msun += water;
                }
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

    // A core that never entered runaway keeps only what survives the
    // disk's dispersal. (Until runaway the envelope tracked above is a
    // clock for its onset, not a prediction of its mass: the contraction
    // fit is calibrated for cores near 5 AU and overestimates what small,
    // hot, close-in cores hold.)
    let luminosity = disk.spec().star_luminosity_lsun;
    for b in &mut bodies {
        if !b.runaway {
            let embedded_yr = (disk.spec().dispersal_time_yr - b.start_yr).max(0.0);
            // Black-body equilibrium temperature: 278.3 K at 1 AU from the Sun.
            let equilibrium = 278.3 * sqrt(sqrt(luminosity)) / sqrt(b.a_au);
            b.envelope_msun =
                b.core_msun * retained_envelope_fraction(b.core_msun, equilibrium, embedded_yr);
        }
    }
    bodies.sort_by(|x, y| x.a_au.total_cmp(&y.a_au).then(x.id.cmp(&y.id)));

    Outcome {
        planets: bodies,
        events,
        frames,
        planetesimals: rings,
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

    /// Sites at the given distances. (The second number of each pair is
    /// ignored: embryos appear when their site has enough planetesimals.)
    fn seeds(list: &[(f64, f64)]) -> Vec<Seed> {
        list.iter().map(|(a_au, _)| Seed { a_au: *a_au }).collect()
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

    /// Without migration a seed stays where it was put. Near the ice line,
    /// where planetesimals are plentiful, it grows to the pebble isolation
    /// mass if the disk lives long enough.
    #[test]
    fn a_lone_seed_grows_to_isolation_in_place() {
        let disk = disk_with(0.08, 3.0e6);
        let params = GrowthParams {
            migration: false,
            ..GrowthParams::default()
        };
        let outcome = evolve(&disk, &params, &seeds(&[(2.0, 0.0)]), &[]);
        let planet = &outcome.planets[0];
        assert_eq!(planet.a_au, 2.0);
        assert!(planet.isolated, "{planet:?}");
        assert!(
            earths(planet.core_msun) > 2.0,
            "{}",
            earths(planet.core_msun)
        );
        assert!(planet.water_msun > 0.2 * planet.core_msun);
        assert!(outcome
            .events
            .iter()
            .any(|(_, e)| matches!(e, GrowthEvent::IsolationReached { id: 0 })));
    }

    /// Embryos appear wherever planetesimals have formed, in more places
    /// in a disk with more solids, and grow faster there. This is where the
    /// dependence of planets on metallicity comes from.
    #[test]
    fn embryos_grow_faster_where_there_are_more_solids() {
        let with_dust = |dust_to_gas: f64| {
            let disk = Disk::new(DiskSpec {
                dust_to_gas,
                ..disk_with(0.03, 3.0e6).spec().clone()
            })
            .unwrap();
            let sites = ring_sites(&disk);
            assert_eq!(sites.len(), RING_COUNT);
            evolve(&disk, &GrowthParams::default(), &sites, &[])
        };
        let seeded = |outcome: &Outcome| -> Vec<(f64, f64)> {
            outcome
                .events
                .iter()
                .filter_map(|(t, e)| match e {
                    GrowthEvent::Seeded { a_au, .. } => Some((*t, *a_au)),
                    _ => None,
                })
                .collect()
        };
        let (rich, poor) = (with_dust(0.03), with_dust(0.005));
        let (rich_seeds, poor_seeds) = (seeded(&rich), seeded(&poor));
        assert!(rich_seeds.len() >= poor_seeds.len());
        assert!(poor_seeds.len() > 5);
        // The largest body near 1 AU is heavier in the metal-rich disk.
        let near_one_au = |o: &Outcome| {
            o.planets
                .iter()
                .filter(|p| (0.5..2.0).contains(&p.origin_a_au))
                .map(Embryo::mass_msun)
                .fold(0.0f64, f64::max)
        };
        assert!(near_one_au(&rich) > 2.0 * near_one_au(&poor));
        // And the metal-rich disk builds far more in total.
        let total = |o: &Outcome| o.planets.iter().map(Embryo::mass_msun).sum::<f64>();
        assert!(
            total(&rich) > 5.0 * total(&poor),
            "{} {}",
            earths(total(&rich)),
            earths(total(&poor))
        );
    }

    /// Solids are conserved: what ends in planets and in leftover
    /// planetesimals never exceeds the disk's dust.
    #[test]
    fn solids_never_exceed_the_dust_supply() {
        for (mass, dust_to_gas) in [(0.02, 0.0149), (0.08, 0.03), (0.005, 0.004)] {
            let disk = Disk::new(DiskSpec {
                dust_to_gas,
                ..disk_with(mass, 3.0e6).spec().clone()
            })
            .unwrap();
            let outcome = evolve(&disk, &GrowthParams::default(), &ring_sites(&disk), &[]);
            let in_planets: f64 = outcome.planets.iter().map(|p| p.core_msun).sum();
            let left: f64 = outcome.planetesimals.iter().map(|r| r.mass_msun).sum();
            let dust = mass * dust_to_gas;
            assert!(
                in_planets + left <= dust,
                "{} + {} vs {}",
                earths(in_planets),
                earths(left),
                earths(dust)
            );
            assert!(left > 0.0);
            for ring in &outcome.planetesimals {
                assert!(ring.mass_msun >= 0.0 && ring.water_msun >= -1e-18);
                assert!(ring.water_msun <= 0.5 * ring.mass_msun + 1e-18);
                assert!(ring.inner_au < ring.outer_au);
            }
            // Planetesimals inside the ice line are dry.
            assert!(outcome.planetesimals[0].water_msun.abs() < 1e-18);
        }
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

    /// A massive, metal-rich disk makes a gas giant; a light one does not.
    #[test]
    fn a_massive_disk_makes_a_gas_giant() {
        let run = |mass: f64, dust_to_gas: f64| {
            let disk = Disk::new(DiskSpec {
                dust_to_gas,
                ..disk_with(mass, 4.0e6).spec().clone()
            })
            .unwrap();
            evolve(&disk, &GrowthParams::default(), &ring_sites(&disk), &[])
        };
        let heavy = run(0.1, 0.03);
        let giant = heavy
            .planets
            .iter()
            .max_by(|x, y| x.mass_msun().total_cmp(&y.mass_msun()))
            .unwrap();
        assert!(giant.runaway, "{giant:?}");
        assert!(
            earths(giant.mass_msun()) > 50.0,
            "{}",
            earths(giant.mass_msun())
        );
        assert!(giant.envelope_msun > giant.core_msun);
        let kinds: Vec<_> = heavy
            .events
            .iter()
            .map(|(_, e)| std::mem::discriminant(e))
            .collect();
        assert!(kinds.contains(&std::mem::discriminant(&GrowthEvent::RunawayGas { id: 0 })));
        assert!(kinds.contains(&std::mem::discriminant(&GrowthEvent::GapOpened { id: 0 })));

        let light = run(0.005, 0.01);
        assert!(light.planets.iter().all(|p| !p.runaway));
    }

    /// A planet halted at the disk's inner edge sits where the gas ends and
    /// draws none in, however heavy its core.
    #[test]
    fn nothing_accretes_gas_at_the_inner_edge() {
        for seed in 0..3 {
            let disk = disk_with(0.05 + 0.02 * f64::from(seed), 3.0e6);
            let outcome = evolve(&disk, &GrowthParams::default(), &ring_sites(&disk), &[]);
            for planet in outcome.planets.iter().filter(|p| p.a_au <= 0.05) {
                // Any envelope it has was gained before it arrived.
                assert!(!planet.runaway || planet.envelope_msun < 50.0 * planet.core_msun);
            }
        }
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
        // Two Earths are caught by the 3:2 first and can be held as close as
        // the 10:9; two Jupiters by the 2:1 only; two bodies of 10 Jupiter
        // masses by nothing that keeps them apart.
        let earth = EARTH_MASS_MSUN;
        let first = |m: f64| capture_candidates(1.0, m, m, 1.0).first().map(|r| r.1);
        assert_eq!(first(earth), Some((3, 2)));
        let for_earths = capture_candidates(1.0, earth, earth, 1.0);
        assert_eq!(for_earths.last().map(|r| r.1), Some((10, 9)));
        assert_eq!(for_earths.len(), 8);
        assert_eq!(
            capture_candidates(1.0, 318.0 * earth, 318.0 * earth, 1.0),
            vec![resonances()[0]]
        );
        assert_eq!(first(3180.0 * earth), Some((2, 1)));
        assert!(
            spacing_at(resonances()[0].0, 1.0, 3180.0 * earth, 3180.0 * earth, 1.0)
                < HILL_STABILITY
        );
        // Heavier pairs cannot be held as tightly: two 10 M⊕ planets stop at 5:4.
        let heavy = capture_candidates(1.0, 10.0 * earth, 10.0 * earth, 1.0);
        assert_eq!(heavy.last().map(|r| r.1), Some((5, 4)));
        // Kepler's third law: period ratios 2:1 and 3:2 are ratios of 1.5874
        // and 1.3104 in distance.
        assert!((resonances()[0].0 - 1.5874).abs() < 1e-4);
        assert!((resonances()[1].0 - 1.3104).abs() < 1e-4);
        assert_eq!(resonances().len(), 9);
    }

    /// Ginzburg et al. (2016): a 5 M⊕ core at 1000 K embedded for 1 Myr
    /// gathers 0.02·5^0.8 = 7.2% of its mass in gas and keeps
    /// 0.01·5^0.44 = 2.0% once the disk has gone, about 30% of it.
    #[test]
    fn envelopes_follow_ginzburg() {
        let core = 5.0 * EARTH_MASS_MSUN;
        let accreted = accreted_envelope_fraction(core, 1000.0, 1.0e6);
        assert!((accreted - 0.02 * 5f64.powf(0.8)).abs() < 1e-12);
        let retained = retained_envelope_fraction(core, 1000.0, 1.0e6);
        assert!((retained - 0.01 * 5f64.powf(0.44)).abs() < 1e-12);
        assert!(
            (retained / accreted - 0.28).abs() < 0.02,
            "{}",
            retained / accreted
        );
        // Hotter planets keep a larger share of less; longer-lived disks give more.
        assert!(retained_envelope_fraction(core, 2000.0, 1.0e6) > retained);
        assert!(retained_envelope_fraction(core, 1000.0, 4.0e6) > 1.9 * retained);
        // Mars-sized bodies keep none; the taper ends at 1 M⊕.
        assert_eq!(
            retained_envelope_fraction(0.1 * EARTH_MASS_MSUN, 1000.0, 1.0e6),
            0.0
        );
        let at_one = retained_envelope_fraction(EARTH_MASS_MSUN, 1000.0, 1.0e6);
        assert!((at_one - 0.01).abs() < 1e-12);
        let below = retained_envelope_fraction(0.75 * EARTH_MASS_MSUN, 1000.0, 1.0e6);
        assert!(below > 0.0 && below < 0.5 * at_one + 1e-3);
        // Never more than was accreted, however cold the planet.
        let cold = retained_envelope_fraction(core, 30.0, 1.0e6);
        assert!(cold <= accreted_envelope_fraction(core, 30.0, 1.0e6));
    }
}
