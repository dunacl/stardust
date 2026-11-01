//! Data cards: the facts about a selected star or planet (PLAN.md §8.5).
//!
//! Pure text formatting over the snapshot, so it is unit-tested without a
//! window.

use crate::i18n::{self, tr, trf, Language};
use sg_render::belt::is_belt;
use sg_schema::{
    Body, Comet, CometFamily, CometReservoir, DiskSummary, HabitableZone, Mineral, MoonOrigin,
    PlanetClass, PlanetZone, Star,
};

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
        trf("{} days", &[&format!("{:.1}", hours / 24.0)])
    } else {
        let years = hours / (24.0 * 365.25);
        match years {
            y if y >= 1.0e6 => trf("{} million years", &[&format!("{:.1}", y / 1.0e6)]),
            y if y >= 1.0e3 => trf("{} years", &[&format!("{y:.0}")]),
            y => trf("{} years", &[&format!("{y:.2}")]),
        }
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

/// A place in a row, as it is written in `language`. Spanish and French
/// agree with the noun, and `feminine` is for moons (`luna`, `lune`).
fn place(language: Language, n: usize, feminine: bool) -> String {
    match language {
        Language::En => ordinal(n),
        Language::Es => format!("{n}.{}", if feminine { "ª" } else { "º" }),
        Language::Fr => match (n, feminine) {
            (1, true) => "1re".to_string(),
            (1, false) => "1er".to_string(),
            _ => format!("{n}e"),
        },
        Language::De => format!("{n}."),
    }
}

/// A distance as text, in the current language's unit.
fn in_au(distance: &str) -> String {
    trf("{} AU", &[&distance])
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
        trf(" ({} × Jupiter)", &[&format!("{:.2}", mass_mearth / 317.8)])
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
    sections.push(Section {
        heading: "Composition",
        rows: vec![
            ("Hydrogen", share(star.hydrogen_frac)),
            ("Helium", share(star.helium_frac)),
            ("Metals", share(star.metal_frac)),
        ],
    });
    let mut notes = Vec::new();
    if let Some(companion) = companion {
        sections.push(Section {
            heading: "Companion",
            rows: vec![
                ("Orbits with", companion.name.clone()),
                ("Companion mass", format!("{:.2} M☉", companion.mass_msun)),
                ("Separation", in_au(&au(companion.separation_au))),
                ("Eccentricity", format!("{:.2}", companion.eccentricity)),
                ("Period", duration(companion.period_days * 24.0)),
            ],
        });
        match zone {
            Some(zone) if zone.limited_outside => notes.push(trf(
                "Its companion leaves stable orbits for planets only within {} AU.",
                &[&au(zone.outer_au)],
            )),
            Some(_) => {}
            None => notes.push(
                tr("Its companion is too close for this star to keep planets of its own.").into(),
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
                    trf(
                        "{} Earth masses",
                        &[&format!(
                            "{:.0}",
                            disk.initial_mass_msun * disk.dust_to_gas * 332_946.0
                        )],
                    ),
                ),
                ("Lasted", format!("{:.1} Myr", disk.lifetime_myr)),
                ("Ice line at the end", in_au(&au(disk.final_ice_line_au))),
            ],
        });
    }
    if let Some(hz) = hz {
        sections.push(Section {
            heading: "Habitable zone",
            rows: vec![
                (
                    "Conservative",
                    in_au(&format!(
                        "{:.2}–{:.2}",
                        hz.conservative_inner_au, hz.conservative_outer_au
                    )),
                ),
                (
                    "Optimistic",
                    in_au(&format!(
                        "{:.2}–{:.2}",
                        hz.optimistic_inner_au, hz.optimistic_outer_au
                    )),
                ),
            ],
        });
    }
    Card {
        title: star.name.clone(),
        subtitle: trf("{} main-sequence star", &[&star.spectral_class]),
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
    body.designation
        .rsplit_once(' ')
        .map_or(tr("its star"), |(host, _)| host)
}

