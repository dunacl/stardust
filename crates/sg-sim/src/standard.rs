//! Standard tier: the modern formation pipeline (PLAN.md §5).
//!
//! For every place a system has room for planets ([`crate::stars`]):
//!
//! 1. a gas disk is set up (`sg-disk`), with its mass and lifetime drawn
//!    from observed distributions unless given;
//! 2. embryos are seeded and grown through the disk's life by pebble
//!    accretion, gas accretion and migration (`sg-formation`);
//! 3. once the gas is gone, neighbours too close to last are merged
//!    (`sg-dynamics::late`);
//! 4. each survivor is aged to the system's age: atmospheric escape, size,
//!    temperature, tidal locking and class (`sg-planet`).
//!
//! What is drawn at random here, and from what:
//!
//! - **Metallicity**, if not given: normal, mean −0.05 dex, σ 0.20 dex, the
//!   solar neighbourhood's distribution (Casagrande et al. 2011, A&A 530,
//!   A138), clipped to ±0.6.
//! - **Disk mass**: log-normal about `disk.mass_fraction_median` of the
//!   stellar mass, σ = 0.4 dex, at most 20% (heavier disks fragment). Disk
//!   masses are observed to span roughly 0.1–10% of the star's mass with a
//!   scatter of this size (Andrews et al. 2013, ApJ 771, 129); early disks
//!   are heavier than the evolved ones surveyed, which is why the median
//!   sits at the top of that range. A model parameter, tuned in validation.
//! - **Disk lifetime**: 0.5 Myr plus an exponential with a 2.5 Myr mean,
//!   following the observed decline of the fraction of young stars with
//!   disks (Mamajek 2009, AIP Conf. Proc. 1158, 3).
//! - **Disk size**: `R₁ = 10 AU (M_disk / 0.002 M☉)^(1/1.6)`, the observed
//!   size–mass relation (Andrews et al. 2010, ApJ 723, 1241).
//!
//! Nothing is drawn for the embryos: one can appear in every ring of the
//! disk, when the planetesimals formed there amount to a seed.
//! - **Final eccentricities**: Rayleigh-distributed, small; larger for
//!   systems that went through mergers after the gas cleared.
//!
//! Embryos that end lighter than the Moon are dropped as debris.
//! Planetesimals that no planet accretes or ejects are reported as belts.
//!
//! Not yet in this tier: moons, rings, climate and atmospheric
//! composition.

use crate::high;
use crate::{
    names, params_hash, stars, Control, Diagnostics, SimError, SimObserver, SimulationResult,
    Stage, StellarModel, MODEL_VERSION,
};
use sg_core::constants::{EARTH_RADIUS_KM, SOLAR_MASS_IN_EARTH_MASSES};
use sg_core::hash::fnv1a64;
use sg_core::math::{exp, ln, pow, sqrt};
use sg_core::rng::{ModuleId, RngStream};
use sg_disk::{Disk, DiskSpec};
use sg_dynamics::kepler::Elements;
use sg_dynamics::late::{self, Orbiter};
use sg_formation::evolve::{self, Embryo, GrowthEvent, GrowthParams, Seed};
use sg_planet::{Makeup, PlanetKind};
use sg_schema::*;
use std::collections::BTreeMap;

const DISK_MASS_SIGMA_DEX: f64 = 0.4;
const DISK_MASS_FRACTION_MAX: f64 = 0.2;
const DISK_LIFE_MIN_MYR: f64 = 0.5;
const DISK_LIFE_MEAN_EXTRA_MYR: f64 = 2.5;
/// Dust-to-gas ratio at solar metallicity (Lodders 2003, ApJ 591, 1220).
const SOLAR_DUST_TO_GAS: f64 = 0.0149;
/// Spin periods of young stars still coupled to their disks, days: the
/// range found in star-forming regions (Herbst et al. 2007, Protostars and
/// Planets V, 297). The star's magnetic field truncates the gas near the
/// radius that orbits at the star's spin rate, so this sets the disk's
/// inner edge; periods are drawn log-uniformly.
const STELLAR_SPIN_RANGE_DAYS: (f64, f64) = (2.0, 10.0);
/// Embryos are not seeded beyond this, AU.
const SEED_OUTER_LIMIT_AU: f64 = 60.0;
/// Bodies lighter than this (about the Moon) are leftovers that never
/// grew, and are not reported as planets.
const MIN_PLANET_MASS_MEARTH: f64 = 0.01;
/// Bond albedo assumed for equilibrium temperatures (Earth's is 0.3).
const ASSUMED_ALBEDO: f64 = 0.3;
/// Age at which planets' sizes are taken for the N-body stage, Gyr: the
/// gas has just cleared and envelopes are still warm and large.
const YOUNG_AGE_GYR: f64 = 0.01;
const KM_PER_AU: f64 = 1.495_978_707e8;
const SOLAR_RADIUS_AU: f64 = 0.004_650_47;
/// States of the N-body stage recorded for playback.
const NBODY_FRAMES: u32 = 12;
/// An integration whose energy error exceeds this is reported. Violent
/// scattering of giants over a million years measures 1.4–3.5 thousandths
/// here (`high.rs` tests); beyond five the result should not be trusted.
const NBODY_ENERGY_TOLERANCE: f64 = 5.0e-3;

