//! Moons and rings for a finished system's planets, in every tier.
//!
//! The physics and the draws are in `sg_planet::moons`; this module turns
//! them into bodies of the snapshot. Giants (a tenth or more hydrogen and
//! helium, or the Classic tier's Jovian classes) get regular moons and
//! perhaps bright rings; rocky planets that have been through a collision
//! may get one large moon.
//!
//! In the Classic tier this is an addition: Dole's and Fogg's model has no
//! moons, and its planets are left as it made them.

use sg_core::constants::EARTH_RADIUS_KM;
use sg_core::hash::fnv1a64;
use sg_core::math::sqrt;
use sg_core::rng::{ModuleId, RngStream};
use sg_planet::moons::{self, Host, Origin};
use sg_schema::*;

/// A planet with at least this share of hydrogen and helium is a giant
/// for the purpose of moons: it had a disk of its own to grow them in.
const GIANT_ENVELOPE_FRACTION: f64 = 0.1;
/// Rocky planets lighter than this are not given impact moons, M⊕: the
/// simulations behind the probability are of Earth-like planets.
const MIN_MOON_HOST_MEARTH: f64 = 0.1;
/// Moons are numbered from here, above any planet's id.
pub(crate) const MOON_ID_BASE: u32 = 100_000;
const KM_PER_AU: f64 = 1.495_978_707e8;

fn is_giant(body: &Body) -> bool {
    body.composition.h_he_frac >= GIANT_ENVELOPE_FRACTION
        || matches!(body.class, PlanetClass::Jovian | PlanetClass::SubJovian)
}

fn is_rocky(body: &Body) -> bool {
    body.composition.h_he_frac <= 0.0
        && body.mass_mearth >= MIN_MOON_HOST_MEARTH
        && !matches!(
            body.class,
            PlanetClass::Jovian
                | PlanetClass::SubJovian
                | PlanetClass::GasDwarf
                | PlanetClass::Asteroids
        )
}

/// Gives the planets in `bodies` their rings and appends their moons after
/// them. `host_mass` is the mass, M☉, of what a planet orbits. Returns the
/// moons formed, for the timeline. Every planet takes the same draws
/// whatever comes of them, so one planet's luck does not change another's.
pub(crate) fn attach(
    bodies: &mut Vec<Body>,
    seed: u64,
    host_mass: impl Fn(HostRef) -> Option<f64>,
) -> Vec<EventKind> {
    let mut rng = RngStream::new(seed, ModuleId::MOONS);
    let mut made: Vec<Body> = Vec::new();
    let mut events = Vec::new();
    for planet in bodies.iter_mut() {
        let star_mass = host_mass(planet.host).unwrap_or(1.0);
        let radius_rearth = planet.radius_km / EARTH_RADIUS_KM;
        let host = Host {
            mass_mearth: planet.mass_mearth,
            radius_rearth,
            hill_km: moons::hill_radius_km(
                planet.orbit.a_au,
                planet.orbit.e,
                planet.mass_mearth,
                star_mass,
            ),
            temp_k: planet.equilibrium_temp_k,
            tidally_locked: planet.rotation.tidally_locked || planet.rotation.spin_orbit_resonance,
        };
        let regular = moons::regular_moons(&host, &mut rng);
        let ring = moons::rings(&host, &mut rng);
        let impact = moons::impact_moon(&host, planet.formation.mergers, &mut rng);
        let giant = is_giant(planet);
        let own: Vec<moons::Moon> = if giant {
            regular
        } else if is_rocky(planet) {
            impact.into_iter().collect()
        } else {
            Vec::new()
        };
        if giant {
            planet.rings = ring.map(|ring| Rings {
                inner_radius_km: ring.inner_km,
                outer_radius_km: ring.outer_km,
                optical_depth: ring.optical_depth,
            });
        }
        for (k, moon) in own.iter().enumerate() {
            let id = BodyId(MOON_ID_BASE + made.len() as u32);
            let radius = sg_planet::solid_radius(moon.mass_mearth, moon.water_fraction);
            let rock = 1.0 - moon.water_fraction;
            let period = moons::moon_period_days(moon.a_km, planet.mass_mearth);
            let mut key = [0u8; 12];
            key[..8].copy_from_slice(&seed.to_le_bytes());
            key[8..].copy_from_slice(&id.0.to_le_bytes());
            events.push(EventKind::MoonFormed {
                moon: id,
                host: planet.id,
                mechanism: match moon.origin {
                    Origin::CircumplanetaryDisk => MoonOrigin::CircumplanetaryDisk,
                    Origin::GiantImpact => MoonOrigin::GiantImpact,
                },
            });
            made.push(Body {
                id,
                name: crate::names::moon_name(&planet.name, k),
                designation: String::new(),
                kind: BodyKind::Moon,
                host: HostRef::Body(planet.id),
                orbit: Orbit {
                    a_au: moon.a_km / KM_PER_AU,
                    e: moon.e,
                    // In the planet's equatorial plane, which is all the
                    // model says about it.
                    i_deg: 0.0,
                    long_asc_node_deg: 0.0,
                    arg_periapsis_deg: rng.uniform_range(0.0, 360.0),
                    mean_anomaly_deg: rng.uniform_range(0.0, 360.0),
                    period_days: period,
                },
                mass_mearth: moon.mass_mearth,
                radius_km: radius * EARTH_RADIUS_KM,
                // Earth's density, gravity and escape velocity, scaled.
                density_g_cm3: 5.514 * moon.mass_mearth / (radius * radius * radius),
                surface_gravity_gees: Some(moon.mass_mearth / (radius * radius)),
                escape_velocity_km_s: 11.186 * sqrt(moon.mass_mearth / radius),
                albedo: planet.albedo,
                equilibrium_temp_k: planet.equilibrium_temp_k,
                minerals: Vec::new(),
                composition: Composition {
                    metal_frac: rock * (1.0 - sg_planet::EARTH_ROCK_FRACTION),
                    rock_frac: rock * sg_planet::EARTH_ROCK_FRACTION,
                    water_frac: moon.water_fraction,
                    volatiles_frac: 0.0,
                    h_he_frac: 0.0,
                },
                // Moons this close keep one face to their planet.
                rotation: Rotation {
                    period_hours: Some(period * 24.0),
                    obliquity_deg: None,
                    tidally_locked: true,
                    spin_orbit_resonance: false,
                },
                atmosphere: None,
                climate: None,
                class: if moon.water_fraction >= 0.1 {
                    PlanetClass::IceWorld
                } else {
                    PlanetClass::Rocky
                },
                habitability: Habitability {
                    surface_liquid_water: false,
                    ..planet.habitability.clone()
                },
                rings: None,
                visual: VisualDescriptor {
                    texture_seed: fnv1a64(&key),
                    ocean_frac: 0.0,
                    ice_frac: 0.0,
                    cloud_frac: 0.0,
                },
                formation: FormationSummary {
                    origin_a_au: planet.orbit.a_au,
                    mergers: 0,
                    captured_gas: false,
                },
            });
        }
    }
    // Planets first, in order outward; then their moons.
    bodies.extend(made);
    events
}

