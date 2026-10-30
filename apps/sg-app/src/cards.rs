//! Data cards: the facts about a selected star or planet (PLAN.md §8.5).
//!
//! Pure text formatting over the snapshot, so it is unit-tested without a
//! window.

use sg_render::belt::is_belt;
use sg_schema::{Body, DiskSummary, HabitableZone, MoonOrigin, PlanetClass, PlanetZone, Star};

/// A titled list of label/value rows, plus free-text notes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Card {
    pub title: String,
    pub subtitle: String,
    pub sections: Vec<Section>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Section {
    pub heading: &'static str,
    pub rows: Vec<(&'static str, String)>,
}

pub fn class_name(class: PlanetClass) -> &'static str {
    match class {
        PlanetClass::Asteroids => "Asteroid field",
        PlanetClass::Rock => "Airless rock",
        PlanetClass::Venusian => "Venusian",
        PlanetClass::Terrestrial => "Terrestrial",
        PlanetClass::Martian => "Martian",
        PlanetClass::Water => "Ocean world",
        PlanetClass::Ice => "Ice world",
        PlanetClass::OneFace => "Tidally locked world",
        PlanetClass::GasDwarf => "Gas dwarf",
        PlanetClass::SubJovian => "Sub-Jovian",
        PlanetClass::Jovian => "Jovian",
        PlanetClass::Unknown => "Unclassified",
        PlanetClass::Rocky => "Rocky planet",
        PlanetClass::SuperEarth => "Super-Earth",
        PlanetClass::LavaWorld => "Lava world",
        PlanetClass::WaterWorld => "Water world",
        PlanetClass::IceWorld => "Ice world",
        PlanetClass::SubNeptune => "Sub-Neptune",
        PlanetClass::IceGiant => "Ice giant",
        PlanetClass::GasGiant => "Gas giant",
        PlanetClass::HotJupiter => "Hot Jupiter",
    }
}

/// A duration in the most readable unit.
pub fn duration(hours: f64) -> String {
    if hours < 72.0 {
        format!("{hours:.1} h")
    } else if hours < 24.0 * 365.25 * 2.0 {
        format!("{:.1} days", hours / 24.0)
    } else {
        format!("{:.2} years", hours / (24.0 * 365.25))
    }
}

/// `1st`, `2nd`, `3rd`, `4th`, … `11th`, `12th`, `13th`, … `21st`.
pub fn ordinal(n: usize) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// A mass in Earth masses: three decimals, or scientific notation when
/// that would round to nothing.
pub fn mass(mass_mearth: f64) -> String {
    if mass_mearth < 0.01 {
        format!("{mass_mearth:.1e}")
    } else if mass_mearth < 30.0 {
        format!("{mass_mearth:.3}")
    } else {
        format!("{mass_mearth:.0}")
    }
}

/// For giants, the mass in Jupiters as well: `(1.83 × Jupiter)`.
fn in_jupiters(mass_mearth: f64) -> String {
    if mass_mearth >= 30.0 {
        format!(" ({:.2} × Jupiter)", mass_mearth / 317.8)
    } else {
        String::new()
    }
}

fn celsius(kelvin: f64) -> String {
    format!("{kelvin:.0} K ({:+.0} °C)", kelvin - 273.15)
}

/// A fraction as a percentage that does not round a small share to zero.
fn share(fraction: f64) -> String {
    if fraction < 0.01 {
        format!("{:.2}%", fraction * 100.0)
    } else {
        percent(fraction)
    }
}

/// True for the classes the Standard tier assigns.
fn is_standard_class(class: PlanetClass) -> bool {
    matches!(
        class,
        PlanetClass::Rocky
            | PlanetClass::SuperEarth
            | PlanetClass::LavaWorld
            | PlanetClass::WaterWorld
            | PlanetClass::IceWorld
            | PlanetClass::SubNeptune
            | PlanetClass::IceGiant
            | PlanetClass::GasGiant
            | PlanetClass::HotJupiter
    )
}

fn percent(fraction: f64) -> String {
    format!("{:.0}%", fraction * 100.0)
}

