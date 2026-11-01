//! Classic tier: runs [`sg_accrete`] and maps its output onto the schema.
//!
//! Dole's accretion model, with Fogg's scalings, is run once in every zone
//! where planets can exist, around stars from [`crate::stars`]. There is one
//! path for every system (spec gap 5). Fogg's range is single stars of
//! 0.6–1.3 M☉; beyond it the model is an extrapolation, and a warning says so.
//!
//! Besides the physics, which is `sg-accrete`'s, this module adds:
//!
//! - the Kopparapu et al. (2014) habitable zone;
//! - orbital angles (Ω, ω, M₀), drawn from a ChaCha stream, since the model
//!   is one-dimensional. Inclinations are zero;
//! - names, from [`crate::names`];
//! - a clock for the timeline: each disk's accretion events are spread
//!   evenly, in order, over `timeline.classic_window_myr`.

use crate::{
    names, params_hash, stars, Control, Diagnostics, SimError, SimObserver, SimulationResult,
    Stage, StellarModel, MODEL_VERSION,
};
use sg_accrete::{AccretionParams, Properties, RunSpec};
use sg_core::constants::SOLAR_MASS_IN_EARTH_MASSES;
use sg_core::hash::fnv1a64;
use sg_core::rng::{ModuleId, RngStream};
use sg_core::units::{Au, SolarMass};
use sg_planet::PlanetKind;
use sg_schema::*;
use std::collections::BTreeMap;

/// Dole's bounds on the accretion (D p.17), kept for every star (F p.504).
const BOUNDS_AU: (f64, f64) = (0.3, 50.0);
/// The cloud has no edge of its own; his density is negligible out here.
const CLOUD_EDGE_AU: f64 = 1000.0;
/// Below this a body is rubble, not a world; `sg-render` draws it as a belt. A
/// naming choice, between Ceres (2e-4) and the Moon (0.012).
const ASTEROID_LIMIT_MEARTH: f64 = 0.001;
/// Air thinner than this reads as a Martian world. A naming choice, between
/// Mars (6 mb) and the Earth.
const MARTIAN_PRESSURE_MB: f64 = 100.0;
/// Stellar masses Fogg's scalings were written for, M☉.
const FOGG_RANGE_MSUN: std::ops::RangeInclusive<f64> = 0.6..=1.3;

fn check(observer: &mut dyn SimObserver, stage: Stage, fraction: f64) -> Result<(), SimError> {
    match observer.progress(stage, fraction) {
        Control::Continue => Ok(()),
        Control::Cancel => Err(SimError::Cancelled),
    }
}

fn accretion_params(params: &SimulationParams) -> AccretionParams {
    let c = &params.formation.classic;
    AccretionParams {
        central_density_msun_au3: c.dust_density_coeff,
        gas_dust_ratio: c.gas_dust_ratio,
        cloud_eccentricity: c.cloud_eccentricity,
        alpha: c.alpha,
        n: c.n,
        critical_mass_coeff: c.crit_mass_coeff_msun,
        nucleus_mass_msun: c.protoplanet_mass_msun,
        eccentricity_exponent: c.eccentricity_coeff,
        ..AccretionParams::set4()
    }
}

fn validate(params: &SimulationParams) -> Result<(), SimError> {
    if params.seed == 0 {
        return Err(SimError::InvalidParams(
            "seed 0 is reserved: use a non-zero seed".to_string(),
        ));
    }
    let c = &params.formation.classic;
    let positive = [
        ("dust_density_coeff", c.dust_density_coeff),
        ("gas_dust_ratio", c.gas_dust_ratio),
        ("crit_mass_coeff_msun", c.crit_mass_coeff_msun),
        ("alpha", c.alpha),
        ("n", c.n),
        ("eccentricity_coeff", c.eccentricity_coeff),
        ("protoplanet_mass_msun", c.protoplanet_mass_msun),
        (
            "timeline.classic_window_myr",
            params.timeline.classic_window_myr,
        ),
    ];
    for (name, value) in positive {
        if !(value.is_finite() && value > 0.0) {
            return Err(SimError::InvalidParams(format!(
                "{name} must be positive and finite, got {value}"
            )));
        }
    }
    if !(0.0..1.0).contains(&c.cloud_eccentricity) {
        return Err(SimError::InvalidParams(format!(
            "cloud_eccentricity must be in [0, 1), got {}",
            c.cloud_eccentricity
        )));
    }
    if let Some(age) = params.age_gyr {
        if !(age.is_finite() && age > 0.0) {
            return Err(SimError::InvalidParams(format!(
                "age_gyr must be positive and finite, got {age}"
            )));
        }
    }
    Ok(())
}