/// Distance within which a planet is lost to what it orbits: the star's
/// radius, or around a pair of stars the inner limit of stable orbits.
fn central_radius_au(plan: &stars::DiskPlan, system: &stars::StellarSystem) -> f64 {
    match plan.host {
        HostRef::Star(id) => system
            .stars
            .iter()
            .find(|star| star.id == id)
            .map_or(SOLAR_RADIUS_AU, |star| star.radius_rsun * SOLAR_RADIUS_AU),
        _ => plan.inner_limit_au.unwrap_or(SOLAR_RADIUS_AU),
    }
}

/// Rayleigh scale of final eccentricities: undisturbed, and after late
/// mergers.
const ECCENTRICITY_SCALE: (f64, f64) = (0.02, 0.06);

fn check(observer: &mut dyn SimObserver, stage: Stage, fraction: f64) -> Result<(), SimError> {
    match observer.progress(stage, fraction) {
        Control::Continue => Ok(()),
        Control::Cancel => Err(SimError::Cancelled),
    }
}

fn invalid<T>(message: String) -> Result<T, SimError> {
    Err(SimError::InvalidParams(message))
}

fn validate(params: &SimulationParams) -> Result<(), SimError> {
    let s = &params.formation.standard;
    let positive = [
        ("disk.alpha_viscous", params.disk.alpha_viscous),
        ("formation.standard.alpha_turbulent", s.alpha_turbulent),
        ("formation.standard.stokes_drift", s.stokes_drift),
        (
            "disk.mass_fraction_median",
            params.disk.mass_fraction_median,
        ),
        (
            "formation.standard.migration_efficiency",
            s.migration_efficiency,
        ),
        (
            "formation.standard.envelope_cooling_time_yr",
            s.envelope_cooling_time_yr,
        ),
        (
            "formation.standard.planetesimal_efficiency",
            s.planetesimal_efficiency,
        ),
        ("formation.standard.alpha_gap", s.alpha_gap),
    ];
    for (name, value) in positive {
        if !(value.is_finite() && value > 0.0) {
            return invalid(format!("{name} must be positive and finite, got {value}"));
        }
    }
    if let Some(fraction) = params.disk.mass_fraction {
        if !(fraction > 0.0 && fraction <= DISK_MASS_FRACTION_MAX) {
            return invalid(format!(
                "disk.mass_fraction must be in (0, {DISK_MASS_FRACTION_MAX}], got {fraction}"
            ));
        }
    }
    if let Some(life) = params.disk.lifetime_myr {
        if !(life.is_finite() && life > 0.0) {
            return invalid(format!("disk.lifetime_myr must be positive, got {life}"));
        }
    }
    Ok(())
}

/// One planet at the end of formation, before it is aged.
struct Formed {
    embryo: Embryo,
    late_mergers: u32,
    /// The orbit the N-body stage left it on (High tier); otherwise the
    /// eccentricity and angles are drawn when the snapshot is written.
    orbit: Option<Elements>,
}

/// Everything formed in one disk.
struct DiskResult {
    plan: stars::DiskPlan,
    disk: Disk,
    id_offset: u32,
    outcome: evolve::Outcome,
    planets: Vec<Formed>,
    late: Vec<late::Merger>,
    belts: Vec<Belt>,
    /// The N-body late stage, in the High tier.
    nbody: Option<high::LateStage>,
}

/// A belt is reported only if it holds at least this much, M⊕: a quarter
/// of the Solar System's asteroid belt.
const MIN_BELT_MASS_MEARTH: f64 = 1.0e-4;
/// Rings holding less than this, M⊕, do not count as part of a belt.
const MIN_BELT_RING_MASS_MEARTH: f64 = 1.0e-6;

/// The stage after the gas for the planetesimals no body swept up while it
/// lasted: each ring is accreted by a planet, ejected by one, or left as
/// part of a belt (`sg_dynamics::late::ring_fate`). Planets grow by what
/// they accrete; the belts are returned.
fn sweep_leftovers(
    planets: &mut [Formed],
    rings: &[evolve::Ring],
    plan: &stars::DiskPlan,
    age_yr: f64,
) -> Vec<Belt> {
    const EARTH_RADIUS_AU: f64 = EARTH_RADIUS_KM / 1.495_978_707e8;
    let sweepers: Vec<late::Sweeper> = planets
        .iter()
        .map(|p| {
            let e = &p.embryo;
            let radius_rearth = if e.runaway {
                sg_planet::giant_radius(earths(e.mass_msun()))
            } else {
                sg_planet::solid_radius(earths(e.core_msun), e.water_msun / e.core_msun)
            };
            late::Sweeper {
                a_au: e.a_au,
                mass_msun: e.mass_msun(),
                radius_au: radius_rearth * EARTH_RADIUS_AU,
            }
        })
        .collect();

    let mut belts: Vec<Belt> = Vec::new();
    // The belt being extended, if the previous ring remained.
    let mut open: Option<Belt> = None;
    let close = |open: &mut Option<Belt>, belts: &mut Vec<Belt>| {
        if let Some(belt) = open.take() {
            if belt.mass_mearth >= MIN_BELT_MASS_MEARTH {
                belts.push(belt);
            }
        }
    };
    for ring in rings {
        let mid = sqrt(ring.inner_au * ring.outer_au);
        let fate = if earths(ring.mass_msun) < MIN_BELT_RING_MASS_MEARTH {
            None
        } else {
            Some(late::ring_fate(
                mid,
                ring.mass_msun,
                ring.surface_density(),
                &sweepers,
                plan.central_mass_msun,
                age_yr,
            ))
        };
        match fate {
            Some(late::RingFate::Remains) => match open.as_mut() {
                Some(belt) => {
                    belt.outer_au = ring.outer_au;
                    belt.mass_mearth += earths(ring.mass_msun);
                }
                None => {
                    open = Some(Belt {
                        host: plan.host,
                        inner_au: ring.inner_au,
                        outer_au: ring.outer_au,
                        mass_mearth: earths(ring.mass_msun),
                    });
                }
            },
            Some(late::RingFate::Accreted(index)) => {
                let embryo = &mut planets[index].embryo;
                embryo.core_msun += ring.mass_msun;
                embryo.water_msun += ring.water_msun;
                close(&mut open, &mut belts);
            }
            Some(late::RingFate::Ejected) | None => close(&mut open, &mut belts),
        }
    }
    close(&mut open, &mut belts);
    belts
}