/// A star's place in a multiple system, for its card.
#[derive(Debug, Clone, PartialEq)]
pub struct Companion {
    /// Names of the star or stars it orbits with.
    pub name: String,
    pub separation_au: f64,
    pub eccentricity: f64,
    pub period_days: f64,
    /// Combined mass of the companion side, M☉.
    pub mass_msun: f64,
}

pub fn star_card(
    star: &Star,
    hz: Option<&HabitableZone>,
    zone: Option<&PlanetZone>,
    planets: usize,
    companion: Option<&Companion>,
    disk: Option<&DiskSummary>,
) -> Card {
    let mut sections = vec![Section {
        heading: "Star",
        rows: vec![
            ("Mass", format!("{:.2} M☉", star.mass_msun)),
            ("Luminosity", format!("{:.3} L☉", star.luminosity_lsun)),
            ("Radius", format!("{:.2} R☉", star.radius_rsun)),
            ("Temperature", format!("{:.0} K", star.teff_k)),
            ("Age", format!("{:.2} Gyr", star.age_gyr)),
            (
                "Main-sequence life",
                format!("{:.1} Gyr", star.main_sequence_lifetime_gyr),
            ),
            ("Planets", planets.to_string()),
        ],
    }];
    let mut notes = Vec::new();
    if let Some(companion) = companion {
        sections.push(Section {
            heading: "Companion",
            rows: vec![
                ("Orbits with", companion.name.clone()),
                ("Companion mass", format!("{:.2} M☉", companion.mass_msun)),
                ("Separation", format!("{} AU", au(companion.separation_au))),
                ("Eccentricity", format!("{:.2}", companion.eccentricity)),
                ("Period", duration(companion.period_days * 24.0)),
            ],
        });
        match zone {
            Some(zone) if zone.limited_outside => notes.push(format!(
                "Its companion leaves stable orbits for planets only within {} AU.",
                au(zone.outer_au)
            )),
            Some(_) => {}
            None => notes.push(
                "Its companion is too close for this star to keep planets of its own.".into(),
            ),
        }
    }
    if let Some(disk) = disk {
        sections.push(Section {
            heading: "Its disk, long gone",
            rows: vec![
                ("Gas mass", format!("{:.3} M☉", disk.initial_mass_msun)),
                (
                    "Dust",
                    format!(
                        "{:.0} Earth masses",
                        disk.initial_mass_msun * disk.dust_to_gas * 332_946.0
                    ),
                ),
                ("Lasted", format!("{:.1} Myr", disk.lifetime_myr)),
                (
                    "Ice line at the end",
                    format!("{} AU", au(disk.final_ice_line_au)),
                ),
            ],
        });
    }
    if let Some(hz) = hz {
        sections.push(Section {
            heading: "Habitable zone",
            rows: vec![
                (
                    "Conservative",
                    format!(
                        "{:.2}–{:.2} AU",
                        hz.conservative_inner_au, hz.conservative_outer_au
                    ),
                ),
                (
                    "Optimistic",
                    format!(
                        "{:.2}–{:.2} AU",
                        hz.optimistic_inner_au, hz.optimistic_outer_au
                    ),
                ),
            ],
        });
    }
    Card {
        title: star.name.clone(),
        subtitle: format!("{} main-sequence star", star.spectral_class),
        sections,
        notes,
    }
}

/// A distance in AU with digits suited to its size.
pub fn au(distance_au: f64) -> String {
    if distance_au < 1.0 {
        format!("{distance_au:.3}")
    } else if distance_au < 100.0 {
        format!("{distance_au:.2}")
    } else {
        format!("{distance_au:.0}")
    }
}

/// What a planet orbits, read off its designation: `Vorus AB d` orbits
/// `Vorus AB`.
fn host_label(body: &Body) -> &str {
    body.name
        .rsplit_once(' ')
        .map_or("its star", |(host, _)| host)
}