/// Adds the moons' formation to the end of a timeline's events. They are
/// not in the keyframes, which follow the planets.
pub(crate) fn record(timeline: &mut EventTimeline, events: Vec<EventKind>) {
    let end = timeline.keyframes.last().map_or(0.0, |k| k.t_myr);
    timeline
        .events
        .extend(events.into_iter().map(|kind| Event { t_myr: end, kind }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulate;

    fn standard(seed: u64) -> SystemSnapshot {
        simulate(&SimulationParams {
            seed,
            fidelity: Fidelity::Standard,
            ..SimulationParams::default()
        })
        .expect("standard run")
        .snapshot
    }

    #[test]
    fn moons_follow_the_planets_and_orbit_inside_their_hill_spheres() {
        let mut moons_seen = 0;
        for seed in [222, 269, 287, 503] {
            let snapshot = standard(seed);
            let first_moon = snapshot
                .bodies
                .iter()
                .position(|b| b.kind == BodyKind::Moon)
                .unwrap_or(snapshot.bodies.len());
            // Planets, then moons; ids unique.
            assert!(snapshot.bodies[..first_moon]
                .iter()
                .all(|b| b.kind == BodyKind::Planet));
            assert!(snapshot.bodies[first_moon..]
                .iter()
                .all(|b| b.kind == BodyKind::Moon));
            let mut ids: Vec<u32> = snapshot.bodies.iter().map(|b| b.id.0).collect();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), snapshot.bodies.len());
            for moon in &snapshot.bodies[first_moon..] {
                moons_seen += 1;
                let HostRef::Body(host) = moon.host else {
                    panic!("a moon orbits a planet");
                };
                let planet = snapshot.bodies.iter().find(|b| b.id == host).expect("host");
                assert_eq!(planet.kind, BodyKind::Planet);
                assert!(!planet.rotation.tidally_locked);
                let a_km = moon.orbit.a_au * KM_PER_AU;
                let hill = moons::hill_radius_km(
                    planet.orbit.a_au,
                    planet.orbit.e,
                    planet.mass_mearth,
                    1.0,
                );
                assert!(a_km > 2.0 * planet.radius_km && a_km < 0.49 * hill);
                assert!(moon.mass_mearth < 0.05 * planet.mass_mearth);
                assert!(moon.designation.starts_with(&planet.designation));
                assert!(moon.radius_km > 0.0 && moon.orbit.period_days > 0.0);
            }
        }
        assert!(moons_seen >= 10, "{moons_seen}");
    }

    #[cfg(feature = "classic")]
    #[test]
    fn classic_planets_are_untouched_and_cold_giants_can_have_rings() {
        let mut ringed = 0;
        let mut moons_seen = 0;
        for seed in 1..40 {
            let snapshot = simulate(&SimulationParams {
                seed,
                ..SimulationParams::default()
            })
            .expect("classic run")
            .snapshot;
            for body in &snapshot.bodies {
                match body.kind {
                    BodyKind::Moon => moons_seen += 1,
                    _ => {
                        if let Some(rings) = &body.rings {
                            ringed += 1;
                            assert!(is_giant(body) && body.equilibrium_temp_k < 150.0);
                            assert!(rings.inner_radius_km > body.radius_km);
                            // The Roche limit for ice, whatever the planet's density.
                            let density = body.density_g_cm3;
                            let roche = sg_planet::moons::roche_limit_radii(density, 0.93);
                            assert!(rings.outer_radius_km <= roche * body.radius_km * 1.01);
                        }
                    }
                }
            }
        }
        assert!(
            ringed > 0 && moons_seen > 0,
            "{ringed} ringed, {moons_seen} moons"
        );
    }

    #[test]
    fn moon_names_use_roman_numerals() {
        assert_eq!(crate::names::moon_name("Vorus f", 0), "Vorus f I");
        assert_eq!(crate::names::moon_name("Vorus f", 3), "Vorus f IV");
        assert_eq!(crate::names::moon_name("Vorus f", 11), "Vorus f moon 12");
    }
}
