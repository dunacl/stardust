//! Classic tier: runs `sg-classic` and maps its output onto the schema.
//!
//! Two paths share the mapping:
//!
//! - **Faithful** ([`simulate_faithful`]): one star of 0.2–1.5 M☉ (or of a
//!   mass StarGen draws), exactly as StarGen models it, including Fogg's
//!   (1985) luminosity. This is what the golden comparison covers.
//! - **Extended** ([`simulate_extended`]): several stars, a star outside
//!   that mass range, a sampled system or a stated metallicity. The stars
//!   come from [`crate::stars`] (main-sequence evolution, stability limits)
//!   and the Classic accretion model is run once in every zone where
//!   planets can exist, through `sg_classic::generate_in_disk`. This is
//!   neither StarGen nor the modern formation model of M4: it is Dole's
//!   accretion placed in a modern stellar system, and it applies that
//!   model's mass scalings outside the range they were written for.
//!
//! Besides the physics, which is `sg-classic`'s, this module adds:
//!
//! - on the faithful path, stellar radius, effective temperature and
//!   spectral class from `sg-stellar`, using the Classic luminosity so the
//!   star is consistent with the planets it produced;
//! - the Kopparapu et al. (2014) habitable zone, replacing StarGen's
//!   constant ecosphere;
//! - orbital angles (Ω, ω, M₀), drawn from a ChaCha stream, since the model
//!   is one-dimensional. Inclinations are zero;
//! - names, from [`crate::names`];
//! - a clock for the timeline: each disk's accretion events are spread
//!   evenly, in order, over `timeline.classic_window_myr`.

use crate::{
    names, params_hash, stars, Control, Diagnostics, SimError, SimObserver, SimulationResult,
    Stage, StellarModel, MODEL_VERSION,
};
use sg_classic::consts::SUN_MASS_IN_EARTH_MASSES;
use sg_classic::{ClassicEvent, ClassicPlanet, ClassicType};
use sg_core::hash::fnv1a64;
use sg_core::rng::{ModuleId, RngStream};
use sg_core::units::{Kelvin, SolarLuminosity, SolarMass};
use sg_schema::*;
use std::collections::BTreeMap;

const STAR: StarId = StarId(0);
const DISK: DiskId = DiskId(0);

fn check(observer: &mut dyn SimObserver, stage: Stage, fraction: f64) -> Result<(), SimError> {
    match observer.progress(stage, fraction) {
        Control::Continue => Ok(()),
        Control::Cancel => Err(SimError::Cancelled),
    }
}

/// The single star's mass on the faithful path, or `None` to let the
/// Classic model draw one. Only called when [`is_stargen_case`] holds.
fn stargen_mass(params: &SimulationParams) -> Option<f64> {
    match &params.system {
        SystemSpec::Auto {
            primary_mass_msun, ..
        } => *primary_mass_msun,
        SystemSpec::Explicit(StarNode::Star(spec)) => Some(spec.mass_msun),
        SystemSpec::Explicit(StarNode::Pair { .. }) => None,
    }
}

fn classic_params(params: &SimulationParams) -> sg_classic::ClassicParams {
    let c = &params.formation.classic;
    sg_classic::ClassicParams {
        dust_density_coeff: c.dust_density_coeff,
        gas_dust_ratio: c.gas_dust_ratio,
        crit_mass_coeff: c.crit_mass_coeff_msun,
        alpha: c.alpha,
        n: c.n,
        eccentricity_coeff: c.eccentricity_coeff,
        cloud_eccentricity: c.cloud_eccentricity,
        protoplanet_mass_msun: c.protoplanet_mass_msun,
        compute_gases: params.toggles.atmospheres,
    }
}