/// `number` is the planet's 1-based position counting outward from its host.
pub fn body_card(body: &Body, number: usize) -> Card {
    let mut sections = vec![
        Section {
            heading: "Orbit",
            rows: vec![
                ("Semi-major axis", format!("{:.3} AU", body.orbit.a_au)),
                ("Eccentricity", format!("{:.3}", body.orbit.e)),
                ("Year", duration(body.orbit.period_days * 24.0)),
            ],
        },
        Section {
            heading: "Body",
            rows: {
                let mut rows = vec![
                    (
                        "Mass",
                        format!(
                            "{} Earth masses{}",
                            mass(body.mass_mearth),
                            in_jupiters(body.mass_mearth)
                        ),
                    ),
                    (
                        "Radius",
                        format!(
                            "{:.0} km ({:.2} × Earth)",
                            body.radius_km,
                            body.radius_km / 6378.0
                        ),
                    ),
                    ("Density", format!("{:.2} g/cm³", body.density_g_cm3)),
                ];
                if let Some(g) = body.surface_gravity_gees {
                    rows.push(("Surface gravity", format!("{g:.2} g")));
                }
                rows.push((
                    "Escape velocity",
                    format!("{:.1} km/s", body.escape_velocity_km_s),
                ));
                if let Some(hours) = body.rotation.period_hours {
                    rows.push(("Day", duration(hours)));
                }
                if let Some(tilt) = body.rotation.obliquity_deg {
                    rows.push(("Axial tilt", format!("{tilt:.0}°")));
                }
                if body.composition.water_frac > 0.005 {
                    rows.push(("Water", percent(body.composition.water_frac)));
                }
                if body.composition.h_he_frac > 0.0005 {
                    rows.push(("Hydrogen/helium", share(body.composition.h_he_frac)));
                }
                rows
            },
        },
    ];

    match &body.climate {
        Some(climate) => {
            let mut rows = vec![
                ("Mean temperature", celsius(climate.surface_temp_k)),
                (
                    "Day / night",
                    format!("{:.0} / {:.0} K", climate.day_temp_k, climate.night_temp_k),
                ),
                (
                    "Extremes",
                    format!("{:.0} to {:.0} K", climate.min_temp_k, climate.max_temp_k),
                ),
            ];
            if let Some(atmosphere) = &body.atmosphere {
                if let Some(pressure) = atmosphere.surface_pressure_mbar {
                    rows.push(("Pressure", format!("{:.3} atm", pressure / 1013.25)));
                    let total: f64 = atmosphere
                        .gases
                        .iter()
                        .map(|g| g.partial_pressure_mbar)
                        .sum();
                    if total > 0.0 {
                        let mix = atmosphere
                            .gases
                            .iter()
                            .take(4)
                            .map(|g| {
                                format!(
                                    "{} {:.1}%",
                                    g.symbol,
                                    100.0 * g.partial_pressure_mbar / total
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        rows.push(("Atmosphere", mix));
                    }
                }
            } else {
                rows.push(("Atmosphere", "none".into()));
            }
            rows.push(("Oceans", percent(climate.ocean_frac)));
            rows.push(("Ice", percent(climate.ice_frac)));
            rows.push(("Cloud", percent(climate.cloud_frac)));
            sections.push(Section {
                heading: "Surface",
                rows,
            });
        }
        None => sections.push(Section {
            heading: if body.atmosphere.is_some() {
                "Atmosphere"
            } else {
                "Temperature"
            },
            rows: vec![
                ("Equilibrium temperature", celsius(body.equilibrium_temp_k)),
                ("Albedo", format!("{:.2}", body.albedo)),
            ],
        }),
    }

    let mut notes = Vec::new();
    let kind = if is_belt(body) {
        notes.push(
            "Too little mass for a planet: drawn as a belt. The model gives the belt's \
             orbit and total mass; the rocks shown are illustrative."
                .into(),
        );
        class_name(body.class)
    } else if body.class == PlanetClass::Asteroids {
        // The Classic model applies this label at random to airless bodies
        // of any mass (docs/classic-constants.md §12).
        notes.push(
            "The Classic model labels this body \"Asteroids\", but at this mass it is a \
             single airless world."
                .into(),
        );
        class_name(PlanetClass::Rock)
    } else {
        class_name(body.class)
    };
    let f = &body.formation;
    if is_standard_class(body.class) {
        let mut story = format!("Began as an embryo {} AU from its star", au(f.origin_a_au));
        if body.orbit.a_au < 0.8 * f.origin_a_au {
            story.push_str(&format!(
                " and migrated inward to {} AU while the gas disk lasted",
                au(body.orbit.a_au)
            ));
        }
        story.push('.');
        notes.push(story);
        match f.mergers {
            0 => {}
            1 => notes.push("Absorbed one other embryo.".into()),
            n => notes.push(format!("Absorbed {n} other embryos.")),
        }
        if f.captured_gas {
            notes.push("Its core grew heavy enough to pull in gas in a runaway.".into());
        }
        if body.climate.is_none() && body.atmosphere.is_none() {
            notes.push(
                "This tier does not yet model its surface or any thin atmosphere: it is drawn \
                 as bare ground, and the terrain is illustrative."
                    .into(),
            );
        }
        if sg_render::surface::appearance(body).assumed_day {
            notes.push(format!(
                "Cloud colours follow its temperature. This tier gives no day length: the \
                 bands are drawn for an assumed {:.0}-hour day.",
                sg_render::surface::ASSUMED_GIANT_DAY_HOURS
            ));
        }
    } else {
        let mut story = format!("Grew from a nucleus injected at {:.2} AU", f.origin_a_au);
        match f.mergers {
            0 => story.push_str(" and never collided with another."),
            1 => story.push_str(", then absorbed one other embryo."),
            n => story.push_str(&format!(", then absorbed {n} other embryos.")),
        }
        notes.push(story);
        if f.captured_gas {
            notes.push("Passed its critical mass and captured gas from the disk.".into());
        }
    }
    if body.rotation.tidally_locked {
        notes.push("Tidally locked: one face always toward the star.".into());
    } else if body.rotation.spin_orbit_resonance {
        notes.push("Caught in a spin–orbit resonance.".into());
    }
    if body.climate.as_ref().is_some_and(|c| c.runaway_greenhouse) {
        notes.push("Runaway greenhouse.".into());
    }
    if body.habitability.in_conservative_hz {
        notes.push("In the conservative habitable zone.".into());
    } else if body.habitability.in_optimistic_hz {
        notes.push("In the optimistic habitable zone.".into());
    }
    if body.habitability.surface_liquid_water {
        notes.push("The model finds liquid water on the surface.".into());
    }
    if let Some(rings) = &body.rings {
        notes.push(format!(
            "Rings of ice from {:.1} to {:.1} planetary radii. Where they end follows from the \
             planet's density (inside that distance its tides pull a moon apart); that this \
             planet has bright rings at all was drawn, at the Solar System's rate of one giant \
             in four. The ringlets shown are illustrative.",
            rings.inner_radius_km / body.radius_km,
            rings.outer_radius_km / body.radius_km
        ));
    }

    Card {
        title: body.name.clone(),
        subtitle: format!(
            "{kind} · {} planet of {}",
            ordinal(number),
            host_label(body)
        ),
        sections,
        notes,
    }
}

/// Adds a line about a planet's moons to its card.
pub fn with_moons(mut card: Card, moons: usize) -> Card {
    match moons {
        0 => {}
        1 => card
            .notes
            .push("One moon: zoom out a little to see it.".into()),
        n => card
            .notes
            .push(format!("{n} moons: zoom out a little to see them.")),
    }
    card
}

/// The card of a moon: `number` counts outward among its planet's moons.
pub fn moon_card(moon: &Body, planet: &Body, number: usize, origin: Option<MoonOrigin>) -> Card {
    const KM_PER_AU: f64 = 1.495_978_707e8;
    const LUNAR_MASS_MEARTH: f64 = 0.0123;
    let a_km = moon.orbit.a_au * KM_PER_AU;
    let mut body_rows = vec![
        (
            "Mass",
            format!(
                "{} Earth masses ({:.2} × the Moon)",
                mass(moon.mass_mearth),
                moon.mass_mearth / LUNAR_MASS_MEARTH
            ),
        ),
        (
            "Radius",
            format!(
                "{:.0} km ({:.2} × the Moon)",
                moon.radius_km,
                moon.radius_km / 1737.4
            ),
        ),
        ("Density", format!("{:.2} g/cm³", moon.density_g_cm3)),
    ];
    if let Some(g) = moon.surface_gravity_gees {
        body_rows.push(("Surface gravity", format!("{g:.3} g")));
    }
    body_rows.push(("Equilibrium temperature", celsius(moon.equilibrium_temp_k)));
    let mut notes = vec![match origin {
        Some(MoonOrigin::GiantImpact) => format!(
            "Formed from the debris of a collision between {} and another body, as the Moon \
             is thought to have. About one rocky planet in twelve gets such a moon.",
            planet.name
        ),
        Some(MoonOrigin::CircumplanetaryDisk) | None => format!(
            "Grew in the disk of gas and dust around {} as the planet formed, as the large \
             moons of Jupiter and Saturn did.",
            planet.name
        ),
        Some(MoonOrigin::Capture) => format!("Captured by {}.", planet.name),
    }];
    notes.push(
        if moon.composition.water_frac >= 0.1 {
            "Half ice, half rock: it is cold enough here for ice to last."
        } else {
            "Rock: too warm here for ice to last."
        }
        .into(),
    );
    notes.push(
        "Its mass and distance were drawn within what such moons are known to have; its \
         formation was not simulated. Shown circling 20 times slower than the orbital clock. \
         The terrain is illustrative."
            .into(),
    );
    Card {
        title: moon.name.clone(),
        subtitle: format!("{} moon of {}", ordinal(number), planet.name),
        sections: vec![
            Section {
                heading: "Orbit",
                rows: vec![
                    (
                        "Distance",
                        format!(
                            "{:.0} km ({:.1} planetary radii)",
                            a_km,
                            a_km / planet.radius_km
                        ),
                    ),
                    ("Eccentricity", format!("{:.3}", moon.orbit.e)),
                    ("Month", duration(moon.orbit.period_days * 24.0)),
                ],
            },
            Section {
                heading: "Body",
                rows: body_rows,
            },
        ],
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sg_schema::{HierarchyNode, SimulationParams};

    fn system() -> sg_sim::SimulationResult {
        sg_sim::simulate(&SimulationParams {
            seed: 42,
            ..SimulationParams::default()
        })
        .unwrap()
    }

    #[test]
    fn durations_pick_a_readable_unit() {
        assert_eq!(duration(15.8), "15.8 h");
        assert_eq!(duration(24.0 * 88.0), "88.0 days");
        assert_eq!(duration(24.0 * 365.25 * 11.86), "11.86 years");
    }

    #[test]
    fn star_card_reports_the_star_and_its_habitable_zone() {
        let result = system();
        let HierarchyNode::Star(star) = &result.snapshot.hierarchy else {
            panic!("single star");
        };
        let card = star_card(
            star,
            result.snapshot.habitable_zones.first(),
            result.snapshot.planet_zones.first(),
            16,
            None,
            None,
        );
        assert!(card.notes.is_empty());
        assert_eq!(card.title, "Vorus");
        assert_eq!(card.subtitle, "G2 main-sequence star");
        let rows: Vec<_> = card.sections.iter().flat_map(|s| s.rows.clone()).collect();
        assert!(rows.contains(&("Mass", "1.00 M☉".to_string())));
        assert!(rows.contains(&("Temperature", format!("{:.0} K", star.teff_k))));
        assert!(rows.iter().any(|(k, _)| *k == "Conservative"));
    }

    #[test]
    fn terrestrial_card_has_a_surface_and_a_story() {
        // The first seed with a temperate ocean world.
        let result = (1..400)
            .map(|seed| {
                sg_sim::simulate(&SimulationParams {
                    seed,
                    ..SimulationParams::default()
                })
                .unwrap()
            })
            .find(|r| {
                r.snapshot
                    .bodies
                    .iter()
                    .any(|b| b.class == PlanetClass::Terrestrial)
            })
            .expect("a terrestrial planet in 400 seeds");
        let (index, body) = result
            .snapshot
            .bodies
            .iter()
            .enumerate()
            .find(|(_, b)| b.class == PlanetClass::Terrestrial)
            .unwrap();
        let card = body_card(body, index + 1);
        assert!(
            card.subtitle.starts_with("Terrestrial"),
            "{}",
            card.subtitle
        );
        let headings: Vec<_> = card.sections.iter().map(|s| s.heading).collect();
        assert_eq!(headings, ["Orbit", "Body", "Surface"]);
        assert!(card.notes[0].starts_with("Grew from a nucleus injected at"));
    }

    /// α Centauri-like: each star's card names its companion and the limit
    /// it sets; its planets are counted from their own star.
    #[test]
    fn cards_of_a_binary_describe_the_companion() {
        use sg_schema::{Fidelity, HostRef, PresetId, StarId};
        let result = sg_sim::simulate(&sg_sim::preset_params(
            PresetId::STypeBinary,
            42,
            Fidelity::Classic,
        ))
        .unwrap();
        let snap = &result.snapshot;
        let stars = sg_render::layout::stars(&snap.hierarchy);
        let companion = Companion {
            name: stars[1].name.clone(),
            separation_au: 23.5,
            eccentricity: 0.52,
            period_days: 29_496.0,
            mass_msun: 0.91,
        };
        let host = HostRef::Star(StarId(0));
        let zone = snap.planet_zones.iter().find(|z| z.host == host);
        let card = star_card(stars[0], None, zone, 8, Some(&companion), None);
        assert_eq!(card.title, "Vorus A");
        let rows: Vec<_> = card.sections.iter().flat_map(|s| s.rows.clone()).collect();
        assert!(rows.contains(&("Orbits with", "Vorus B".to_string())));
        assert!(rows.contains(&("Separation", "23.50 AU".to_string())));
        assert!(rows.contains(&("Period", "80.76 years".to_string())));
        assert!(
            card.notes[0].contains("only within 2.76 AU"),
            "{}",
            card.notes[0]
        );
        // Without a zone, the star keeps no planets.
        let card = star_card(stars[0], None, None, 0, Some(&companion), None);
        assert!(card.notes[0].contains("too close"));

        let third_of_b = snap
            .bodies
            .iter()
            .filter(|b| b.host == HostRef::Star(StarId(1)))
            .nth(2)
            .unwrap();
        let card = body_card(third_of_b, 3);
        assert_eq!(card.title, "Vorus B d");
        assert!(
            card.subtitle.ends_with("· 3rd planet of Vorus B"),
            "{}",
            card.subtitle
        );
        assert_eq!(
            (au(0.2243), au(23.5), au(8700.0)),
            ("0.224".into(), "23.50".into(), "8700".into())
        );
    }

    /// Standard-tier planets: no invented day or surface, and a formation
    /// story in that tier's terms.
    #[test]
    fn standard_cards_show_only_what_the_tier_models() {
        use sg_schema::{Fidelity, PresetId};
        let result = sg_sim::simulate(&sg_sim::preset_params(
            PresetId::HotJupiter,
            42,
            Fidelity::Standard,
        ))
        .unwrap();
        assert!(!result.snapshot.bodies.is_empty());
        for (index, body) in result.snapshot.bodies.iter().enumerate() {
            let card = body_card(body, index + 1);
            let rows: Vec<_> = card.sections.iter().flat_map(|s| s.rows.clone()).collect();
            assert!(rows
                .iter()
                .all(|(k, _)| *k != "Axial tilt" && *k != "Oceans"));
            assert_eq!(
                rows.iter().any(|(k, _)| *k == "Day"),
                body.rotation.tidally_locked
            );
            assert!(rows.iter().any(|(k, _)| *k == "Equilibrium temperature"));
            assert!(
                card.notes[0].starts_with("Began as an embryo"),
                "{}",
                card.notes[0]
            );
            assert!(!card.subtitle.starts_with("Unclassified"));
        }
        // The star's card describes the disk its planets formed in.
        let star = sg_render::layout::stars(&result.snapshot.hierarchy)[0];
        let disk = result.snapshot.disks_at_dispersal.first();
        let card = star_card(star, None, None, 3, None, disk);
        let rows: Vec<_> = card.sections.iter().flat_map(|s| s.rows.clone()).collect();
        assert!(rows.contains(&("Gas mass", "0.120 M☉".to_string())));
        assert!(rows.contains(&("Lasted", "5.0 Myr".to_string())));
        assert_eq!(share(0.004), "0.40%");
        assert_eq!(share(0.25), "25%");
    }

    #[test]
    fn small_masses_do_not_round_to_zero() {
        assert_eq!(mass(1.0), "1.000");
        assert_eq!(mass(0.0123), "0.012");
        assert_eq!(mass(2.95e-4), "3.0e-4");
        assert_eq!(mass(582.6), "583");
        assert_eq!(in_jupiters(582.6), " (1.83 × Jupiter)");
        assert_eq!(in_jupiters(5.0), "");
    }

    #[test]
    fn ordinals_read_naturally() {
        let got: Vec<String> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 101, 111]
            .into_iter()
            .map(ordinal)
            .collect();
        assert_eq!(
            got,
            [
                "1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "101st",
                "111th"
            ]
        );
    }

    /// The Classic model's "Asteroids" label is a random draw: only a body
    /// light enough is presented as a belt (seed 736, third body).
    #[test]
    fn asteroid_label_is_only_shown_for_belt_sized_bodies() {
        // A heavy body under this label is a world, however it came by it.
        let result = system();
        let mut heavy = result.snapshot.bodies[0].clone();
        heavy.class = PlanetClass::Asteroids;
        heavy.mass_mearth = 0.1;
        let card = body_card(&heavy, 1);
        assert!(
            card.subtitle.starts_with("Airless rock"),
            "{}",
            card.subtitle
        );
        assert!(card
            .notes
            .iter()
            .any(|n| n.contains("single airless world")));

        let mut light = heavy;
        light.mass_mearth = 3e-4;
        let card = body_card(&light, 3);
        assert!(
            card.subtitle.starts_with("Asteroid field"),
            "{}",
            card.subtitle
        );
        assert!(card.notes.iter().any(|n| n.contains("drawn as a belt")));
    }

    #[test]
    fn giant_card_has_no_surface() {
        let result = system();
        let body = result
            .snapshot
            .bodies
            .iter()
            .find(|b| b.class == PlanetClass::Jovian)
            .expect("seed 42 has a Jovian planet");
        let card = body_card(body, 9);
        assert!(card.sections.iter().all(|s| s.heading != "Surface"));
        assert!(card.notes.iter().any(|n| n.contains("captured gas")));
        let rows: Vec<_> = card.sections.iter().flat_map(|s| s.rows.clone()).collect();
        assert!(rows.iter().all(|(k, _)| *k != "Surface gravity"));
    }

    #[test]
    fn moons_and_rings_have_their_say_on_the_cards() {
        // The first seed with a ringed planet that also has a moon.
        let result = (1..400)
            .map(|seed| {
                sg_sim::simulate(&sg_schema::SimulationParams {
                    seed,
                    ..sg_schema::SimulationParams::default()
                })
                .expect("classic run")
            })
            .find(|r| {
                let s = &r.snapshot;
                s.planets()
                    .any(|b| b.rings.is_some() && s.moons_of(b.id).next().is_some())
            })
            .expect("rings and a moon in 400 seeds");
        let snapshot = &result.snapshot;
        let ringed = snapshot
            .planets()
            .find(|b| b.rings.is_some() && snapshot.moons_of(b.id).next().is_some())
            .unwrap();
        let card = body_card(ringed, 7);
        assert!(card
            .notes
            .iter()
            .any(|n| n.starts_with("Rings of ice from 1.2 to")));
        let moons: Vec<&Body> = snapshot.moons_of(ringed.id).collect();
        assert!(!moons.is_empty());
        let counted = with_moons(card.clone(), moons.len());
        assert_eq!(counted.notes.len(), card.notes.len() + 1);
        assert_eq!(with_moons(card.clone(), 0).notes, card.notes);

        let moon = moon_card(moons[0], ringed, 1, Some(MoonOrigin::CircumplanetaryDisk));
        assert_eq!(moon.subtitle, format!("1st moon of {}", ringed.name));
        assert!(moon.sections[0].rows[0].1.contains("planetary radii"));
        assert!(moon.notes[0].contains("Grew in the disk"));
        assert!(moon.notes.iter().any(|n| n.contains("not simulated")));
        let impact = moon_card(moons[0], ringed, 1, Some(MoonOrigin::GiantImpact));
        assert!(impact.notes[0].contains("collision"));
    }
}