/// `number` is the planet's 1-based position counting outward from its host.
pub fn body_card(body: &Body, number: usize) -> Card {
    let mut sections = vec![
        Section {
            heading: "Orbit",
            rows: vec![
                ("Semi-major axis", in_au(&format!("{:.3}", body.orbit.a_au))),
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
                        trf("{} Earth masses", &[&mass(body.mass_mearth)])
                            + &in_jupiters(body.mass_mearth),
                    ),
                    (
                        "Radius",
                        trf(
                            "{} km ({} × Earth)",
                            &[
                                &format!("{:.0}", body.radius_km),
                                &format!("{:.2}", body.radius_km / 6378.0),
                            ],
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
                rows
            },
        },
        composition_section(body),
    ];
    if let Some(section) = minerals_section(body) {
        sections.push(section);
    }

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
                    trf(
                        "{} to {} K",
                        &[
                            &format!("{:.0}", climate.min_temp_k),
                            &format!("{:.0}", climate.max_temp_k),
                        ],
                    ),
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
                rows.push(("Atmosphere", tr("none").into()));
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
            tr(
                "Too little mass for a planet: drawn as a belt. The model gives the belt's \
                orbit and total mass; the rocks shown are illustrative.",
            )
            .into(),
        );
        tr(class_name(body.class))
    } else if body.class == PlanetClass::Asteroids {
        // The Classic tier gives this label by mass, so this should not occur;
        // a hand-edited snapshot could still carry it.
        notes.push(
            tr("Labelled \"Asteroids\", but at this mass it is a single airless world.").into(),
        );
        tr(class_name(PlanetClass::Rock))
    } else {
        tr(class_name(body.class))
    };
    let f = &body.formation;
    if is_standard_class(body.class) {
        notes.push(if body.orbit.a_au < 0.8 * f.origin_a_au {
            trf(
                "Began as an embryo {} AU from its star and migrated inward to {} AU while the \
                 gas disk lasted.",
                &[&au(f.origin_a_au), &au(body.orbit.a_au)],
            )
        } else {
            trf(
                "Began as an embryo {} AU from its star.",
                &[&au(f.origin_a_au)],
            )
        });
        match f.mergers {
            0 => {}
            1 => notes.push(tr("Absorbed one other embryo.").into()),
            n => notes.push(trf("Absorbed {} other embryos.", &[&n])),
        }
        if f.captured_gas {
            notes.push(tr("Its core grew heavy enough to pull in gas in a runaway.").into());
        }
        if body.climate.is_none() && body.atmosphere.is_none() {
            notes.push(
                tr(
                    "This tier does not yet model its surface or any thin atmosphere: it is drawn \
                    as bare ground, and the terrain is illustrative.",
                )
                .into(),
            );
        }
        if sg_render::surface::appearance(body).assumed_day {
            notes.push(trf(
                "Cloud colours follow its temperature. This tier gives no day length: the \
                 bands are drawn for an assumed {}-hour day.",
                &[&format!(
                    "{:.0}",
                    sg_render::surface::ASSUMED_GIANT_DAY_HOURS
                )],
            ));
        }
    } else {
        let origin = format!("{:.2}", f.origin_a_au);
        notes.push(match f.mergers {
            0 => trf(
                "Grew from a nucleus injected at {} AU and never collided with another.",
                &[&origin],
            ),
            1 => trf(
                "Grew from a nucleus injected at {} AU, then absorbed one other embryo.",
                &[&origin],
            ),
            n => trf(
                "Grew from a nucleus injected at {} AU, then absorbed {} other embryos.",
                &[&origin, &n],
            ),
        });
        if f.captured_gas {
            notes.push(tr("Passed its critical mass and captured gas from the disk.").into());
        }
    }
    if body.rotation.tidally_locked {
        notes.push(tr("Tidally locked: one face always toward the star.").into());
    } else if body.rotation.spin_orbit_resonance {
        notes.push(tr("Caught in a spin–orbit resonance.").into());
    }
    if body.climate.as_ref().is_some_and(|c| c.runaway_greenhouse) {
        notes.push(tr("Runaway greenhouse.").into());
    }
    if body.habitability.in_conservative_hz {
        notes.push(tr("In the conservative habitable zone.").into());
    } else if body.habitability.in_optimistic_hz {
        notes.push(tr("In the optimistic habitable zone.").into());
    }
    if body.habitability.surface_liquid_water {
        notes.push(tr("The model finds liquid water on the surface.").into());
    }
    if let Some(rings) = &body.rings {
        notes.push(trf(
            "Rings of ice from {} to {} planetary radii. Where they end follows from the \
             planet's density (inside that distance its tides pull a moon apart); that this \
             planet has bright rings at all was drawn, at the Solar System's rate of one giant \
             in four. The ringlets shown are illustrative.",
            &[
                &format!("{:.1}", rings.inner_radius_km / body.radius_km),
                &format!("{:.1}", rings.outer_radius_km / body.radius_km),
            ],
        ));
    }

    Card {
        title: body.name.clone(),
        subtitle: trf(
            "{} · {} planet of {} · {}",
            &[
                &kind,
                &place(i18n::language(), number, false),
                &host_label(body),
                &body.designation,
            ],
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
            .push(tr("One moon: zoom out a little to see it.").into()),
        n => card
            .notes
            .push(trf("{} moons: zoom out a little to see them.", &[&n])),
    }
    card
}

