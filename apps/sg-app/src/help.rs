//! What every item on the screen means, in one place.
//!
//! The Help window lists all of it, for touch screens as much as for a
//! mouse; the hover text on a control comes from the same [`Item`], so the
//! two cannot disagree. Words only: this module holds no physics, and says
//! plainly where the app draws something the model does not give.

use sg_render::ScaleMode;
use sg_schema::Fidelity;

/// One thing on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    /// As it is labelled on the screen.
    pub name: &'static str,
    pub text: &'static str,
}

impl Item {
    /// Whether a search matches the name or the text, ignoring case. An
    /// empty search matches everything.
    pub fn matches(&self, search: &str) -> bool {
        let search = search.trim().to_lowercase();
        self.name.to_lowercase().contains(&search) || self.text.to_lowercase().contains(&search)
    }
}

/// A part of the screen.
#[derive(Debug, Clone, Copy)]
pub struct Topic {
    pub title: &'static str,
    pub items: &'static [Item],
}

const fn item(name: &'static str, text: &'static str) -> Item {
    Item { name, text }
}

pub const STANDARD: Item = item(
    "Standard",
    "The modern model: a gas disk that evolves and clears, cores grown by pebble accretion, \
     gas giants by runaway accretion, migration, and loss of atmospheres to the star's radiation.",
);
pub const CLASSIC: Item = item(
    "Classic",
    "Dole's accretion model of 1969 with Fogg's extension of 1985, around modern stars and \
     orbital-stability limits. A historical method, not a description of how planets form.",
);
pub const HIGH: Item = item(
    "High",
    "The Standard model, with the orbits of the massive planets followed directly after the \
     gas clears: a run aims at a million years but usually stops near 30,000. Giants packed too \
     closely throw one another onto elongated orbits or out of the system, where the Standard \
     tier would merge them. Takes a few seconds when there are two or more massive planets; \
     otherwise it is the Standard tier.",
);
pub const SHOWN: Item = item(
    "Shown",
    "The tier of the system on the screen. It can differ from the tier chosen above until you \
     press Simulate.",
);
pub const MODEL_VERSION_NOTE: Item = item(
    "Model version",
    "The same settings, seed and model version always give the same system, on every platform.",
);

pub const PRESET: Item = item(
    "Preset",
    "A starting point: it sets the stars and the other parameters for a kind of system. Greyed \
     out where the chosen tier cannot run it; hover over it for the reason.",
);
pub const SEED: Item = item(
    "Seed",
    "The number all the random draws come from. The same seed and settings give the same system.",
);
pub const NEW_SEED: Item = item(
    "New seed",
    "Picks another seed. The system changes on the next Simulate.",
);
pub const STAR_MASS: Item = item(
    "Star mass",
    "In solar masses (M☉). Heavier stars are brighter and much shorter-lived. Binary and triple \
     presets have one slider per star.",
);
pub const SEPARATION: Item = item(
    "Separation",
    "The average distance between two stars of a pair, in AU (the Earth–Sun distance).",
);
pub const ECCENTRICITY: Item = item(
    "Eccentricity",
    "How elongated an orbit is: 0 is a circle, and values near 1 are long thin ellipses.",
);
pub const FIX_AGE: Item = item(
    "Fix age",
    "Sets the age of the system in billions of years (Gyr). Unticked, the age is sampled.",
);
pub const SIMULATE: Item = item(
    "Simulate",
    "Builds a new system from everything above. Sliders change nothing until you press it.",
);
pub const PLAY_FORMATION: Item = item(
    "Play formation",
    "Plays the formation timeline after each Simulate. Unticked, the finished system appears \
     at once.",
);
pub const SIMULATED_IN: Item = item("Simulated in", "How long the last run took on this device.");
pub const WARNINGS: Item = item(
    "Yellow notes",
    "Warnings from the model about this run, such as a star outside the range a method was \
     built for.",
);

pub const METALLICITY: Item = item(
    "Metallicity",
    "The share of elements heavier than helium, in dex relative to the Sun: 0 is solar, +0.3 \
     about twice as much. More metals mean more solids to build planets from. Unticked, it is \
     drawn at random.",
);
pub const DISK_MASS: Item = item(
    "Disk mass",
    "The gas disk's mass as a fraction of the star's. Heavier disks give more and bigger \
     planets. Unticked, it is drawn at random.",
);
pub const DISK_LIFETIME: Item = item(
    "Disk lifetime",
    "How long the gas lasts, in millions of years (Myr). Giants must capture their gas in this \
     time. Unticked, it is drawn at random.",
);
pub const MIGRATION: Item = item(
    "Migration",
    "Planets drift inward through the gas disk as they grow.",
);
pub const RESET_DISK: Item = item(
    "Reset",
    "Puts the disk and formation settings back to their defaults.",
);