fn validate(params: &SimulationParams) -> Result<(), SimError> {
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

fn planet_class(t: ClassicType) -> PlanetClass {
    match t {
        ClassicType::Unknown => PlanetClass::Unknown,
        ClassicType::Rock => PlanetClass::Rock,
        ClassicType::Venusian => PlanetClass::Venusian,
        ClassicType::Terrestrial => PlanetClass::Terrestrial,
        ClassicType::GasGiant => PlanetClass::Jovian,
        ClassicType::Martian => PlanetClass::Martian,
        ClassicType::Water => PlanetClass::Water,
        ClassicType::Ice => PlanetClass::Ice,
        ClassicType::SubGasGiant => PlanetClass::SubJovian,
        ClassicType::SubSubGasGiant => PlanetClass::GasDwarf,
        ClassicType::Asteroids => PlanetClass::Asteroids,
        ClassicType::OneFace => PlanetClass::OneFace,
    }
}

fn finite(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}

fn body(
    p: &ClassicPlanet,
    id: BodyId,
    host: HostRef,
    name: String,
    seed: u64,
    angles: &mut RngStream,
    hz: Option<&HabitableZone>,
) -> Body {
    let giant = p.planet_type.is_giant();
    let has_air = giant || p.surf_pressure_mb > 0.0;
    let h_he_frac = p.gas_mass_msun / p.mass_msun;
    let year_hours = p.orb_period_days * 24.0;

    let atmosphere = has_air.then(|| Atmosphere {
        surface_pressure_mbar: finite(p.surf_pressure_mb),
        min_molecular_weight: p.molec_weight,
        gases: p
            .atmosphere
            .iter()
            .map(|g| GasFraction {
                symbol: g.symbol.to_string(),
                partial_pressure_mbar: g.surf_pressure_mb,
            })
            .collect(),
    });

    let climate = (!giant).then(|| Climate {
        surface_temp_k: p.surf_temp_k,
        day_temp_k: p.high_temp_k,
        night_temp_k: p.low_temp_k,
        max_temp_k: p.max_temp_k,
        min_temp_k: p.min_temp_k,
        greenhouse_rise_k: p.greenhs_rise_k,
        runaway_greenhouse: p.greenhouse_effect,
        water_boiling_point_k: (p.surf_pressure_mb > 0.0)
            .then_some(p.boil_point_k)
            .and_then(finite),
        ocean_frac: p.hydrosphere_frac,
        ice_frac: p.ice_cover_frac,
        cloud_frac: p.cloud_cover_frac,
    });

    let within = |inner: f64, outer: f64| p.a_au >= inner && p.a_au <= outer;
    let habitability = Habitability {
        in_conservative_hz: hz
            .is_some_and(|z| within(z.conservative_inner_au, z.conservative_outer_au)),
        in_optimistic_hz: hz.is_some_and(|z| within(z.optimistic_inner_au, z.optimistic_outer_au)),
        surface_liquid_water: !giant && p.hydrosphere_frac > 0.0,
    };

    let mut texture_key = [0u8; 12];
    texture_key[..8].copy_from_slice(&seed.to_le_bytes());
    texture_key[8..].copy_from_slice(&id.0.to_le_bytes());

    Body {
        id,
        name,
        kind: BodyKind::Planet,
        host,
        orbit: Orbit {
            a_au: p.a_au,
            e: p.e,
            i_deg: 0.0,
            long_asc_node_deg: angles.uniform_range(0.0, 360.0),
            arg_periapsis_deg: angles.uniform_range(0.0, 360.0),
            mean_anomaly_deg: angles.uniform_range(0.0, 360.0),
            period_days: p.orb_period_days,
        },
        mass_mearth: p.mass_msun * SUN_MASS_IN_EARTH_MASSES,
        radius_km: p.radius_km,
        density_g_cm3: p.density_g_cm3,
        surface_gravity_gees: finite(p.surf_grav_gees),
        escape_velocity_km_s: p.esc_velocity_cm_s / 1.0e5,
        albedo: p.albedo,
        equilibrium_temp_k: p.estimated_temp_k,
        composition: Composition {
            metal_frac: 0.0,
            rock_frac: 1.0 - h_he_frac,
            water_frac: 0.0,
            volatiles_frac: 0.0,
            h_he_frac,
        },
        rotation: Rotation {
            period_hours: Some(p.day_hours),
            obliquity_deg: Some(p.axial_tilt_deg),
            // Same test as the Classic model: equal to the hour.
            tidally_locked: !p.resonant_period && (p.day_hours as i64) == (year_hours as i64),
            spin_orbit_resonance: p.resonant_period,
        },
        atmosphere,
        climate,
        class: planet_class(p.planet_type),
        habitability,
        rings: None,
        visual: VisualDescriptor {
            texture_seed: fnv1a64(&texture_key),
            ocean_frac: if giant { 0.0 } else { p.hydrosphere_frac },
            ice_frac: if giant { 0.0 } else { p.ice_cover_frac },
            cloud_frac: p.cloud_cover_frac,
        },
        formation: FormationSummary {
            origin_a_au: p.origin_a_au,
            mergers: p.mergers,
            captured_gas: p.gas_giant,
        },
    }
}

/// The accretion history of one disk.
struct DiskRun<'a> {
    disk: DiskId,
    host: HostRef,
    /// Added to the model's nucleus ids to make body ids unique in the system.
    id_offset: u32,
    events: &'a [ClassicEvent],
}