/// The card of a moon: `number` counts outward among its planet's moons.
/// What a body is made of, by mass, as the model gives it.
fn composition_section(body: &Body) -> Section {
    let c = &body.composition;
    let rows = [
        ("Iron and metals", c.metal_frac),
        ("Rock", c.rock_frac),
        ("Water", c.water_frac),
        ("Volatiles", c.volatiles_frac),
        ("Hydrogen/helium", c.h_he_frac),
    ]
    .into_iter()
    .filter(|(_, fraction)| *fraction > 0.0005)
    .map(|(label, fraction)| (label, share(fraction)))
    .collect();
    Section {
        heading: "Composition",
        rows,
    }
}

fn mineral_name(mineral: Mineral) -> &'static str {
    match mineral {
        Mineral::Silicates => "Silicates",
        Mineral::IronOxides => "Iron oxides",
        Mineral::Sulfides => "Sulfides",
        Mineral::Carbonates => "Carbonates",
        Mineral::WaterIce => "Water ice",
        Mineral::VolatileIces => "Volatile ices",
        Mineral::MoltenRock => "Molten rock",
    }
}

/// The estimated surface, if the body has one.
fn minerals_section(body: &Body) -> Option<Section> {
    (!body.minerals.is_empty()).then(|| Section {
        heading: "Surface minerals (estimated)",
        rows: body
            .minerals
            .iter()
            .map(|part| (mineral_name(part.mineral), percent(part.share)))
            .collect(),
    })
}

/// A count of comets to two figures: more would claim too much.
fn round_count(count: f64) -> String {
    if count < 100.0 {
        return format!("{count:.0}");
    }
    let magnitude = 10f64.powi(count.log10().floor() as i32 - 1);
    format!("{:.0}", (count / magnitude).round() * magnitude)
}

/// Adds what a host has in comets to its star's card.
pub fn with_comets(mut card: Card, reservoir: &CometReservoir) -> Card {
    let mut rows = vec![
        (
            "Active comets",
            trf("about {}", &[&round_count(reservoir.active_comets)]),
        ),
        (
            "Icy belts",
            trf(
                "{} Earth masses",
                &[&format!("{:.2}", reservoir.belt_mass_mearth)],
            ),
        ),
        ("Ejectors", reservoir.ejectors.to_string()),
    ];
    if let Some(oort) = reservoir.oort_mass_mearth {
        rows.push((
            "Oort cloud",
            trf("{} Earth masses", &[&format!("{oort:.1}")]),
        ));
    }
    card.sections.push(Section {
        heading: "Comets",
        rows,
    });
    card.notes.push(
        tr(
            "Comet counts are orders of magnitude, scaled from the Solar System. Only belt mass \
            within reach of the outermost planet feeds comets.",
        )
        .into(),
    );
    card
}

/// A comet's card. `orbits` names what it orbits.
pub fn comet_card(comet: &Comet, orbits: &str) -> Card {
    let orbit = &comet.orbit;
    let family = match comet.family {
        CometFamily::JupiterFamily => tr("Jupiter-family comet"),
        CometFamily::LongPeriod => tr("Long-period comet"),
    };
    let mut notes = vec![match comet.family {
        CometFamily::JupiterFamily => {
            tr("A short-period comet, held on its orbit by the system's \
            heaviest planet.")
        }
        CometFamily::LongPeriod => tr(
            "It fell in from an Oort cloud. It will not be back for a very long time, so the \
            view replays this one passage.",
        ),
    }
    .to_string()];
    notes.push(
        tr(
            "Its orbit and size were drawn from what comets of this family are known to have; \
            the model follows no small body. The nucleus is drawn far larger than it is, and \
            the length of the tails is a drawing choice. Tails grow near the star and point \
            away from it.",
        )
        .into(),
    );
    Card {
        title: comet.name.clone(),
        subtitle: trf(
            "{} · orbits {} · {}",
            &[&family, &orbits, &comet.designation],
        ),
        sections: vec![
            Section {
                heading: "Orbit",
                rows: vec![
                    (
                        "Perihelion",
                        in_au(&format!("{:.2}", orbit.a_au * (1.0 - orbit.e))),
                    ),
                    (
                        "Aphelion",
                        in_au(&format!("{:.2}", orbit.a_au * (1.0 + orbit.e))),
                    ),
                    ("Eccentricity", format!("{:.3}", orbit.e)),
                    ("Inclination", format!("{:.0}°", orbit.i_deg)),
                    ("Year", duration(orbit.period_days * 24.0)),
                ],
            },
            Section {
                heading: "Comet",
                rows: vec![
                    ("Nucleus", format!("{:.1} km", comet.diameter_km)),
                    ("Ice and dust", tr("about equal").to_string()),
                ],
            },
        ],
        notes,
    }
}