pub const DUST_DENSITY: Item = item(
    "Dust density A",
    "More dust: more and heavier planets. Dole: 0.0015.",
);
pub const GAS_DUST: Item = item(
    "Gas/dust ratio K",
    "How much gas a giant can capture per unit of dust. Dole: 50.",
);
pub const CRITICAL_MASS: Item = item(
    "Critical mass B",
    "Mass above which a planet captures gas, at 1 AU. Dole: 1.2e-5.",
);
pub const CLOUD_ECCENTRICITY: Item = item(
    "Cloud eccentricity",
    "Widens each nucleus's reach. Dole: 0.2–0.25.",
);
pub const ECCENTRICITY_EXPONENT: Item = item(
    "Eccentricity exponent",
    "Larger: more eccentric nuclei. Dole: 0.077.",
);
pub const COMPUTE_ATMOSPHERES: Item = item(
    "Compute atmospheres",
    "Has no effect in this version: the setting is stored but the model does not read it.",
);
pub const RESET_DOLE: Item = item(
    "Reset to Dole's values",
    "Puts the accretion settings back to those of Dole's paper.",
);

pub const TRUE_SCALE: Item = item(
    "True scale",
    "Distances and sizes are both real. Planets are then far too small to see without zooming \
     in.",
);
pub const ENLARGED: Item = item(
    "Enlarged bodies",
    "Distances are real, bodies are drawn much larger than they are, and each is fitted so it \
     does not reach a neighbour's orbit.",
);
pub const SCHEMATIC: Item = item(
    "Schematic",
    "Distances are compressed so that near and far orbits fit together, and bodies are drawn \
     large. A diagram: do not read distances off it.",
);
pub const ORBITS: Item = item("Orbits", "Draws the path of each planet.");
pub const HABITABLE_ZONE: Item = item(
    "Habitable zone",
    "Marks where a planet with an atmosphere like Earth's could keep liquid water on its \
     surface.",
);
pub const LIMITS: Item = item(
    "Limits",
    "Red rings: where a companion star's pull ends the zone in which planets keep stable orbits.",
);
pub const NAMES: Item = item(
    "Names",
    "Writes names beside the stars and, once the system is finished, the planets. Where two \
     would overlap, the selected or heavier body keeps its name.",
);
pub const DISK: Item = item(
    "Disk, while forming",
    "The gas and dust the planets grow in. How dense it is, where giants have opened gaps and \
     when it clears come from the model; the streaks do not.",
);
pub const SKY: Item = item(
    "Sky backdrop",
    "Scenery: the stars and galaxies behind the system are not simulated.",
);
pub const FRAME_SYSTEM: Item = item(
    "Frame system",
    "Clears the selection and moves the camera to show the whole system.",
);
pub const ORBITAL_MOTION: Item = item(
    "Orbital motion",
    "Moves the finished planets along their orbits.",
);
pub const DAYS_PER_SECOND: Item = item(
    "days / second",
    "How fast the finished planets move: simulated days for each second on the screen.",
);
pub const TOUR: Item = item(
    "Spaceship tour",
    "Just for fun: a small ship visits each planet and moon. It is not simulated, and its speed \
     and size are chosen for the picture.",
);
pub const EFFECTS: Item = item(
    "Effects",
    "Reduced is for phones and older graphics: no glow around stars, coarser spheres, fewer \
     belt rocks, less surface detail.",
);
pub const FRAME_RATE: Item = item(
    "Frame rate",
    "Shows frames per second and the time per frame in a corner of the screen.",
);
pub const CONTROL_PANEL: Item = item(
    "Control panel",
    "Shows or hides the panel on the left. Its settings stay as they are.",
);

pub const PLAY: Item = item(
    "Play / Pause",
    "Plays the formation or stops it. At the end, Play starts again from the beginning.",
);
pub const RESTART: Item = item("Restart", "Plays the formation from the beginning.");
pub const END: Item = item("End", "Jumps to the finished system.");
pub const TIME: Item = item(
    "Time slider",
    "Moves through the formation, in millions of years (Myr) since the disk formed. Dragging it \
     pauses the playback. In the Classic tier these times are nominal, not physical.",
);
pub const SPEED: Item = item(
    "Speed",
    "How fast the formation plays, as a multiple of the usual speed.",
);
pub const STATUS: Item = item(
    "Status line",
    "How many embryos exist at this moment, how many events the formation had, and what the \
     clock measures. Embryo sizes are a drawing choice.",
);

