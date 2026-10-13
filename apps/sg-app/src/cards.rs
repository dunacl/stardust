//! Data cards: the facts about a selected star or planet (PLAN.md §8.5).
//!
//! Pure text formatting over the snapshot, so it is unit-tested without a
//! window.

use sg_render::belt::is_belt;
use sg_schema::{Body, HabitableZone, PlanetClass, Star};

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
    } else {
        format!("{mass_mearth:.3}")
    }
}

fn celsius(kelvin: f64) -> String {
    format!("{kelvin:.0} K ({:+.0} °C)", kelvin - 273.15)
}

fn percent(fraction: f64) -> String {
    format!("{:.0}%", fraction * 100.0)
}

pub fn star_card(star: &Star, hz: Option<&HabitableZone>, planets: usize) -> Card {
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
        notes: Vec::new(),
    }
}

/// `number` is the planet's 1-based position counting outward.
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
                    ("Mass", format!("{} Earth masses", mass(body.mass_mearth))),
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
                rows.push(("Day", duration(body.rotation.period_hours)));
                rows.push(("Axial tilt", format!("{:.0}°", body.rotation.obliquity_deg)));
                if body.composition.h_he_frac > 0.0005 {
                    rows.push(("Hydrogen/helium", percent(body.composition.h_he_frac)));
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
            heading: "Atmosphere",
            rows: vec![
                ("Cloud-top temperature", celsius(body.equilibrium_temp_k)),
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

    Card {
        title: body.name.clone(),
        subtitle: format!("{kind} · {} from the star", ordinal(number)),
        sections,
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
        let card = star_card(star, result.snapshot.habitable_zones.first(), 16);
        assert_eq!(card.title, "Vorus");
        assert_eq!(card.subtitle, "G2 main-sequence star");
        let rows: Vec<_> = card.sections.iter().flat_map(|s| s.rows.clone()).collect();
        assert!(rows.contains(&("Mass", "1.00 M☉".to_string())));
        assert!(rows.contains(&("Temperature", "5772 K".to_string())));
        assert!(rows.contains(&("Conservative", "0.95–1.68 AU".to_string())));
    }

    #[test]
    fn terrestrial_card_has_a_surface_and_a_story() {
        let result = system();
        let (index, body) = result
            .snapshot
            .bodies
            .iter()
            .enumerate()
            .find(|(_, b)| b.class == PlanetClass::Terrestrial)
            .expect("seed 42 has a terrestrial planet");
        let card = body_card(body, index + 1);
        assert_eq!(card.title, "Vorus h");
        assert_eq!(card.subtitle, "Terrestrial · 7th from the star");
        let headings: Vec<_> = card.sections.iter().map(|s| s.heading).collect();
        assert_eq!(headings, ["Orbit", "Body", "Surface"]);
        let surface = &card.sections[2].rows;
        assert!(surface
            .iter()
            .any(|(k, v)| *k == "Atmosphere" && v.starts_with("N ")));
        assert!(card.notes[0].starts_with("Grew from a nucleus injected at"));
        assert!(card.notes.iter().any(|n| n.contains("habitable zone")));
    }

    #[test]
    fn small_masses_do_not_round_to_zero() {
        assert_eq!(mass(1.0), "1.000");
        assert_eq!(mass(0.0123), "0.012");
        assert_eq!(mass(2.95e-4), "3.0e-4");
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
        let seed = |seed| {
            sg_sim::simulate(&SimulationParams {
                seed,
                ..SimulationParams::default()
            })
            .unwrap()
        };
        // Seed 2's innermost body is labelled Asteroids at 0.1 Earth masses.
        let heavy = seed(2);
        let (index, body) = (0, &heavy.snapshot.bodies[0]);
        assert_eq!(body.class, PlanetClass::Asteroids);
        let card = body_card(body, index + 1);
        assert!(
            card.subtitle.starts_with("Airless rock"),
            "{}",
            card.subtitle
        );
        assert!(card
            .notes
            .iter()
            .any(|n| n.contains("single airless world")));

        let light = seed(736);
        let card = body_card(&light.snapshot.bodies[2], 3);
        assert_eq!(card.subtitle, "Asteroid field · 3rd from the star");
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
        assert!(card.sections.iter().any(|s| s.heading == "Atmosphere"));
        assert!(card.notes.iter().any(|n| n.contains("captured gas")));
        let rows: Vec<_> = card.sections.iter().flat_map(|s| s.rows.clone()).collect();
        assert!(rows.iter().all(|(k, _)| *k != "Surface gravity"));
    }
}