/// The schema's Classic classes: Fogg's words where his climate applies,
/// otherwise `sg-planet`'s kind under its Classic name. Cards and the renderer
/// tell the tiers apart by these, so Standard's classes must not appear here.
fn planet_class(p: &Properties) -> PlanetClass {
    if p.makeup.envelope_mearth == 0.0 && p.makeup.mass_mearth() < ASTEROID_LIMIT_MEARTH {
        return PlanetClass::Asteroids;
    }
    match &p.climate {
        Some(c) if c.runaway => return PlanetClass::Venusian,
        Some(c) if has_liquid_water(c) => return PlanetClass::Terrestrial,
        Some(c) if c.pressure_mb < MARTIAN_PRESSURE_MB => return PlanetClass::Martian,
        _ => {}
    }
    match p.kind {
        PlanetKind::GasGiant | PlanetKind::HotJupiter => PlanetClass::Jovian,
        PlanetKind::IceGiant => PlanetClass::SubJovian,
        PlanetKind::SubNeptune => PlanetClass::GasDwarf,
        PlanetKind::WaterWorld => PlanetClass::Water,
        PlanetKind::IceWorld => PlanetClass::Ice,
        PlanetKind::Rocky | PlanetKind::SuperEarth | PlanetKind::LavaWorld => PlanetClass::Rock,
    }
}

fn has_liquid_water(c: &sg_accrete::Climate) -> bool {
    c.ocean_frac > 0.0 && c.surface_k > 273.0 && c.surface_k < c.boiling_k
}

/// What all the planets of one disk share.
struct Surroundings<'a> {
    star_mass_msun: f64,
    host: HostRef,
    seed: u64,
    hz: Option<&'a HabitableZone>,
}