const GETTING_AROUND: &[Item] = &[
    item("Rotate", "Drag on the empty sky."),
    item("Zoom", "Scroll, or pinch on a touch screen."),
    item(
        "Select a body",
        "Click or tap a star, planet or moon to follow it and open its data card.",
    ),
    item("Help", "This window. F1 opens and closes it."),
];
const TIERS: &[Item] = &[STANDARD, CLASSIC, HIGH, SHOWN, MODEL_VERSION_NOTE];
const SYSTEM: &[Item] = &[
    PRESET,
    SEED,
    NEW_SEED,
    STAR_MASS,
    SEPARATION,
    ECCENTRICITY,
    FIX_AGE,
    SIMULATE,
    PLAY_FORMATION,
    SIMULATED_IN,
    WARNINGS,
];
const DISK_AND_FORMATION: &[Item] = &[METALLICITY, DISK_MASS, DISK_LIFETIME, MIGRATION, RESET_DISK];
const ACCRETION: &[Item] = &[
    DUST_DENSITY,
    GAS_DUST,
    CRITICAL_MASS,
    CLOUD_ECCENTRICITY,
    ECCENTRICITY_EXPONENT,
    COMPUTE_ATMOSPHERES,
    RESET_DOLE,
];
const VIEW: &[Item] = &[
    TRUE_SCALE,
    ENLARGED,
    SCHEMATIC,
    ORBITS,
    HABITABLE_ZONE,
    LIMITS,
    NAMES,
    DISK,
    SKY,
    FRAME_SYSTEM,
];
const PLAYBACK: &[Item] = &[
    PLAY,
    RESTART,
    END,
    TIME,
    SPEED,
    STATUS,
    ORBITAL_MOTION,
    DAYS_PER_SECOND,
    TOUR,
];
const SETTINGS: &[Item] = &[EFFECTS, FRAME_RATE, CONTROL_PANEL];
const PRESETS: &[Item] = &[
    item("Sun-like", "One star of one solar mass."),
    item(
        "Red dwarf",
        "One star of 0.2 solar masses. Standard and High tiers only.",
    ),
    item(
        "Hot Jupiter",
        "A Sun-like, metal-rich star with a massive, long-lived disk, which favours giants that \
         move far in. Not every seed gives a hot Jupiter: about one in six gives a giant inside \
         10 days.",
    ),
    item("Massive A star", "One star of two solar masses."),
    item(
        "Metal-poor",
        "A star of 0.8 solar masses with a tenth of the Sun's metals: few solids to build \
         planets from.",
    ),
    item(
        "Circumbinary (Kepler-16-like)",
        "Two close stars, with planets orbiting both.",
    ),
    item(
        "S-type binary (α Cen-like)",
        "Two stars far apart, with planets around each one.",
    ),
    item(
        "Hierarchical triple",
        "A close pair of stars with a third one far away.",
    ),
    item(
        "Random",
        "Stars drawn from the mass function and multiplicity statistics.",
    ),
];
/// What a data card's rows mean; [`card_row`] finds them by label.
const CARD_ROWS: &[Item] = &[
    item(
        "Mass",
        "In Earth masses for bodies, solar masses (M☉) for stars.",
    ),
    item(
        "Luminosity",
        "The star's total light, in units of the Sun's (L☉).",
    ),
    item("Radius", "Size of the body or star."),
    item("Temperature", "The star's surface temperature."),
    item(
        "Age",
        "Time since the star formed, in billions of years (Gyr).",
    ),
    item(
        "Main-sequence life",
        "How long the star burns hydrogen in its core.",
    ),
    item("Planets", "How many planets orbit this star."),
    item("Orbits with", "The star or stars it is paired with."),
    item("Companion mass", "The combined mass of those stars."),
    SEPARATION,
    ECCENTRICITY,
    item("Period", "How long one orbit of the pair takes."),
    item("Gas mass", "The gas in the star's disk when it formed."),
    item(
        "Dust",
        "The solids in that disk, which planets are built from.",
    ),
    item("Lasted", "How long the gas stayed before it cleared."),
    item(
        "Ice line at the end",
        "The distance beyond which water froze when the disk cleared.",
    ),
    item(
        "Conservative",
        "The narrower habitable zone, from where water would escape to where carbon dioxide \
         stops warming a planet (Kopparapu et al. 2014).",
    ),
    item(
        "Optimistic",
        "The wider habitable zone, set by the light recent Venus and early Mars received \
         (Kopparapu et al. 2014).",
    ),
    item(
        "Semi-major axis",
        "The orbit's average distance from what it circles, in AU (the Earth–Sun distance).",
    ),
    item("Year", "How long one orbit takes."),
    item(
        "Density",
        "Mass over volume. Water is 1 g/cm³, Earth 5.5, Saturn 0.7.",
    ),
    item("Surface gravity", "In units of Earth's (g)."),
    item(
        "Escape velocity",
        "The speed needed to leave the body for good. Low values lose atmospheres more easily.",
    ),
    item("Day", "How long the body takes to turn once."),
    item(
        "Axial tilt",
        "The angle between its spin and its orbit. Earth's is 23°.",
    ),
    item("Water", "The share of the body's mass that is water."),
    item(
        "Hydrogen/helium",
        "The share of the body's mass that is hydrogen and helium gas.",
    ),
    item("Mean temperature", "The average at the surface."),
    item("Day / night", "Typical temperatures on the two sides."),
    item(
        "Extremes",
        "The lowest and highest temperatures reached on the surface.",
    ),
    item(
        "Pressure",
        "At the surface, in atmospheres (Earth is 1 atm).",
    ),
    item("Atmosphere", "The main gases, by share of the pressure."),
    item("Oceans", "The share of the surface under liquid water."),
    item("Ice", "The share of the surface under ice."),
    item("Cloud", "The share of the surface under cloud."),
    item(
        "Equilibrium temperature",
        "The temperature the star's light alone would give, with no greenhouse warming.",
    ),
    item(
        "Albedo",
        "The share of light reflected: 0 is black, 1 a perfect mirror.",
    ),
    item("Distance", "The moon's average distance from its planet."),
    item("Month", "How long one orbit of the planet takes."),
];