pub fn moon_card(moon: &Body, planet: &Body, number: usize, origin: Option<MoonOrigin>) -> Card {
    const KM_PER_AU: f64 = 1.495_978_707e8;
    const LUNAR_MASS_MEARTH: f64 = 0.0123;
    let a_km = moon.orbit.a_au * KM_PER_AU;
    let mut body_rows = vec![
        (
            "Mass",
            trf(
                "{} Earth masses ({} × the Moon)",
                &[
                    &mass(moon.mass_mearth),
                    &format!("{:.2}", moon.mass_mearth / LUNAR_MASS_MEARTH),
                ],
            ),
        ),
        (
            "Radius",
            trf(
                "{} km ({} × the Moon)",
                &[
                    &format!("{:.0}", moon.radius_km),
                    &format!("{:.2}", moon.radius_km / 1737.4),
                ],
            ),
        ),
        ("Density", format!("{:.2} g/cm³", moon.density_g_cm3)),
    ];
    if let Some(g) = moon.surface_gravity_gees {
        body_rows.push(("Surface gravity", format!("{g:.3} g")));
    }
    body_rows.push(("Equilibrium temperature", celsius(moon.equilibrium_temp_k)));
    let mut notes = vec![match origin {
        Some(MoonOrigin::GiantImpact) => trf(
            "Formed from the debris of a collision between {} and another body, as the Moon \
             is thought to have. About one rocky planet in twelve gets such a moon.",
            &[&planet.name],
        ),
        Some(MoonOrigin::CircumplanetaryDisk) | None => trf(
            "Grew in the disk of gas and dust around {} as the planet formed, as the large \
             moons of Jupiter and Saturn did.",
            &[&planet.name],
        ),
        Some(MoonOrigin::Capture) => trf("Captured by {}.", &[&planet.name]),
    }];
    notes.push(
        if moon.composition.water_frac >= 0.1 {
            tr("Half ice, half rock: it is cold enough here for ice to last.")
        } else {
            tr("Rock: too warm here for ice to last.")
        }
        .into(),
    );
    notes.push(
        tr(
            "Its mass and distance were drawn within what such moons are known to have; its \
            formation was not simulated. Shown circling 20 times slower than the orbital clock. \
            Outside True scale it is drawn closer in, to stay clear of other planets. The \
            terrain is illustrative.",
        )
        .into(),
    );
    Card {
        title: moon.name.clone(),
        subtitle: trf(
            "{} moon of {} · {}",
            &[
                &place(i18n::language(), number, true),
                &planet.name,
                &moon.designation,
            ],
        ),
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
            composition_section(moon),
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
    fn every_row_of_every_card_has_help() {
        for fidelity in [sg_schema::Fidelity::Standard, sg_schema::Fidelity::Classic] {
            for seed in 1..=8 {
                let result = sg_sim::simulate(&SimulationParams {
                    seed,
                    fidelity,
                    ..SimulationParams::default()
                })
                .unwrap();
                let snapshot = &result.snapshot;
                let mut cards = Vec::new();
                for star in sg_render::layout::stars(&snapshot.hierarchy) {
                    let companion = Companion {
                        name: "B".into(),
                        separation_au: 1.0,
                        eccentricity: 0.0,
                        period_days: 1.0,
                        mass_msun: 1.0,
                    };
                    cards.push(star_card(
                        star,
                        snapshot.habitable_zones.first(),
                        None,
                        0,
                        Some(&companion),
                        snapshot.disks_at_dispersal.first(),
                    ));
                }
                for comet in &snapshot.comets {
                    cards.push(comet_card(comet, "x"));
                }
                for reservoir in &snapshot.comet_reservoirs {
                    let empty = Card {
                        title: String::new(),
                        subtitle: String::new(),
                        sections: Vec::new(),
                        notes: Vec::new(),
                    };
                    cards.push(with_comets(empty, reservoir));
                }
                for body in snapshot.planets() {
                    cards.push(body_card(body, 1));
                    for moon in snapshot.moons_of(body.id) {
                        cards.push(moon_card(moon, body, 1, None));
                    }
                }
                for card in cards {
                    for section in card.sections {
                        assert!(i18n::has(section.heading), "{}", section.heading);
                        for (label, _) in section.rows {
                            assert!(crate::help::card_row(label).is_some(), "no help: {label}");
                            assert!(i18n::has(label), "no translation: {label}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn every_class_has_a_translation() {
        use PlanetClass::*;
        for class in [
            Asteroids,
            Rock,
            Venusian,
            Terrestrial,
            Martian,
            Water,
            Ice,
            OneFace,
            GasDwarf,
            SubJovian,
            Jovian,
            Unknown,
            Rocky,
            SuperEarth,
            LavaWorld,
            WaterWorld,
            IceWorld,
            SubNeptune,
            IceGiant,
            GasGiant,
            HotJupiter,
        ] {
            assert!(i18n::has(class_name(class)), "{class:?}");
        }
    }

    #[test]
    fn every_mineral_has_a_row_and_help() {
        for mineral in [
            Mineral::Silicates,
            Mineral::IronOxides,
            Mineral::Sulfides,
            Mineral::Carbonates,
            Mineral::WaterIce,
            Mineral::VolatileIces,
            Mineral::MoltenRock,
        ] {
            let name = mineral_name(mineral);
            assert!(
                i18n::has(name) && crate::help::card_row(name).is_some(),
                "{name}"
            );
        }
    }

    #[test]
    fn counts_of_comets_keep_two_figures() {
        assert_eq!(round_count(7.4), "7");
        assert_eq!(round_count(300.0), "300");
        assert_eq!(round_count(124_131.4), "120000");
    }

    #[test]
    fn a_comet_card_says_what_it_is() {
        let (result, comet) = (1..50)
            .find_map(|seed| {
                let result = system_of(seed);
                let comet = result.snapshot.comets.first().cloned()?;
                Some((result, comet))
            })
            .expect("a system with comets");
        let card = comet_card(&comet, "Vela");
        assert_eq!(card.title, comet.name);
        assert!(card.subtitle.contains("Vela"));
        assert!(card.notes.iter().any(|note| note.contains("no small body")));
        assert!(!result.snapshot.comet_reservoirs.is_empty());
    }

    #[test]
    fn places_are_written_as_each_language_writes_them() {
        assert_eq!(place(Language::En, 3, false), "3rd");
        assert_eq!(place(Language::Es, 3, false), "3.º");
        assert_eq!(place(Language::Es, 3, true), "3.ª");
        assert_eq!(place(Language::Fr, 1, false), "1er");
        assert_eq!(place(Language::Fr, 1, true), "1re");
        assert_eq!(place(Language::Fr, 2, true), "2e");
        assert_eq!(place(Language::De, 4, false), "4.");
    }

    fn system_of(seed: u64) -> sg_sim::SimulationResult {
        sg_sim::simulate(&SimulationParams {
            seed,
            fidelity: sg_schema::Fidelity::Standard,
            ..SimulationParams::default()
        })
        .unwrap()
    }

    #[test]
    fn durations_pick_a_readable_unit() {
        assert_eq!(duration(15.8), "15.8 h");
        assert_eq!(duration(24.0 * 88.0), "88.0 days");
        assert_eq!(duration(24.0 * 365.25 * 11.86), "11.86 years");
        assert_eq!(duration(24.0 * 365.25 * 42_000.4), "42000 years");
        assert_eq!(duration(24.0 * 365.25 * 1_757_029.7), "1.8 million years");
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
        assert_eq!(
            headings,
            [
                "Orbit",
                "Body",
                "Composition",
                "Surface minerals (estimated)",
                "Surface"
            ]
        );
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
        assert!(card.subtitle.ends_with("Vorus B d"), "{}", card.subtitle);
        assert!(
            card.subtitle.contains("· 3rd planet of Vorus B ·"),
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
        assert!(rows.contains(&("Gas mass", "0.030 M☉".to_string())));
        assert!(rows.contains(&("Lasted", "8.0 Myr".to_string())));
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
        assert!(moon
            .subtitle
            .starts_with(&format!("1st moon of {}", ringed.name)));
        assert!(moon.sections[0].rows[0].1.contains("planetary radii"));
        assert!(moon.notes[0].contains("Grew in the disk"));
        assert!(moon.notes.iter().any(|n| n.contains("not simulated")));
        let impact = moon_card(moons[0], ringed, 1, Some(MoonOrigin::GiantImpact));
        assert!(impact.notes[0].contains("collision"));
    }
}