fn earths(msun: f64) -> f64 {
    msun * SOLAR_MASS_IN_EARTH_MASSES
}

/// Rayleigh variate with scale `sigma`.
fn rayleigh(rng: &mut RngStream, sigma: f64) -> f64 {
    sigma * sqrt(-2.0 * ln(1.0 - rng.uniform()))
}

fn class_of(kind: PlanetKind) -> PlanetClass {
    match kind {
        PlanetKind::Rocky => PlanetClass::Rocky,
        PlanetKind::SuperEarth => PlanetClass::SuperEarth,
        PlanetKind::LavaWorld => PlanetClass::LavaWorld,
        PlanetKind::WaterWorld => PlanetClass::WaterWorld,
        PlanetKind::IceWorld => PlanetClass::IceWorld,
        PlanetKind::SubNeptune => PlanetClass::SubNeptune,
        PlanetKind::IceGiant => PlanetClass::IceGiant,
        PlanetKind::GasGiant => PlanetClass::GasGiant,
        PlanetKind::HotJupiter => PlanetClass::HotJupiter,
    }
}

/// Sets up the disk of one zone and grows its planets.
fn form_in_disk(
    params: &SimulationParams,
    plan: &stars::DiskPlan,
    dust_to_gas: f64,
    rng: &mut RngStream,
) -> Result<Option<(Disk, Vec<Seed>, f64)>, SimError> {
    let star = plan.central_mass_msun;

    // Draws are taken in a fixed order whether or not they are used, so a
    // given parameter changes only what it controls.
    let mass_draw = pow(10.0, DISK_MASS_SIGMA_DEX * rng.standard_normal());
    let life_draw = -ln(1.0 - rng.uniform());
    let spin_days = STELLAR_SPIN_RANGE_DAYS.0
        * pow(
            STELLAR_SPIN_RANGE_DAYS.1 / STELLAR_SPIN_RANGE_DAYS.0,
            rng.uniform(),
        );
    let fraction = params
        .disk
        .mass_fraction
        .unwrap_or((params.disk.mass_fraction_median * mass_draw).min(DISK_MASS_FRACTION_MAX));
    let lifetime_myr = params
        .disk
        .lifetime_myr
        .unwrap_or(DISK_LIFE_MIN_MYR + DISK_LIFE_MEAN_EXTRA_MYR * life_draw);

    let mut mass = fraction * star;
    let mut r1 = 10.0 * pow(mass / 2.0e-3, 1.0 / 1.6);
    // Corotation radius, from Kepler's third law: a³ = M P² (AU, M☉, yr).
    let spin_yr = spin_days / 365.25;
    let corotation = sg_core::math::cbrt(star * spin_yr * spin_yr);
    let inner_edge = corotation.max(plan.inner_limit_au.unwrap_or(0.0));
    let outer_edge = match plan.outer_limit_au {
        // A companion truncates the disk: only the gas inside the limit is
        // kept, in a correspondingly smaller disk.
        Some(limit) => {
            mass *= 1.0 - exp(-limit / r1);
            r1 = r1.min(0.5 * limit);
            limit
        }
        None => 20.0 * r1,
    };
    // Embryos are seeded between twice the inner edge and the smaller of
    // the outer edge, three disk radii and a fixed limit. A zone without
    // room for that range to span a factor of two (a sliver beside a close
    // companion, or a circumbinary zone that starts beyond the limit) is
    // left without a disk.
    let seed_inner = 2.0 * inner_edge;
    let seed_outer = outer_edge.min(SEED_OUTER_LIMIT_AU).min(3.0 * r1);
    if seed_inner * 2.0 >= seed_outer || mass <= 0.0 {
        return Ok(None);
    }
    let disk = Disk::new(DiskSpec {
        star_mass_msun: star,
        star_luminosity_lsun: plan.luminosity_lsun,
        initial_mass_msun: mass,
        characteristic_radius_au: r1,
        alpha: params.disk.alpha_viscous,
        dispersal_time_yr: lifetime_myr * 1.0e6,
        dust_to_gas,
        inner_edge_au: inner_edge,
        outer_edge_au: outer_edge,
    })
    .map_err(|err| SimError::InvalidParams(err.to_string()))?;

    // Embryos can appear in every ring of the disk's planetesimals within
    // the seeded range. When each does is decided by the growth model.
    let seeds: Vec<Seed> = evolve::ring_sites(&disk)
        .into_iter()
        .filter(|site| site.a_au >= seed_inner && site.a_au <= seed_outer)
        .collect();
    Ok(Some((disk, seeds, lifetime_myr)))
}