fn body(
    planet: &sg_accrete::Planet,
    p: &Properties,
    around: &Surroundings,
    id: BodyId,
    name: String,
    angles: &mut RngStream,
) -> Body {
    let Surroundings {
        star_mass_msun,
        host,
        seed,
        hz,
    } = *around;
    let mass = p.makeup.mass_mearth();
    let f_env = p.makeup.envelope_fraction();
    let period_days =
        sg_dynamics::kepler::orbital_period(Au(planet.a_au), SolarMass(star_mass_msun)).0;
    let core_share = 1.0 - f_env;
    let rock_and_iron = core_share * (1.0 - p.makeup.water_fraction);

    let atmosphere = p.atmosphere.then(|| Atmosphere {
        surface_pressure_mbar: p.climate.as_ref().map(|c| c.pressure_mb),
        min_molecular_weight: if p.makeup.envelope_mearth > 0.0 {
            2.0
        } else {
            28.0
        },
        gases: Vec::new(),
    });
    let climate = p.climate.as_ref().map(|c| Climate {
        surface_temp_k: c.surface_k,
        day_temp_k: c.surface_k,
        night_temp_k: c.surface_k,
        max_temp_k: c.surface_k,
        min_temp_k: c.surface_k,
        greenhouse_rise_k: c.greenhouse_k,
        runaway_greenhouse: c.runaway,
        water_boiling_point_k: Some(c.boiling_k),
        ocean_frac: c.ocean_frac,
        ice_frac: c.ice_frac,
        cloud_frac: c.cloud_frac,
    });
    let within = |inner: f64, outer: f64| planet.a_au >= inner && planet.a_au <= outer;
    let habitability = Habitability {
        in_conservative_hz: hz
            .is_some_and(|z| within(z.conservative_inner_au, z.conservative_outer_au)),
        in_optimistic_hz: hz.is_some_and(|z| within(z.optimistic_inner_au, z.optimistic_outer_au)),
        surface_liquid_water: p.climate.as_ref().is_some_and(has_liquid_water),
    };

    let mut texture_key = [0u8; 12];
    texture_key[..8].copy_from_slice(&seed.to_le_bytes());
    texture_key[8..].copy_from_slice(&id.0.to_le_bytes());
    let cover = |f: fn(&sg_accrete::Climate) -> f64| p.climate.as_ref().map_or(0.0, f);

    Body {
        id,
        name,
        designation: String::new(),
        kind: BodyKind::Planet,
        host,
        orbit: Orbit {
            a_au: planet.a_au,
            e: planet.e,
            i_deg: 0.0,
            long_asc_node_deg: angles.uniform_range(0.0, 360.0),
            arg_periapsis_deg: angles.uniform_range(0.0, 360.0),
            mean_anomaly_deg: angles.uniform_range(0.0, 360.0),
            period_days,
        },
        mass_mearth: mass,
        radius_km: p.radius_rearth * sg_core::constants::EARTH_RADIUS_KM,
        density_g_cm3: p.density_g_cm3,
        surface_gravity_gees: (f_env < 0.1).then_some(mass / (p.radius_rearth * p.radius_rearth)),
        escape_velocity_km_s: p.escape_km_s,
        albedo: p.albedo,
        equilibrium_temp_k: p.equilibrium_k,
        composition: Composition {
            metal_frac: rock_and_iron * (1.0 - sg_planet::EARTH_ROCK_FRACTION),
            rock_frac: rock_and_iron * sg_planet::EARTH_ROCK_FRACTION,
            water_frac: core_share * p.makeup.water_fraction,
            volatiles_frac: 0.0,
            h_he_frac: f_env,
        },
        rotation: Rotation {
            period_hours: Some(p.rotation_hours),
            obliquity_deg: None,
            tidally_locked: p.locked && !p.resonant,
            spin_orbit_resonance: p.resonant,
        },
        atmosphere,
        climate,
        class: planet_class(p),
        habitability,
        rings: None,
        visual: VisualDescriptor {
            texture_seed: fnv1a64(&texture_key),
            ocean_frac: cover(|c| c.ocean_frac),
            ice_frac: cover(|c| c.ice_frac),
            cloud_frac: cover(|c| c.cloud_frac),
        },
        formation: FormationSummary {
            origin_a_au: planet.origin_a_au,
            mergers: planet.mergers,
            captured_gas: planet.is_giant(),
        },
    }
}

/// The accretion history of one disk.
struct DiskRun<'a> {
    disk: DiskId,
    host: HostRef,
    /// Added to the model's nucleus ids to make body ids unique in the system.
    id_offset: u32,
    events: &'a [sg_accrete::Event],
}

impl DiskRun<'_> {
    /// One more than the largest body id this disk uses.
    fn id_end(&self) -> u32 {
        let injected = self.events.iter().filter_map(|event| match event {
            sg_accrete::Event::Grew { id, .. } => Some(*id + 1),
            _ => None,
        });
        self.id_offset + injected.max().unwrap_or(0)
    }
}