impl DiskRun<'_> {
    /// One more than the largest body id this disk uses.
    fn id_end(&self) -> u32 {
        let injected = self.events.iter().filter_map(|event| match event {
            ClassicEvent::NucleusInjected { id, .. } => Some(*id + 1),
            _ => None,
        });
        self.id_offset + injected.max().unwrap_or(0)
    }
}

/// Builds the timeline from each disk's ordered accretion events.
///
/// `nucleus_mass_msun` is the mass shown for a nucleus between its injection
/// and its first sweep; the two events are adjacent in time. The disks'
/// histories are independent, so they are interleaved by their nominal
/// times.
fn timeline(disks: &[DiskRun], params: &TimelineParams, nucleus_mass_msun: f64) -> EventTimeline {
    let window = params.classic_window_myr;
    let mearth = |msun: f64| msun * SUN_MASS_IN_EARTH_MASSES;

    // (time, disk index, event), ordered by time; ties keep disk order.
    let mut ordered: Vec<(f64, usize, &ClassicEvent)> = Vec::new();
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

        let kind = match *event {
            ClassicEvent::NucleusInjected { id, a_au, e } => {
                state.insert(
                    global(id),
                    KeyframeBody {
                        id: BodyId(global(id)),
                        host: run.host,
                        a_au,
                        e,
                        mass_mearth: mearth(nucleus_mass_msun),
                    },
                );
                EventKind::EmbryoInjected {
                    body: BodyId(global(id)),
                    host: run.host,
                    a_au,
                    e,
                }
            }
            ClassicEvent::DustSwept {
                id,
                swept_mass_msun,
                inner_au,
                outer_au,
                mass_after_msun,
            } => {
                if let Some(b) = state.get_mut(&global(id)) {
                    b.mass_mearth = mearth(mass_after_msun);
                }
                EventKind::DustSwept {
                    body: BodyId(global(id)),
                    dm_mearth: mearth(swept_mass_msun),
                    inner_au,
                    outer_au,
                }
            }
            ClassicEvent::GasCaptureBegan { id } => EventKind::RunawayGasAccretion {
                body: BodyId(global(id)),
            },
            ClassicEvent::Collision {
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
                EventKind::Collision {
                    a: BodyId(global(survivor)),
                    b: BodyId(global(absorbed)),
                    result: BodyId(global(survivor)),
                }
            }
        };
        if params.record_events {
            events.push(Event { t_myr, kind });
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

/// True if the parameters describe what StarGen itself models: one star,
/// of solar metallicity, with a mass in its range or left for it to draw.
fn is_stargen_case(params: &SimulationParams) -> bool {
    let mass_in_range = match &params.system {
        SystemSpec::Auto {
            primary_mass_msun, ..
        } => primary_mass_msun.map_or(true, |m| CLASSIC_MASS_RANGE_MSUN.contains(&m)),
        SystemSpec::Explicit(StarNode::Star(spec)) => {
            CLASSIC_MASS_RANGE_MSUN.contains(&spec.mass_msun)
        }
        SystemSpec::Explicit(StarNode::Pair { .. }) => false,
    };
    stars::is_single(&params.system) && mass_in_range && params.metallicity_feh.is_none()
}

/// Stellar masses StarGen models, M☉.
const CLASSIC_MASS_RANGE_MSUN: std::ops::RangeInclusive<f64> = 0.2..=1.5;

pub(crate) fn simulate(
    params: &SimulationParams,
    observer: &mut dyn SimObserver,
) -> Result<SimulationResult, SimError> {
    validate(params)?;
    if is_stargen_case(params) {
        simulate_faithful(params, observer)
    } else {
        simulate_extended(params, observer)
    }
}

/// Several stars, or a star StarGen does not model: modern stars, with the
/// Classic accretion model run in every zone open to planets.
fn simulate_extended(
    params: &SimulationParams,
    observer: &mut dyn SimObserver,
) -> Result<SimulationResult, SimError> {
    check(observer, Stage::Stellar, 0.0)?;
    let system_name = names::star_name(params.seed);
    let mut stellar_rng = RngStream::new(params.seed, ModuleId::STELLAR);
    let system = stars::build(params, &mut stellar_rng, &system_name)?;
    let mut warnings = system.warnings.clone();

    check(observer, Stage::Formation, 0.1)?;
    let classic = classic_params(params);
    let mut seeds = RngStream::new(params.seed, ModuleId::FORMATION);
    let mut runs = Vec::with_capacity(system.disks.len());
    for disk in &system.disks {
        // The Classic generator takes a non-zero 32-bit seed.
        let seed = loop {
            let candidate = seeds.next_u64() & 0xFFFF_FFFF;
            if candidate != 0 {
                break candidate;
            }
        };
        let spec = sg_classic::DiskSpec {
            central_mass_msun: disk.central_mass_msun,
            luminosity_lsun: disk.luminosity_lsun,
            age_yr: system.age_gyr * 1.0e9,
            main_seq_life_yr: disk.lifetime_gyr * 1.0e9,
            inner_limit_au: disk.inner_limit_au,
            outer_limit_au: disk.outer_limit_au,
        };
        runs.push(sg_classic::generate_in_disk(seed, &spec, &classic)?);
    }
    check(observer, Stage::Planets, 0.8)?;

    let mut angles = RngStream::new(params.seed, ModuleId::ORBIT_ANGLES);
    let mut bodies = Vec::new();
    let mut habitable_zones = Vec::new();
    let mut planet_zones = Vec::new();
    let mut disk_runs = Vec::new();
    let mut id_offset = 0u32;
    let mut out_of_range: Vec<f64> = Vec::new();
    for (index, (disk, run)) in system.disks.iter().zip(&runs).enumerate() {
        // The zone the model could place planets in; none if the limits
        // leave no room.
        let mass = disk.central_mass_msun;
        let inner =
            sg_classic::accrete::nearest_planet(mass).max(disk.inner_limit_au.unwrap_or(0.0));
        let reach = sg_classic::accrete::farthest_planet(mass);
        let outer = disk.outer_limit_au.map_or(reach, |limit| limit.min(reach));
        if inner >= outer {
            continue;
        }
        planet_zones.push(stars::planet_zone(disk, inner, outer));
        if !CLASSIC_MASS_RANGE_MSUN.contains(&mass) {
            out_of_range.push(mass);
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
            disk: DiskId(index as u32),
            host: disk.host,
            id_offset,
            events: &run.events,
        };
        let prefix = host_label(&system_name, &disk.designation);
        for (order, planet) in run.planets.iter().enumerate() {
            bodies.push(body(
                planet,
                BodyId(planet.id + id_offset),
                disk.host,
                names::planet_name(&prefix, order),
                params.seed,
                &mut angles,
                hz.as_ref(),
            ));
        }
        id_offset = disk_run.id_end();
        disk_runs.push(disk_run);
        habitable_zones.extend(hz);
    }
    if !out_of_range.is_empty() {
        let list: Vec<String> = out_of_range.iter().map(|m| format!("{m:.2}")).collect();
        warnings.push(format!(
            "the accretion model was written for stars of 0.2–1.5 M☉; here it also ran around \
             {} M☉",
            list.join(", ")
        ));
    }
    check(observer, Stage::Assembly, 0.95)?;

    let snapshot = SystemSnapshot {
        schema_version: SCHEMA_VERSION.to_string(),
        model_version: MODEL_VERSION.to_string(),
        params_hash: params_hash(params),
        age_gyr: system.age_gyr,
        hierarchy: system.hierarchy.clone(),
        bodies,
        belts: Vec::new(),
        disks_at_dispersal: Vec::new(),
        habitable_zones,
        planet_zones,
    };
    let timeline = timeline(
        &disk_runs,
        &params.timeline,
        params.formation.classic.protoplanet_mass_msun,
    );
    check(observer, Stage::Assembly, 1.0)?;

    Ok(SimulationResult {
        snapshot,
        timeline,
        diagnostics: Diagnostics {
            fidelity: Fidelity::Classic,
            stellar_model: StellarModel::HurleyPolsTout2000,
            bit_deterministic: true,
            rng_draws: runs.iter().map(|run| run.rng_draws).sum(),
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

/// One star as StarGen models it.
fn simulate_faithful(
    params: &SimulationParams,
    observer: &mut dyn SimObserver,
) -> Result<SimulationResult, SimError> {
    // StarGen draws a mass in [0.7, 1.4] M☉ when given one outside its
    // range; 0 asks for that on purpose.
    let mass = stargen_mass(params).unwrap_or(0.0);
    let mut warnings = Vec::new();

    check(observer, Stage::Stellar, 0.0)?;
    check(observer, Stage::Formation, 0.05)?;
    let system = sg_classic::generate(
        params.seed,
        mass,
        params.age_gyr.map(|gyr| gyr * 1.0e9),
        &classic_params(params),
    )?;
    check(observer, Stage::Planets, 0.8)?;

    let star_mass = SolarMass(system.star.mass_msun);
    let luminosity = SolarLuminosity(system.star.luminosity_lsun);
    let radius = sg_stellar::main_sequence_radius(star_mass);
    let teff: Kelvin = sg_stellar::effective_temperature(luminosity, radius);
    let age_gyr = system.star.age_yr / 1.0e9;

    let habitable_zones: Vec<HabitableZone> = match sg_stellar::habitable_zone(luminosity, teff) {
        Ok(hz) => vec![HabitableZone {
            host: HostRef::Star(STAR),
            optimistic_inner_au: hz.recent_venus.0,
            conservative_inner_au: hz.runaway_greenhouse.0,
            conservative_outer_au: hz.maximum_greenhouse.0,
            optimistic_outer_au: hz.early_mars.0,
        }],
        Err(err) => {
            warnings.push(format!("no habitable zone computed: {err}"));
            Vec::new()
        }
    };

    let star_name = names::star_name(params.seed);
    let mut angles = RngStream::new(params.seed, ModuleId::ORBIT_ANGLES);
    let bodies = system
        .planets
        .iter()
        .enumerate()
        .map(|(index, p)| {
            let name = names::planet_name(&star_name, index);
            body(
                p,
                BodyId(p.id),
                HostRef::Star(STAR),
                name,
                params.seed,
                &mut angles,
                habitable_zones.first(),
            )
        })
        .collect();

    check(observer, Stage::Assembly, 0.95)?;

    let snapshot = SystemSnapshot {
        schema_version: SCHEMA_VERSION.to_string(),
        model_version: MODEL_VERSION.to_string(),
        params_hash: params_hash(params),
        age_gyr,
        hierarchy: HierarchyNode::Star(Star {
            id: STAR,
            name: star_name,
            mass_msun: system.star.mass_msun,
            luminosity_lsun: system.star.luminosity_lsun,
            radius_rsun: radius.0,
            teff_k: teff.0,
            spectral_class: sg_stellar::spectral_class(teff).to_string(),
            color_linear_rgb: sg_stellar::blackbody_linear_srgb(teff),
            age_gyr,
            main_sequence_lifetime_gyr: system.star.main_seq_life_yr / 1.0e9,
        }),
        bodies,
        belts: Vec::new(),
        disks_at_dispersal: Vec::new(),
        habitable_zones,
        planet_zones: vec![PlanetZone {
            host: HostRef::Star(STAR),
            inner_au: sg_classic::accrete::nearest_planet(system.star.mass_msun),
            outer_au: sg_classic::accrete::farthest_planet(system.star.mass_msun),
            limited_inside: false,
            limited_outside: false,
        }],
    };

    let timeline = timeline(
        &[DiskRun {
            disk: DISK,
            host: HostRef::Star(STAR),
            id_offset: 0,
            events: &system.events,
        }],
        &params.timeline,
        params.formation.classic.protoplanet_mass_msun,
    );
    check(observer, Stage::Assembly, 1.0)?;

    Ok(SimulationResult {
        snapshot,
        timeline,
        diagnostics: Diagnostics {
            fidelity: Fidelity::Classic,
            stellar_model: StellarModel::Fogg1985,
            bit_deterministic: true,
            rng_draws: system.rng_draws,
            rejected_samples: 0,
            warnings,
        },
    })
}