#[allow(clippy::too_many_lines)]
pub(crate) fn simulate(
    params: &SimulationParams,
    observer: &mut dyn SimObserver,
) -> Result<SimulationResult, SimError> {
    validate(params)?;
    check(observer, Stage::Stellar, 0.0)?;
    let system_name = names::star_name(params.seed);
    let high_tier = params.fidelity == Fidelity::High;
    let mut dynamics_rng = RngStream::new(params.seed, ModuleId::DYNAMICS);
    let mut stellar_rng = RngStream::new(params.seed, ModuleId::STELLAR);
    let mut disk_rng = RngStream::new(params.seed, ModuleId::DISK);

    // Metallicity first: the stars depend on it.
    let feh_draw = (-0.05 + 0.20 * disk_rng.standard_normal()).clamp(-0.6, 0.6);
    let feh = params.metallicity_feh.unwrap_or(feh_draw);
    if !feh.is_finite() {
        return invalid(format!("metallicity must be finite, got {feh}"));
    }
    let with_feh = SimulationParams {
        metallicity_feh: Some(feh),
        ..params.clone()
    };
    let system = stars::build(&with_feh, &mut stellar_rng, &system_name)?;
    let mut warnings = system.warnings.clone();
    let dust_to_gas = SOLAR_DUST_TO_GAS * pow(10.0, feh);
    let age_gyr = system.age_gyr;

    check(observer, Stage::Formation, 0.1)?;
    let growth = GrowthParams {
        alpha_turbulent: params.formation.standard.alpha_turbulent,
        stokes_drift: params.formation.standard.stokes_drift,
        alpha_gap: params.formation.standard.alpha_gap,
        planetesimal_efficiency: params.formation.standard.planetesimal_efficiency,
        cooling_time_1mearth_yr: params.formation.standard.envelope_cooling_time_yr,
        migration: params.toggles.migration,
        migration_efficiency: params.formation.standard.migration_efficiency,
    };
    let frame_count = params.timeline.keyframe_count.max(1);

    // Set up every disk first, so the timeline can span the longest-lived.
    let mut setups = Vec::new();
    for plan in &system.disks {
        if let Some((disk, seeds, lifetime_myr)) =
            form_in_disk(params, plan, dust_to_gas, &mut disk_rng)?
        {
            setups.push((plan.clone(), disk, seeds, lifetime_myr));
        }
    }
    let end_yr = setups
        .iter()
        .map(|(_, disk, _, _)| disk.gas_gone_yr())
        .fold(0.0f64, f64::max)
        * 1.05;
    // A system with no room for any disk still gets a (featureless) timeline.
    let end_yr = if end_yr > 0.0 { end_yr } else { 1.0e6 };
    // The last frame is replaced by the settled system, so the growth model
    // is asked only for the ones before it.
    let frame_times: Vec<f64> = (1..frame_count)
        .map(|k| end_yr * f64::from(k) / f64::from(frame_count))
        .collect();

    let mut results: Vec<DiskResult> = Vec::new();
    let mut id_offset = 0u32;
    let mut steps = 0u64;
    for (index, (plan, disk, seeds, _)) in setups.into_iter().enumerate() {
        check(
            observer,
            Stage::Formation,
            0.1 + 0.6 * index as f64 / system.disks.len() as f64,
        )?;
        let outcome = evolve::evolve(&disk, &growth, &seeds, &frame_times);
        steps += outcome.steps;
        // After the gas: the High tier integrates the orbits; the Standard
        // tier applies its stability rule.
        let mut nbody = high_tier
            .then(|| {
                let starts: Vec<high::Start> = outcome
                    .planets
                    .iter()
                    .map(|p| {
                        let makeup = Makeup {
                            core_mearth: earths(p.core_msun),
                            envelope_mearth: earths(p.envelope_msun),
                            water_fraction: (p.water_msun / p.core_msun).clamp(0.0, 1.0),
                        };
                        let flux = sg_planet::flux_earth(plan.luminosity_lsun, p.a_au);
                        // Sizes when the gas clears, a few million years in.
                        let radius = sg_planet::radius(&makeup, flux, YOUNG_AGE_GYR);
                        high::Start {
                            id: p.id,
                            a_au: p.a_au,
                            mass_msun: p.mass_msun(),
                            radius_au: radius * EARTH_RADIUS_KM / KM_PER_AU,
                        }
                    })
                    .collect();
                high::integrate(
                    &starts,
                    plan.central_mass_msun,
                    central_radius_au(&plan, &system),
                    &mut dynamics_rng,
                    &params.formation.high,
                    NBODY_FRAMES,
                )
            })
            .flatten();
        let (settled, late): (Vec<Orbiter>, Vec<late::Merger>) = match &mut nbody {
            Some(stage) => {
                // The integrated planets as they ended, the small ones as
                // they were; the stability rule then applies to every pair
                // but those the integration has already settled.
                let mut orbiters: Vec<Orbiter> = stage
                    .orbits
                    .iter()
                    .map(|(id, el, mass)| Orbiter {
                        id: *id,
                        a_au: el.a.0,
                        mass_msun: *mass,
                    })
                    .collect();
                orbiters.extend(stage.passive.iter().map(|(id, a_au, mass)| Orbiter {
                    id: *id,
                    a_au: *a_au,
                    mass_msun: *mass,
                }));
                let active = stage.active.clone();
                let (settled, by_rule) =
                    late::settle_except(orbiters, plan.central_mass_msun, |x, y| {
                        active.contains(&x.id) && active.contains(&y.id)
                    });
                // Mergers by rule have no time of their own: they are put
                // at the end of the span.
                let span = stage.span_yr;
                stage
                    .mergers
                    .extend(by_rule.iter().map(|m| (span, m.survivor, m.absorbed)));
                let all = stage
                    .mergers
                    .iter()
                    .map(|(_, survivor, absorbed)| late::Merger {
                        survivor: *survivor,
                        absorbed: *absorbed,
                    })
                    .collect();
                (settled, all)
            }
            None => {
                if high_tier {
                    warnings.push(format!(
                        "{}: fewer than two planets of {} Earth masses or more, so no orbits \
                         were integrated; the late stage is the Standard tier's",
                        host_label(&system_name, &plan.designation),
                        params.formation.high.active_mass_mearth
                    ));
                }
                let orbiters: Vec<Orbiter> = outcome
                    .planets
                    .iter()
                    .map(|p| Orbiter {
                        id: p.id,
                        a_au: p.a_au,
                        mass_msun: p.mass_msun(),
                    })
                    .collect();
                late::settle(orbiters, plan.central_mass_msun)
            }
        };
        if let Some(stage) = &nbody {
            let label = host_label(&system_name, &plan.designation);
            if stage.integrated_yr < stage.span_yr {
                warnings.push(format!(
                    "{label}: orbits integrated for {:.0} of {:.0} thousand years before the \
                     step budget ran out; later instabilities are not captured",
                    stage.integrated_yr / 1.0e3,
                    stage.span_yr / 1.0e3,
                ));
            }
            if stage.energy_error > NBODY_ENERGY_TOLERANCE {
                warnings.push(format!(
                    "{label}: the orbit integration's energy error reached {:.1e}",
                    stage.energy_error
                ));
            }
        }

        // Rebuild the survivors: each takes its settled orbit and the
        // material of everything merged into it.
        let mut by_id: BTreeMap<u32, Embryo> =
            outcome.planets.iter().map(|p| (p.id, p.clone())).collect();
        let mut late_count: BTreeMap<u32, u32> = BTreeMap::new();
        for merger in &late {
            if let Some(absorbed) = by_id.remove(&merger.absorbed) {
                if let Some(survivor) = by_id.get_mut(&merger.survivor) {
                    survivor.core_msun += absorbed.core_msun;
                    survivor.envelope_msun += absorbed.envelope_msun;
                    survivor.water_msun += absorbed.water_msun;
                    survivor.mergers += absorbed.mergers + 1;
                    survivor.runaway |= absorbed.runaway;
                    survivor.resonance = None;
                }
                let inherited = late_count.remove(&merger.absorbed).unwrap_or(0);
                *late_count.entry(merger.survivor).or_insert(0) += inherited + 1;
            }
        }
        let mut planets: Vec<Formed> = settled
            .iter()
            .filter_map(|orbiter| {
                let mut embryo = by_id.remove(&orbiter.id)?;
                embryo.a_au = orbiter.a_au;
                Some(Formed {
                    late_mergers: late_count.get(&orbiter.id).copied().unwrap_or(0),
                    embryo,
                    orbit: nbody.as_ref().and_then(|stage| {
                        stage
                            .orbits
                            .iter()
                            .find(|(id, _, _)| *id == orbiter.id)
                            .map(|(_, el, _)| *el)
                    }),
                })
            })
            .collect();
        let belts = sweep_leftovers(&mut planets, &outcome.planetesimals, &plan, age_gyr * 1.0e9);
        planets.retain(|p| earths(p.embryo.mass_msun()) >= MIN_PLANET_MASS_MEARTH);
        let seeded = seeds.len() as u32;
        results.push(DiskResult {
            plan,
            disk,
            id_offset,
            outcome,
            planets,
            late,
            belts,
            nbody,
        });
        id_offset += seeded;
    }
    check(observer, Stage::Planets, 0.8)?;

    // Age the planets and write the snapshot.
    let mut angles = RngStream::new(params.seed, ModuleId::ORBIT_ANGLES);
    let mut bodies = Vec::new();
    let mut habitable_zones = Vec::new();
    let mut planet_zones = Vec::new();
    let mut disks_at_dispersal = Vec::new();
    for (index, result) in results.iter().enumerate() {
        let plan = &result.plan;
        let spec = result.disk.spec();
        let star_mass = plan.central_mass_msun;
        // Embryos are seeded out to this distance and migrate inward; a
        // resonance can hold the outermost a little outside it.
        let reach = 1.1
            * spec
                .outer_edge_au
                .min(SEED_OUTER_LIMIT_AU)
                .min(3.0 * spec.characteristic_radius_au);
        planet_zones.push(stars::planet_zone(plan, spec.inner_edge_au, reach));
        disks_at_dispersal.push(DiskSummary {
            id: DiskId(index as u32),
            host: plan.host,
            inner_au: spec.inner_edge_au,
            outer_au: spec.outer_edge_au,
            lifetime_myr: spec.dispersal_time_yr / 1.0e6,
            initial_mass_msun: spec.initial_mass_msun,
            dust_to_gas: spec.dust_to_gas,
            final_ice_line_au: result.disk.water_ice_line(spec.dispersal_time_yr),
            characteristic_radius_au: spec.characteristic_radius_au,
            viscous_time_myr: result.disk.viscous_time_yr() / 1.0e6,
        });
        let hz = match stars::habitable_zone(plan) {
            Ok(hz) => Some(hz),
            Err(err) => {
                warnings.push(format!(
                    "no habitable zone computed for {}: {err}",
                    host_label(&system_name, &plan.designation)
                ));
                None
            }
        };
        let unsettled = result.planets.iter().any(|p| p.late_mergers > 0);
        let prefix = host_label(&system_name, &plan.designation);
        for (order, formed) in result.planets.iter().enumerate() {
            let e = &formed.embryo;
            let id = BodyId(e.id + result.id_offset);
            let formed_makeup = Makeup {
                core_mearth: earths(e.core_msun),
                envelope_mearth: earths(e.envelope_msun),
                water_fraction: (e.water_msun / e.core_msun).clamp(0.0, 1.0),
            };
            let makeup = Makeup {
                envelope_mearth: sg_planet::envelope_after_escape(
                    &formed_makeup,
                    e.a_au,
                    star_mass,
                    plan.luminosity_lsun,
                    age_gyr,
                ),
                ..formed_makeup
            };
            let mass = makeup.mass_mearth();
            let flux = sg_planet::flux_earth(plan.luminosity_lsun, e.a_au);
            let radius = sg_planet::radius(&makeup, flux, age_gyr);
            let temp =
                sg_planet::equilibrium_temperature(plan.luminosity_lsun, e.a_au, ASSUMED_ALBEDO);
            let kind = sg_planet::classify(&makeup, temp);
            let f_env = makeup.envelope_fraction();
            let period_days = sg_dynamics::kepler::orbital_period(
                sg_core::units::Au(e.a_au),
                sg_core::units::SolarMass(star_mass),
            )
            .0;
            let locked =
                sg_planet::tidal_locking_time_yr(e.a_au, star_mass, mass, radius) < age_gyr * 1.0e9;
            let scale = if unsettled {
                ECCENTRICITY_SCALE.1
            } else {
                ECCENTRICITY_SCALE.0
            };
            // Drawn in every tier, so that the streams stay in step; the
            // High tier then uses the orbit it integrated instead.
            let drawn = Orbit {
                a_au: e.a_au,
                e: rayleigh(&mut angles, scale).min(0.6),
                i_deg: 0.0,
                long_asc_node_deg: angles.uniform_range(0.0, 360.0),
                arg_periapsis_deg: angles.uniform_range(0.0, 360.0),
                mean_anomaly_deg: angles.uniform_range(0.0, 360.0),
                period_days,
            };
            let orbit = match formed.orbit {
                Some(el) => Orbit {
                    e: el.e,
                    i_deg: el.i.to_degrees(),
                    long_asc_node_deg: el.long_asc_node.to_degrees(),
                    arg_periapsis_deg: el.arg_periapsis.to_degrees(),
                    mean_anomaly_deg: el.mean_anomaly.to_degrees(),
                    ..drawn
                },
                None => drawn,
            };
            let within = |inner: f64, outer: f64| e.a_au >= inner && e.a_au <= outer;
            let core_share = 1.0 - f_env;
            let rock_and_iron = core_share * (1.0 - makeup.water_fraction);

            let mut texture_key = [0u8; 12];
            texture_key[..8].copy_from_slice(&params.seed.to_le_bytes());
            texture_key[8..].copy_from_slice(&id.0.to_le_bytes());

            bodies.push(Body {
                id,
                name: names::planet_name(&prefix, order),
                kind: BodyKind::Planet,
                host: plan.host,
                orbit,
                mass_mearth: mass,
                radius_km: radius * EARTH_RADIUS_KM,
                // Earth's mean density, scaled.
                density_g_cm3: 5.514 * mass / (radius * radius * radius),
                surface_gravity_gees: (f_env < 0.1).then_some(mass / (radius * radius)),
                // Earth's escape velocity, scaled.
                escape_velocity_km_s: 11.186 * sqrt(mass / radius),
                albedo: ASSUMED_ALBEDO,
                equilibrium_temp_k: temp,
                composition: Composition {
                    metal_frac: rock_and_iron * (1.0 - sg_planet::EARTH_ROCK_FRACTION),
                    rock_frac: rock_and_iron * sg_planet::EARTH_ROCK_FRACTION,
                    water_frac: core_share * makeup.water_fraction,
                    volatiles_frac: 0.0,
                    h_he_frac: f_env,
                },
                rotation: Rotation {
                    period_hours: locked.then_some(period_days * 24.0),
                    obliquity_deg: None,
                    tidally_locked: locked,
                    spin_orbit_resonance: false,
                },
                atmosphere: (makeup.envelope_mearth > 0.0).then(|| Atmosphere {
                    surface_pressure_mbar: None,
                    min_molecular_weight: 2.0,
                    gases: Vec::new(),
                }),
                climate: None,
                class: class_of(kind),
                habitability: Habitability {
                    in_conservative_hz: hz
                        .as_ref()
                        .is_some_and(|z| within(z.conservative_inner_au, z.conservative_outer_au)),
                    in_optimistic_hz: hz
                        .as_ref()
                        .is_some_and(|z| within(z.optimistic_inner_au, z.optimistic_outer_au)),
                    surface_liquid_water: false,
                },
                rings: None,
                visual: VisualDescriptor {
                    texture_seed: fnv1a64(&texture_key),
                    ocean_frac: 0.0,
                    ice_frac: 0.0,
                    cloud_frac: 0.0,
                },
                formation: FormationSummary {
                    origin_a_au: e.origin_a_au,
                    mergers: e.mergers,
                    captured_gas: e.runaway,
                },
            });
        }
        habitable_zones.extend(hz);
    }
    check(observer, Stage::Assembly, 0.95)?;

    let mut timeline = timeline(
        &results,
        &bodies,
        &frame_times,
        end_yr,
        params.timeline.record_events,
    );
    // Moons and rings, after the planets (`moons.rs`).
    let moon_events = crate::moons::attach(&mut bodies, params.seed, |host| {
        results
            .iter()
            .find(|result| result.plan.host == host)
            .map(|result| result.plan.central_mass_msun)
    });
    if params.timeline.record_events {
        crate::moons::record(&mut timeline, moon_events);
    }
    let snapshot = SystemSnapshot {
        schema_version: SCHEMA_VERSION.to_string(),
        model_version: MODEL_VERSION.to_string(),
        params_hash: params_hash(params),
        age_gyr,
        hierarchy: system.hierarchy.clone(),
        bodies,
        belts: results.iter().flat_map(|r| r.belts.clone()).collect(),
        disks_at_dispersal,
        habitable_zones,
        planet_zones,
    };
    check(observer, Stage::Assembly, 1.0)?;

    Ok(SimulationResult {
        snapshot,
        timeline,
        diagnostics: Diagnostics {
            fidelity: params.fidelity,
            stellar_model: StellarModel::HurleyPolsTout2000,
            bit_deterministic: true,
            rng_draws: steps,
            rejected_samples: 0,
            warnings,
        },
    })
}