/// Everything the Help window lists, in the order of the screen.
pub const TOPICS: &[Topic] = &[
    Topic {
        title: "Getting around",
        items: GETTING_AROUND,
    },
    Topic {
        title: "Model tiers",
        items: TIERS,
    },
    Topic {
        title: "System",
        items: SYSTEM,
    },
    Topic {
        title: "Presets",
        items: PRESETS,
    },
    Topic {
        title: "Disk and formation (Standard, High)",
        items: DISK_AND_FORMATION,
    },
    Topic {
        title: "Accretion model (Classic)",
        items: ACCRETION,
    },
    Topic {
        title: "View",
        items: VIEW,
    },
    Topic {
        title: "Playback",
        items: PLAYBACK,
    },
    Topic {
        title: "Settings",
        items: SETTINGS,
    },
    Topic {
        title: "Data card",
        items: CARD_ROWS,
    },
];

/// What a row of a data card means, by the row's label.
pub fn card_row(label: &str) -> Option<&'static str> {
    CARD_ROWS
        .iter()
        .find(|row| row.name == label)
        .map(|row| row.text)
}

/// What a tier is.
pub fn tier(fidelity: Fidelity) -> &'static str {
    match fidelity {
        Fidelity::Standard => STANDARD.text,
        Fidelity::Classic => CLASSIC.text,
        Fidelity::High => HIGH.text,
    }
}

/// What a scale mode shows.
pub fn scale(mode: ScaleMode) -> &'static str {
    match mode {
        ScaleMode::True => TRUE_SCALE.text,
        ScaleMode::Exaggerated => ENLARGED.text,
        ScaleMode::Schematic => SCHEMATIC.text,
    }
}

/// What a preset sets up, by its name on the screen.
pub fn preset(name: &str) -> Option<&'static str> {
    PRESETS
        .iter()
        .find(|item| item.name == name)
        .map(|item| item.text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_item_says_something_and_is_not_listed_twice_in_a_topic() {
        for topic in TOPICS {
            let mut names = BTreeSet::new();
            for item in topic.items {
                assert!(!item.text.trim().is_empty(), "{}", item.name);
                assert!(names.insert(item.name), "{} twice", item.name);
            }
        }
    }

    #[test]
    fn search_ignores_case_and_looks_in_the_text() {
        assert!(SEED.matches(""));
        assert!(SEED.matches("sEeD"));
        assert!(SEED.matches("random draws"));
        assert!(!SEED.matches("albedo"));
    }

    #[test]
    fn card_rows_are_found_by_label() {
        assert_eq!(card_row("Eccentricity"), Some(ECCENTRICITY.text));
        assert_eq!(card_row("no such row"), None);
    }

    #[test]
    fn tiers_and_scales_all_have_text() {
        for fidelity in [Fidelity::Standard, Fidelity::Classic, Fidelity::High] {
            assert!(!tier(fidelity).is_empty());
        }
        for mode in ScaleMode::ALL {
            assert!(!scale(mode).is_empty());
        }
    }

    #[test]
    fn every_preset_has_an_entry() {
        assert_eq!(sg_sim::ALL_PRESETS.len(), PRESETS.len());
    }
}