/// Builds the timeline from each disk's ordered accretion events.
///
/// The disks' histories are independent, so they are interleaved by their
/// nominal times.
fn timeline(disks: &[DiskRun], params: &TimelineParams) -> EventTimeline {
    let window = params.classic_window_myr;
    let mearth = |msun: f64| msun * SOLAR_MASS_IN_EARTH_MASSES;

    // (time, disk index, event), ordered by time; ties keep disk order.
    let mut ordered: Vec<(f64, usize, &sg_accrete::Event)> = Vec::new();
    for (index, run) in disks.iter().enumerate() {
        let n = run.events.len();
        for (k, event) in run.events.iter().enumerate() {
            ordered.push((window * (k + 1) as f64 / (n + 1) as f64, index, event));
        }
    }
    ordered.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));

    let mut events = Vec::new();
    if params.record_events {
        events.reserve(ordered.len() + 2 * disks.len());
        for run in disks {
            events.push(Event {
                t_myr: 0.0,
                kind: EventKind::DiskFormed { disk: run.disk },
            });
        }
    }

    // Replay the events to recover the state of every body through time.
    let keyframe_count = params.keyframe_count as usize;
    let mut keyframes = Vec::with_capacity(keyframe_count);
    let mut state: BTreeMap<u32, KeyframeBody> = BTreeMap::new();
    let mut next_keyframe = 1usize;
    let keyframe_time = |j: usize| window * j as f64 / keyframe_count as f64;
    let snapshot_state = |state: &BTreeMap<u32, KeyframeBody>, t_myr: f64| Keyframe {
        t_myr,
        bodies: state.values().cloned().collect(),
    };

    for (t_myr, index, event) in ordered {
        let run = &disks[index];
        let global = |id: u32| id + run.id_offset;
        while next_keyframe <= keyframe_count && keyframe_time(next_keyframe) < t_myr {
            keyframes.push(snapshot_state(&state, keyframe_time(next_keyframe)));
            next_keyframe += 1;
        }

        match *event {
            sg_accrete::Event::Grew {
                id,
                a_au,
                e,
                mass_msun,
                inner_au,
                outer_au,
                captured_gas,
            } => {
                let body = BodyId(global(id));
                state.insert(
                    global(id),
                    KeyframeBody {
                        id: body,
                        host: run.host,
                        a_au,
                        e,
                        mass_mearth: mearth(mass_msun),
                    },
                );
                if params.record_events {
                    events.push(Event {
                        t_myr,
                        kind: EventKind::EmbryoInjected {
                            body,
                            host: run.host,
                            a_au,
                            e,
                        },
                    });
                    events.push(Event {
                        t_myr,
                        kind: EventKind::DustSwept {
                            body,
                            dm_mearth: mearth(mass_msun),
                            inner_au,
                            outer_au,
                        },
                    });
                    if captured_gas {
                        events.push(Event {
                            t_myr,
                            kind: EventKind::RunawayGasAccretion { body },
                        });
                    }
                }
            }
            sg_accrete::Event::Coalesced {
                survivor,
                absorbed,
                a_au,
                e,
                mass_msun,
            } => {
                state.remove(&global(absorbed));
                if let Some(b) = state.get_mut(&global(survivor)) {
                    b.a_au = a_au;
                    b.e = e;
                    b.mass_mearth = mearth(mass_msun);
                }
                if params.record_events {
                    events.push(Event {
                        t_myr,
                        kind: EventKind::Collision {
                            a: BodyId(global(survivor)),
                            b: BodyId(global(absorbed)),
                            result: BodyId(global(survivor)),
                        },
                    });
                }
            }
        }
    }
    while next_keyframe <= keyframe_count {
        keyframes.push(snapshot_state(&state, keyframe_time(next_keyframe)));
        next_keyframe += 1;
    }

    if params.record_events {
        for run in disks {
            events.push(Event {
                t_myr: window,
                kind: EventKind::DiskDispersed { disk: run.disk },
            });
        }
    }

    EventTimeline { events, keyframes }
}