/// `Name`, or `Name AB` for the stars a disk orbits in a multiple system.
fn host_label(system_name: &str, designation: &str) -> String {
    if designation.is_empty() {
        system_name.to_string()
    } else {
        format!("{system_name} {designation}")
    }
}

/// Builds the timeline: the growth model's events and frames for every
/// disk, in years converted to Myr, ending with the settled system.
fn timeline(
    results: &[DiskResult],
    bodies: &[Body],
    frame_times: &[f64],
    end_yr: f64,
    record_events: bool,
) -> EventTimeline {
    let myr = |yr: f64| yr / 1.0e6;
    let mut events: Vec<(f64, usize, EventKind)> = Vec::new();
    if record_events {
        for (index, result) in results.iter().enumerate() {
            let disk = DiskId(index as u32);
            let body = |id: u32| BodyId(id + result.id_offset);
            events.push((0.0, index, EventKind::DiskFormed { disk }));
            for (t_yr, event) in &result.outcome.events {
                let kind = match *event {
                    GrowthEvent::Seeded { id, a_au } => EventKind::EmbryoInjected {
                        body: body(id),
                        host: result.plan.host,
                        a_au,
                        e: 0.0,
                    },
                    GrowthEvent::IsolationReached { id } => {
                        EventKind::PebbleIsolationReached { body: body(id) }
                    }
                    GrowthEvent::RunawayGas { id } => {
                        EventKind::RunawayGasAccretion { body: body(id) }
                    }
                    GrowthEvent::GapOpened { id } => EventKind::GapOpened { body: body(id) },
                    GrowthEvent::ResonanceCaptured {
                        inner,
                        outer,
                        ratio,
                    } => EventKind::ResonanceCaptured {
                        a: body(inner),
                        b: body(outer),
                        ratio,
                    },
                    GrowthEvent::Merged { survivor, absorbed } => EventKind::Collision {
                        a: body(survivor),
                        b: body(absorbed),
                        result: body(survivor),
                    },
                    // Not an event of the schema: the planet simply stops.
                    GrowthEvent::ReachedInnerEdge { .. } => continue,
                };
                events.push((myr(*t_yr), index, kind));
            }
            // The ice line's position at each frame.
            for frame in &result.outcome.frames {
                events.push((
                    myr(frame.t_yr),
                    index,
                    EventKind::IceLineMoved {
                        disk,
                        species: Species::H2o,
                        r_au: frame.ice_line_au,
                    },
                ));
            }
            events.push((
                myr(result.disk.gas_gone_yr()),
                index,
                EventKind::DiskDispersed { disk },
            ));
            match &result.nbody {
                // The N-body stage follows the growth model: its events
                // are at the times the integration found.
                Some(stage) => {
                    for (t_yr, survivor, absorbed) in &stage.mergers {
                        events.push((
                            myr(end_yr + t_yr),
                            index,
                            EventKind::Collision {
                                a: body(*survivor),
                                b: body(*absorbed),
                                result: body(*survivor),
                            },
                        ));
                    }
                    for (t_yr, id, how) in &stage.lost {
                        let kind = match how {
                            high::Lost::IntoStar => EventKind::FellIntoStar { body: body(*id) },
                            // (Planets swallowed by a giant are among the
                            // mergers, not here.)
                            high::Lost::Ejected | high::Lost::IntoGiant => {
                                EventKind::Ejected { body: body(*id) }
                            }
                        };
                        events.push((myr(end_yr + t_yr), index, kind));
                    }
                }
                // Late mergers happen over the following millions of
                // years; they are placed just before the end of the
                // timeline.
                None => {
                    for merger in &result.late {
                        events.push((
                            myr(end_yr) * 0.999,
                            index,
                            EventKind::Collision {
                                a: body(merger.survivor),
                                b: body(merger.absorbed),
                                result: body(merger.survivor),
                            },
                        ));
                    }
                }
            }
        }
        events.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
    }

    let mut keyframes: Vec<Keyframe> = frame_times
        .iter()
        .enumerate()
        .map(|(k, t_yr)| {
            let mut all: Vec<KeyframeBody> = Vec::new();
            for result in results {
                if let Some(frame) = result.outcome.frames.get(k) {
                    all.extend(frame.bodies.iter().map(|(id, a_au, mass)| KeyframeBody {
                        id: BodyId(id + result.id_offset),
                        host: result.plan.host,
                        a_au: *a_au,
                        e: 0.0,
                        mass_mearth: earths(*mass),
                    }));
                }
            }
            all.sort_by_key(|b| b.id);
            Keyframe {
                t_myr: myr(*t_yr),
                bodies: all,
            }
        })
        .collect();
    // The N-body stage, where there is one, continues the timeline.
    let span_yr = results
        .iter()
        .filter_map(|result| result.nbody.as_ref())
        .map(|stage| stage.span_yr)
        .fold(0.0f64, f64::max);
    let frame_count = results
        .iter()
        .filter_map(|result| result.nbody.as_ref())
        .map(|stage| stage.frames.len())
        .max()
        .unwrap_or(0);
    // The last of them is replaced by the finished system below.
    for k in 0..frame_count.saturating_sub(1) {
        let mut all: Vec<KeyframeBody> = Vec::new();
        for result in results {
            let Some(stage) = result.nbody.as_ref() else {
                // A disk whose planets were not integrated: they wait as
                // the growth model left them.
                if let Some(frame) = result.outcome.frames.last() {
                    all.extend(frame.bodies.iter().map(|(id, a_au, mass)| KeyframeBody {
                        id: BodyId(id + result.id_offset),
                        host: result.plan.host,
                        a_au: *a_au,
                        e: 0.0,
                        mass_mearth: earths(*mass),
                    }));
                }
                continue;
            };
            let Some(frame) = stage.frames.get(k) else {
                continue;
            };
            all.extend(frame.iter().map(|(id, a_au, e, mass)| KeyframeBody {
                id: BodyId(id + result.id_offset),
                host: result.plan.host,
                a_au: *a_au,
                e: e.clamp(0.0, 0.999),
                mass_mearth: earths(*mass),
            }));
            // The small planets, which were not integrated.
            all.extend(stage.passive.iter().map(|(id, a_au, mass)| KeyframeBody {
                id: BodyId(id + result.id_offset),
                host: result.plan.host,
                a_au: *a_au,
                e: 0.0,
                mass_mearth: earths(*mass),
            }));
        }
        all.sort_by_key(|b| b.id);
        keyframes.push(Keyframe {
            t_myr: myr(end_yr + span_yr * (k + 1) as f64 / frame_count as f64),
            bodies: all,
        });
    }
    let end_yr = end_yr + span_yr;
    // The settled system closes the timeline.
    let mut settled: Vec<KeyframeBody> = bodies
        .iter()
        .map(|b| KeyframeBody {
            id: b.id,
            host: b.host,
            a_au: b.orbit.a_au,
            e: b.orbit.e,
            mass_mearth: b.mass_mearth,
        })
        .collect();
    settled.sort_by_key(|b| b.id);
    keyframes.push(Keyframe {
        t_myr: myr(end_yr),
        bodies: settled,
    });

    EventTimeline {
        events: events
            .into_iter()
            .map(|(t_myr, _, kind)| Event { t_myr, kind })
            .collect(),
        keyframes,
    }
}
