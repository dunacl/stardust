//! Classic tier: runs `sg-classic` and maps its output onto the schema.
//!
//! The physics is entirely `sg-classic`'s. This module adds only what the
//! Classic model does not have:
//!
//! - stellar radius, effective temperature and spectral class, from
//!   `sg-stellar`, using the Classic luminosity so the star is consistent
//!   with the planets it produced;
//! - the Kopparapu et al. (2014) habitable zone, replacing StarGen's
//!   constant ecosphere;
//! - orbital angles (Ω, ω, M₀), drawn from a ChaCha stream, since the model
//!   is one-dimensional. Inclinations are zero;
//! - a clock for the timeline: accretion events are spread evenly, in order,
//!   over `timeline.classic_window_myr`.

use crate::{
    params_hash, Control, Diagnostics, SimError, SimObserver, SimulationResult, Stage,
    MODEL_VERSION,
};
use sg_classic::consts::SUN_MASS_IN_EARTH_MASSES;
use sg_classic::{ClassicEvent, ClassicPlanet, ClassicSystem, ClassicType};
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

/// The single star's mass, or `None` to let the Classic model draw one.
fn star_mass(params: &SimulationParams) -> Result<Option<f64>, SimError> {
    let unsupported = |what: &str| {
        Err(SimError::Unsupported(
            Fidelity::Classic,
            format!("{what}; the Classic model handles single stars only"),
        ))
    };
    match &params.system {
        SystemSpec::Auto {
            primary_mass_msun,
            multiplicity,
        } => match multiplicity {
            None | Some(Multiplicity::Single) => Ok(*primary_mass_msun),
            Some(_) => unsupported("multiple stars requested"),
        },
        SystemSpec::Explicit(StarNode::Star(spec)) => Ok(Some(spec.mass_msun)),
        SystemSpec::Explicit(StarNode::Pair { .. }) => unsupported("a stellar pair was given"),
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

fn body(p: &ClassicPlanet, seed: u64, angles: &mut RngStream, hz: Option<&HabitableZone>) -> Body {
    let id = BodyId(p.id);
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
    texture_key[8..].copy_from_slice(&p.id.to_le_bytes());

    Body {
        id,
        kind: BodyKind::Planet,
        host: HostRef::Star(STAR),
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
            period_hours: p.day_hours,
            obliquity_deg: p.axial_tilt_deg,
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

/// Builds the timeline from the ordered accretion events.
///
/// `nucleus_mass_msun` is the mass shown for a nucleus between its injection
/// and its first sweep; the two events are adjacent in time.
fn timeline(
    system: &ClassicSystem,
    params: &TimelineParams,
    nucleus_mass_msun: f64,
) -> EventTimeline {
    let window = params.classic_window_myr;
    let n = system.events.len();
    let time_of = |index: usize| window * (index + 1) as f64 / (n + 1) as f64;
    let mearth = |msun: f64| msun * SUN_MASS_IN_EARTH_MASSES;

    let mut events = Vec::new();
    if params.record_events {
        events.reserve(n + 2);
        events.push(Event {
            t_myr: 0.0,
            kind: EventKind::DiskFormed { disk: DISK },
        });
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

    for (index, event) in system.events.iter().enumerate() {
        let t_myr = time_of(index);
        while next_keyframe <= keyframe_count && keyframe_time(next_keyframe) < t_myr {
            keyframes.push(snapshot_state(&state, keyframe_time(next_keyframe)));
            next_keyframe += 1;
        }

        let kind = match *event {
            ClassicEvent::NucleusInjected { id, a_au, e } => {
                state.insert(
                    id,
                    KeyframeBody {
                        id: BodyId(id),
                        a_au,
                        e,
                        mass_mearth: mearth(nucleus_mass_msun),
                    },
                );
                EventKind::EmbryoInjected {
                    body: BodyId(id),
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
                if let Some(b) = state.get_mut(&id) {
                    b.mass_mearth = mearth(mass_after_msun);
                }
                EventKind::DustSwept {
                    body: BodyId(id),
                    dm_mearth: mearth(swept_mass_msun),
                    inner_au,
                    outer_au,
                }
            }
            ClassicEvent::GasCaptureBegan { id } => {
                EventKind::RunawayGasAccretion { body: BodyId(id) }
            }
            ClassicEvent::Collision {
                survivor,
                absorbed,
                a_au,
                e,
                mass_msun,
            } => {
                state.remove(&absorbed);
                if let Some(b) = state.get_mut(&survivor) {
                    b.a_au = a_au;
                    b.e = e;
                    b.mass_mearth = mearth(mass_msun);
                }
                EventKind::Collision {
                    a: BodyId(survivor),
                    b: BodyId(absorbed),
                    result: BodyId(survivor),
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
        events.push(Event {
            t_myr: window,
            kind: EventKind::DiskDispersed { disk: DISK },
        });
    }

    EventTimeline { events, keyframes }
}

pub(crate) fn simulate(
    params: &SimulationParams,
    observer: &mut dyn SimObserver,
) -> Result<SimulationResult, SimError> {
    validate(params)?;
    let requested_mass = star_mass(params)?;
    // StarGen draws a mass in [0.7, 1.4] M☉ when given one outside its range;
    // 0 asks for that on purpose.
    let mass = requested_mass.unwrap_or(0.0);
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

    if requested_mass.is_some_and(|m| !(0.2..=1.5).contains(&m)) {
        warnings.push(format!(
            "stellar mass {mass} M☉ is outside the Classic model's range [0.2, 1.5]; \
             as in StarGen, {:.3} M☉ was drawn instead",
            system.star.mass_msun
        ));
    }

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

    let mut angles = RngStream::new(params.seed, ModuleId::ORBIT_ANGLES);
    let bodies = system
        .planets
        .iter()
        .map(|p| body(p, params.seed, &mut angles, habitable_zones.first()))
        .collect();

    check(observer, Stage::Assembly, 0.95)?;

    let snapshot = SystemSnapshot {
        schema_version: SCHEMA_VERSION.to_string(),
        model_version: MODEL_VERSION.to_string(),
        params_hash: params_hash(params),
        age_gyr,
        hierarchy: HierarchyNode::Star(Star {
            id: STAR,
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
    };

    let timeline = timeline(
        &system,
        &params.timeline,
        params.formation.classic.protoplanet_mass_msun,
    );
    check(observer, Stage::Assembly, 1.0)?;

    Ok(SimulationResult {
        snapshot,
        timeline,
        diagnostics: Diagnostics {
            fidelity: Fidelity::Classic,
            bit_deterministic: true,
            rng_draws: system.rng_draws,
            rejected_samples: 0,
            warnings,
        },
    })
}