/// Stars from [`crate::stars`], with the accretion model run in every zone open
/// to planets.
pub(crate) fn simulate(
    params: &SimulationParams,
    observer: &mut dyn SimObserver,
) -> Result<SimulationResult, SimError> {
    validate(params)?;
    check(observer, Stage::Stellar, 0.0)?;
    let system_name = names::star_name(params.seed);
    let mut stellar_rng = RngStream::new(params.seed, ModuleId::STELLAR);
    let system = stars::build(params, &mut stellar_rng, &system_name)?;
    let mut warnings = system.warnings.clone();

    check(observer, Stage::Formation, 0.1)?;
    let accretion = accretion_params(params);
    let mut seeds = RngStream::new(params.seed, ModuleId::ACCRETION);
    let mut described = RngStream::new(params.seed, ModuleId::PLANET);
    let mut angles = RngStream::new(params.seed, ModuleId::ORBIT_ANGLES);
    let mut bodies = Vec::new();
    let mut habitable_zones = Vec::new();
    let mut planet_zones = Vec::new();
    let mut runs = Vec::new();
    let mut out_of_range: Vec<f64> = Vec::new();
    for (index, disk) in system.disks.iter().enumerate() {
        // Every disk takes a draw, so one never shifts another's stream.
        let mut rng = RngStream::new(seeds.next_u64(), ModuleId::ACCRETION);
        let inner = BOUNDS_AU.0.max(disk.inner_limit_au.unwrap_or(0.0));
        let outer = BOUNDS_AU
            .1
            .min(disk.outer_limit_au.unwrap_or(f64::INFINITY));
        if inner >= outer {
            continue;
        }
        let spec = RunSpec {
            star_mass_msun: disk.central_mass_msun,
            luminosity_lsun: disk.luminosity_lsun,
            inner_bound_au: inner,
            outer_bound_au: outer,
            material_inner_au: disk.inner_limit_au.unwrap_or(0.0),
            material_outer_au: disk.outer_limit_au.unwrap_or(CLOUD_EDGE_AU),
        };
        runs.push((
            index,
            disk,
            sg_accrete::accrete(&mut rng, &accretion, &spec)?,
            inner,
            outer,
        ));
    }
    check(observer, Stage::Planets, 0.8)?;

    let mut disk_runs = Vec::new();
    let mut id_offset = 0u32;
    for (index, disk, run, inner, outer) in &runs {
        planet_zones.push(stars::planet_zone(disk, *inner, *outer));
        if !FOGG_RANGE_MSUN.contains(&disk.central_mass_msun) {
            out_of_range.push(disk.central_mass_msun);
        }
        let hz = match stars::habitable_zone(disk) {
            Ok(hz) => Some(hz),
            Err(err) => {
                warnings.push(format!(
                    "no habitable zone computed for {}: {err}",
                    host_label(&system_name, &disk.designation)
                ));
                None
            }
        };
        let disk_run = DiskRun {
            disk: DiskId(*index as u32),
            host: disk.host,
            id_offset,
            events: &run.events,
        };
        let star = sg_accrete::Star {
            mass_msun: disk.central_mass_msun,
            luminosity_lsun: disk.luminosity_lsun,
            age_gyr: system.age_gyr,
        };
        let around = Surroundings {
            star_mass_msun: disk.central_mass_msun,
            host: disk.host,
            seed: params.seed,
            hz: hz.as_ref(),
        };
        let prefix = host_label(&system_name, &disk.designation);
        for (order, planet) in run.planets.iter().enumerate() {
            let properties = sg_accrete::describe(planet, &star, &mut described);
            bodies.push(body(
                planet,
                &properties,
                &around,
                BodyId(planet.id + id_offset),
                names::planet_name(&prefix, order),
                &mut angles,
            ));
        }
        id_offset = disk_run.id_end();
        disk_runs.push(disk_run);
        habitable_zones.extend(hz);
    }
    if !out_of_range.is_empty() {
        let list: Vec<String> = out_of_range.iter().map(|m| format!("{m:.2}")).collect();
        warnings.push(format!(
            "Fogg's scalings were written for single stars of 0.6–1.3 M☉; here the model also \
             ran around {} M☉",
            list.join(", ")
        ));
        if out_of_range
            .iter()
            .any(|m| *m < FOGG_RANGE_MSUN.start() * 0.99)
        {
            warnings.push(
                "around stars below 0.6 M☉ these scalings give giants that real red dwarfs \
                 rarely have; use the Standard tier for such stars"
                    .to_string(),
            );
        }
    }
    check(observer, Stage::Assembly, 0.95)?;

    // Moons and rings: an addition to the model's planets (`moons.rs`).
    let moon_events = crate::moons::attach(&mut bodies, params.seed, |host| {
        system
            .disks
            .iter()
            .find(|disk| disk.host == host)
            .map(|disk| disk.central_mass_msun)
    });

    let snapshot = SystemSnapshot {
        schema_version: SCHEMA_VERSION.to_string(),
        model_version: MODEL_VERSION.to_string(),
        params_hash: params_hash(params),
        age_gyr: system.age_gyr,
        hierarchy: system.hierarchy.clone(),
        bodies,
        belts: Vec::new(),
        comets: Vec::new(),
        comet_reservoirs: Vec::new(),
        disks_at_dispersal: Vec::new(),
        habitable_zones,
        planet_zones,
    };
    let mut timeline = timeline(&disk_runs, &params.timeline);
    if params.timeline.record_events {
        crate::moons::record(&mut timeline, moon_events);
    }
    check(observer, Stage::Assembly, 1.0)?;

    Ok(SimulationResult {
        snapshot,
        timeline,
        diagnostics: Diagnostics {
            fidelity: Fidelity::Classic,
            stellar_model: StellarModel::HurleyPolsTout2000,
            bit_deterministic: true,
            rng_draws: runs.iter().map(|r| u64::from(r.2.nuclei) * 2).sum(),
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
