//! The stellar system: which stars there are, how they orbit one another,
//! what they are like at the system's age, and where planets can exist
//! around them (PLAN.md §5.1, §5.2). Independent of the fidelity tier.
//!
//! - Stars given explicitly are used as given; otherwise the primary's mass
//!   comes from the Kroupa IMF and companions from observed multiplicity
//!   statistics (`sg_stellar::population`).
//! - A system is a binary tree: a leaf is a star, a node is two subsystems
//!   in mutual orbit. Every hierarchy must pass the Mardling & Aarseth
//!   (2001) stability test.
//! - Luminosity, radius and temperature come from main-sequence evolution
//!   (`sg_stellar::evolution`). Stars past the main sequence are not
//!   modelled, so the system's age is kept below the shortest main-sequence
//!   lifetime among its stars.
//! - Planets may form around a single star (S-type) inside the Holman &
//!   Wiegert (1999) limit set by its companion, and around a pair (P-type)
//!   outside the pair's limit.
//!
//! Simplifications: all orbits are coplanar; the planet limits are applied
//! to semi-major axes, as fitted, so an eccentric planet next to a limit
//! can reach across it; the stability limit for
//! planets also stands in for the truncation of the gas disk (Artymowicz &
//! Lubow 1994), which the disk model of M4 will treat properly.

use crate::SimError;
use sg_core::math::cbrt;
use sg_core::rng::RngStream;
use sg_core::units::{Au, Days, Kelvin, SolarLuminosity, SolarMass};
use sg_dynamics::stability;
use sg_schema::{
    HabitableZone, HierarchyNode, HostRef, Multiplicity, NodeId, Orbit, PlanetZone,
    SimulationParams, Star, StarId, StarNode, SystemSpec,
};
use sg_stellar::evolution::{self, MainSequence};
use sg_stellar::population;

/// Range of primary masses drawn when none is given. Below the upper end
/// the main sequence lasts at least half a billion years.
pub const SAMPLED_MASS_RANGE_MSUN: (f64, f64) = (0.1, 2.5);
/// Stars closer than this many times the sum of their radii at periastron
/// would interact (mass transfer, tides), which is not modelled.
const MIN_PERIASTRON_IN_RADII: f64 = 3.0;
/// Youngest and oldest systems drawn, Gyr.
const AGE_RANGE_GYR: (f64, f64) = (0.3, 10.0);
/// Fraction of the shortest main-sequence lifetime a system may reach.
const MAX_LIFETIME_FRACTION: f64 = 0.95;
const DAYS_PER_YEAR: f64 = 365.25;
/// Times a sampled system is redrawn if its stars would interact.
const SAMPLING_ATTEMPTS: u32 = 50;

/// A hierarchy with masses and orbits decided, before evolution.
#[derive(Debug, Clone, PartialEq)]
enum Layout {
    Star(f64),
    Pair {
        a: Box<Layout>,
        b: Box<Layout>,
        a_au: f64,
        e: f64,
    },
}

impl Layout {
    fn mass(&self) -> f64 {
        match self {
            Layout::Star(mass) => *mass,
            Layout::Pair { a, b, .. } => a.mass() + b.mass(),
        }
    }

    fn star_masses(&self, out: &mut Vec<f64>) {
        match self {
            Layout::Star(mass) => out.push(*mass),
            Layout::Pair { a, b, .. } => {
                a.star_masses(out);
                b.star_masses(out);
            }
        }
    }

    /// Semi-major axis of the widest orbit inside this subsystem.
    fn inner_extent_au(&self) -> Option<f64> {
        match self {
            Layout::Star(_) => None,
            Layout::Pair { a_au, .. } => Some(*a_au),
        }
    }
}

/// Semi-major axis from Kepler's third law (AU, M☉, days).
fn semi_major_axis(period: Days, total_mass: f64) -> f64 {
    let years = period.0 / DAYS_PER_YEAR;
    cbrt(total_mass * years * years)
}

fn period_days(a_au: f64, total_mass: f64) -> f64 {
    sg_dynamics::kepler::orbital_period(Au(a_au), SolarMass(total_mass)).0
}

fn invalid<T>(message: String) -> Result<T, SimError> {
    Err(SimError::InvalidParams(message))
}

fn from_spec(node: &StarNode) -> Result<Layout, SimError> {
    match node {
        StarNode::Star(spec) => {
            let range = evolution::MIN_MASS_MSUN..=evolution::MAX_MASS_MSUN;
            if !range.contains(&spec.mass_msun) {
                return invalid(format!(
                    "stellar mass must be between {} and {} M☉, got {}",
                    evolution::MIN_MASS_MSUN,
                    evolution::MAX_MASS_MSUN,
                    spec.mass_msun
                ));
            }
            Ok(Layout::Star(spec.mass_msun))
        }
        StarNode::Pair { a, b, orbit } => {
            if !(orbit.a_au.is_finite() && orbit.a_au > 0.0) {
                return invalid(format!(
                    "a pair's separation must be positive, got {} AU",
                    orbit.a_au
                ));
            }
            if !(0.0..1.0).contains(&orbit.e) {
                return invalid(format!(
                    "a pair's eccentricity must be in [0, 1), got {}",
                    orbit.e
                ));
            }
            Ok(Layout::Pair {
                a: Box::new(from_spec(a)?),
                b: Box::new(from_spec(b)?),
                a_au: orbit.a_au,
                e: orbit.e,
            })
        }
    }
}

/// Draws a companion orbit for a pair of the given masses.
fn sample_orbit(rng: &mut RngStream, primary: f64, total_mass: f64) -> (f64, f64) {
    let period = population::sample_period(rng, SolarMass(primary));
    let e = population::sample_eccentricity(rng, period);
    (semi_major_axis(period, total_mass), e)
}

fn sample(
    rng: &mut RngStream,
    primary_mass: Option<f64>,
    multiplicity: Option<Multiplicity>,
) -> Result<Layout, SimError> {
    let primary = match primary_mass {
        Some(mass) => {
            from_spec(&StarNode::Star(sg_schema::StarSpec { mass_msun: mass }))?;
            mass
        }
        None => {
            population::kroupa_mass(
                rng.uniform(),
                SolarMass(SAMPLED_MASS_RANGE_MSUN.0),
                SolarMass(SAMPLED_MASS_RANGE_MSUN.1),
            )
            .0
        }
    };
    let multiplicity = multiplicity.unwrap_or_else(|| {
        let fractions = population::multiplicity_fractions(SolarMass(primary));
        let u = rng.uniform();
        if u < fractions.single {
            Multiplicity::Single
        } else if u < fractions.single + fractions.binary {
            Multiplicity::Binary
        } else {
            // Systems of four or more are drawn as triples.
            Multiplicity::Triple
        }
    });

    match multiplicity {
        Multiplicity::Single => Ok(Layout::Star(primary)),
        Multiplicity::Binary => {
            let secondary = population::sample_companion_mass(rng, SolarMass(primary)).0;
            let (a_au, e) = sample_orbit(rng, primary, primary + secondary);
            Ok(Layout::Pair {
                a: Box::new(Layout::Star(primary)),
                b: Box::new(Layout::Star(secondary)),
                a_au,
                e,
            })
        }
        Multiplicity::Triple => {
            let secondary = population::sample_companion_mass(rng, SolarMass(primary)).0;
            let tertiary = population::sample_companion_mass(rng, SolarMass(primary)).0;
            let inner_mass = primary + secondary;
            // Two orbits from the same distribution; the tighter is the
            // inner pair's. Redraw until the hierarchy is stable.
            for _ in 0..200 {
                let first = sample_orbit(rng, primary, inner_mass);
                let second = sample_orbit(rng, primary, inner_mass + tertiary);
                let (inner, outer) = if first.0 <= second.0 {
                    (first, second)
                } else {
                    (second, first)
                };
                if stability::triple_is_stable(
                    Au(inner.0),
                    Au(outer.0),
                    outer.1,
                    SolarMass(inner_mass),
                    SolarMass(tertiary),
                ) {
                    return Ok(Layout::Pair {
                        a: Box::new(Layout::Pair {
                            a: Box::new(Layout::Star(primary)),
                            b: Box::new(Layout::Star(secondary)),
                            a_au: inner.0,
                            e: inner.1,
                        }),
                        b: Box::new(Layout::Star(tertiary)),
                        a_au: outer.0,
                        e: outer.1,
                    });
                }
            }
            invalid("no stable triple was found for these stars in 200 draws".into())
        }
    }
}

/// Checks every level of a hierarchy against the triple-stability test.
fn check_hierarchy(layout: &Layout) -> Result<(), SimError> {
    let Layout::Pair { a, b, a_au, e } = layout else {
        return Ok(());
    };
    for (inner, outer) in [(a, b), (b, a)] {
        if let Some(inner_au) = inner.inner_extent_au() {
            let needed = stability::triple_critical_ratio(
                SolarMass(inner.mass()),
                SolarMass(outer.mass()),
                *e,
            ) * inner_au
                / (1.0 - e);
            if *a_au <= needed {
                return invalid(format!(
                    "this hierarchy is not stable: an orbit of {a_au:.3} AU (e = {e:.2}) around \
                     a pair {inner_au:.3} AU apart must be wider than {needed:.3} AU \
                     (Mardling & Aarseth 2001)"
                ));
            }
        }
    }
    check_hierarchy(a)?;
    check_hierarchy(b)
}

/// One star at the system's age.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StarState {
    pub id: StarId,
    pub mass_msun: f64,
    pub luminosity_lsun: f64,
    pub radius_rsun: f64,
    pub teff_k: f64,
    pub lifetime_gyr: f64,
}

/// A place where planets may form.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DiskPlan {
    pub host: HostRef,
    /// Mass the disk orbits, M☉.
    pub central_mass_msun: f64,
    /// Combined luminosity of the stars it orbits, L☉.
    pub luminosity_lsun: f64,
    /// Shortest main-sequence lifetime among them, Gyr.
    pub lifetime_gyr: f64,
    /// Stars the disk orbits: (luminosity, temperature), for the habitable zone.
    pub stars: Vec<(SolarLuminosity, Kelvin)>,
    /// Stability limits, AU. `None` where nothing limits the disk.
    pub inner_limit_au: Option<f64>,
    pub outer_limit_au: Option<f64>,
    /// Letters of the stars it orbits, e.g. `A` or `AB`; empty for a
    /// single-star system.
    pub designation: String,
}

/// The finished stellar system.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StellarSystem {
    pub age_gyr: f64,
    pub hierarchy: HierarchyNode,
    pub stars: Vec<StarState>,
    pub disks: Vec<DiskPlan>,
    pub warnings: Vec<String>,
}

struct Builder<'a> {
    rng: &'a mut RngStream,
    tracks: &'a MainSequence,
    age_gyr: f64,
    age_myr: f64,
    name: &'a str,
    multiple: bool,
    next_star: u32,
    next_node: u32,
    stars: Vec<StarState>,
    disks: Vec<DiskPlan>,
    warnings: Vec<String>,
}

/// What a subtree contributes to its parent.
struct Built {
    node: HierarchyNode,
    mass: f64,
    /// Indices into `Builder::stars`.
    members: Vec<usize>,
    letters: String,
}

impl Builder<'_> {
    fn angle(&mut self) -> f64 {
        self.rng.uniform_range(0.0, 360.0)
    }

    /// Builds a subtree. `outer_limit_au` is the stability limit imposed on
    /// planets of this subtree by everything outside it.
    fn build(
        &mut self,
        layout: &Layout,
        letters: &mut impl Iterator<Item = char>,
        outer_limit_au: Option<f64>,
    ) -> Result<Built, SimError> {
        match layout {
            Layout::Star(mass) => {
                let state = self
                    .tracks
                    .state(SolarMass(*mass), self.age_myr)
                    .map_err(|err| SimError::InvalidParams(err.to_string()))?;
                let lifetime_gyr = self
                    .tracks
                    .lifetime_myr(SolarMass(*mass))
                    .map_err(|err| SimError::InvalidParams(err.to_string()))?
                    / 1000.0;
                let id = StarId(self.next_star);
                self.next_star += 1;
                let letter = letters.next().unwrap_or('?').to_string();
                let name = if self.multiple {
                    format!("{} {letter}", self.name)
                } else {
                    self.name.to_string()
                };
                let index = self.stars.len();
                self.stars.push(StarState {
                    id,
                    mass_msun: *mass,
                    luminosity_lsun: state.luminosity.0,
                    radius_rsun: state.radius.0,
                    teff_k: state.teff.0,
                    lifetime_gyr,
                });
                self.disks.push(DiskPlan {
                    host: HostRef::Star(id),
                    central_mass_msun: *mass,
                    luminosity_lsun: state.luminosity.0,
                    lifetime_gyr,
                    stars: vec![(state.luminosity, state.teff)],
                    inner_limit_au: None,
                    outer_limit_au,
                    designation: if self.multiple {
                        letter.clone()
                    } else {
                        String::new()
                    },
                });
                Ok(Built {
                    node: HierarchyNode::Star(Star {
                        id,
                        name,
                        mass_msun: *mass,
                        luminosity_lsun: state.luminosity.0,
                        radius_rsun: state.radius.0,
                        teff_k: state.teff.0,
                        spectral_class: sg_stellar::spectral_class(state.teff).to_string(),
                        color_linear_rgb: sg_stellar::blackbody_linear_srgb(state.teff),
                        age_gyr: self.age_gyr,
                        main_sequence_lifetime_gyr: lifetime_gyr,
                    }),
                    mass: *mass,
                    members: vec![index],
                    letters: letter,
                })
            }
            Layout::Pair { a, b, a_au, e } => {
                let limit = |host: &Layout, companion: &Layout| {
                    let own = stability::s_type_critical_ratio(
                        SolarMass(host.mass()),
                        SolarMass(companion.mass()),
                        *e,
                    ) * a_au;
                    Some(outer_limit_au.map_or(own, |outer| outer.min(own)))
                };
                let id = NodeId(self.next_node);
                self.next_node += 1;
                let first = self.build(a, letters, limit(a, b))?;
                let second = self.build(b, letters, limit(b, a))?;
                let mass = first.mass + second.mass;

                let periastron = a_au * (1.0 - e);
                let radii_au: f64 = first
                    .members
                    .iter()
                    .chain(&second.members)
                    .map(|i| self.stars[*i].radius_rsun)
                    .sum::<f64>()
                    * sg_core::constants::SOLAR_RADIUS_M
                    / sg_core::constants::AU_M;
                if periastron < MIN_PERIASTRON_IN_RADII * radii_au {
                    return invalid(format!(
                        "the stars of a pair {a_au:.4} AU apart (e = {e:.2}) come within \
                         {periastron:.4} AU, closer than {MIN_PERIASTRON_IN_RADII} times their \
                         combined radii ({radii_au:.4} AU); interacting stars are not modelled"
                    ));
                }

                let members: Vec<usize> = first
                    .members
                    .iter()
                    .chain(&second.members)
                    .copied()
                    .collect();
                let letters_here = format!("{}{}", first.letters, second.letters);
                let lighter = first.mass.min(second.mass);
                self.disks.push(DiskPlan {
                    host: HostRef::Barycenter(id),
                    central_mass_msun: mass,
                    luminosity_lsun: members.iter().map(|i| self.stars[*i].luminosity_lsun).sum(),
                    lifetime_gyr: members
                        .iter()
                        .map(|i| self.stars[*i].lifetime_gyr)
                        .fold(f64::INFINITY, f64::min),
                    stars: members
                        .iter()
                        .map(|i| {
                            let s = &self.stars[*i];
                            (SolarLuminosity(s.luminosity_lsun), Kelvin(s.teff_k))
                        })
                        .collect(),
                    inner_limit_au: Some(
                        stability::p_type_critical_ratio(
                            SolarMass(mass - lighter),
                            SolarMass(lighter),
                            *e,
                        ) * a_au,
                    ),
                    outer_limit_au,
                    designation: letters_here.clone(),
                });

                let orbit = Orbit {
                    a_au: *a_au,
                    e: *e,
                    i_deg: 0.0,
                    long_asc_node_deg: self.angle(),
                    arg_periapsis_deg: self.angle(),
                    mean_anomaly_deg: self.angle(),
                    period_days: period_days(*a_au, mass),
                };
                Ok(Built {
                    node: HierarchyNode::Pair {
                        id,
                        a: Box::new(first.node),
                        b: Box::new(second.node),
                        orbit,
                    },
                    mass,
                    members,
                    letters: letters_here,
                })
            }
        }
    }
}

/// Builds the stellar system for a run. `name` is the system's name.
pub(crate) fn build(
    params: &SimulationParams,
    rng: &mut RngStream,
    name: &str,
) -> Result<StellarSystem, SimError> {
    let mut warnings = Vec::new();

    let z = match params.metallicity_feh {
        None => evolution::SOLAR_Z,
        Some(feh) => {
            if !feh.is_finite() {
                return invalid(format!("metallicity must be finite, got {feh}"));
            }
            let z = evolution::z_from_feh(feh);
            let clamped = z.clamp(evolution::MIN_Z, evolution::MAX_Z);
            if clamped != z {
                warnings.push(format!(
                    "the stellar model does not reach [Fe/H] = {feh}: the stars were computed \
                     with Z = {clamped}, its nearest limit"
                ));
            }
            clamped
        }
    };
    let tracks = MainSequence::new(z).map_err(|err| SimError::InvalidParams(err.to_string()))?;

    match &params.system {
        SystemSpec::Explicit(node) => {
            assemble(params, rng, name, &tracks, &from_spec(node)?, warnings)
        }
        SystemSpec::Auto {
            primary_mass_msun,
            multiplicity,
        } => {
            // A drawn system can turn out to have stars that would touch;
            // draw again rather than fail.
            let mut last = None;
            for _ in 0..SAMPLING_ATTEMPTS {
                let layout = sample(rng, *primary_mass_msun, *multiplicity)?;
                match assemble(params, rng, name, &tracks, &layout, warnings.clone()) {
                    Ok(system) => return Ok(system),
                    Err(err) => last = Some(err),
                }
            }
            Err(last.unwrap_or_else(|| SimError::InvalidParams("no system was drawn".into())))
        }
    }
}

/// Evolves the stars of a decided hierarchy and plans its disks.
fn assemble(
    params: &SimulationParams,
    rng: &mut RngStream,
    name: &str,
    tracks: &MainSequence,
    layout: &Layout,
    mut warnings: Vec<String>,
) -> Result<StellarSystem, SimError> {
    check_hierarchy(layout)?;

    // The system must be younger than its shortest-lived star's main
    // sequence, since later stages are not modelled.
    let mut masses = Vec::new();
    layout.star_masses(&mut masses);
    let mut shortest_gyr = f64::INFINITY;
    for mass in &masses {
        let lifetime = tracks
            .lifetime_myr(SolarMass(*mass))
            .map_err(|err| SimError::InvalidParams(err.to_string()))?
            / 1000.0;
        shortest_gyr = shortest_gyr.min(lifetime);
    }
    let oldest = MAX_LIFETIME_FRACTION * shortest_gyr;
    let age_gyr = match params.age_gyr {
        Some(age) => {
            if !(age.is_finite() && age > 0.0) {
                return invalid(format!("age_gyr must be positive and finite, got {age}"));
            }
            if age > oldest {
                warnings.push(format!(
                    "at {age} Gyr the shortest-lived star would have left the main sequence \
                     (after {shortest_gyr:.2} Gyr); evolved stars are not modelled, so the age \
                     was set to {oldest:.2} Gyr"
                ));
            }
            age.min(oldest)
        }
        None => {
            let hi = AGE_RANGE_GYR.1.min(oldest);
            let lo = AGE_RANGE_GYR.0.min(0.5 * hi);
            rng.uniform_range(lo, hi)
        }
    };

    // Letters go to stars in order of decreasing mass: A is the heaviest.
    let mut order: Vec<usize> = (0..masses.len()).collect();
    order.sort_by(|x, y| masses[*y].total_cmp(&masses[*x]).then(x.cmp(y)));
    let mut letter_of = vec!['?'; masses.len()];
    for (rank, index) in order.iter().enumerate() {
        letter_of[*index] = char::from(b'A' + (rank as u8).min(25));
    }

    let mut builder = Builder {
        rng,
        tracks,
        age_gyr,
        age_myr: age_gyr * 1000.0,
        name,
        multiple: masses.len() > 1,
        next_star: 0,
        next_node: 0,
        stars: Vec::new(),
        disks: Vec::new(),
        warnings,
    };
    let built = builder.build(layout, &mut letter_of.into_iter(), None)?;
    // Disks in a fixed order: by host, stars before barycentres.
    builder.disks.sort_by_key(|d| match d.host {
        HostRef::Star(id) => (0, id.0),
        HostRef::Barycenter(id) => (1, id.0),
        HostRef::Body(id) => (2, id.0),
    });

    Ok(StellarSystem {
        age_gyr,
        hierarchy: built.node,
        stars: builder.stars,
        disks: builder.disks,
        warnings: builder.warnings,
    })
}

/// The habitable zone of a disk's stars, if their temperatures are within
/// the model's range (Kopparapu et al. 2014; combined for a pair).
pub(crate) fn habitable_zone(disk: &DiskPlan) -> Result<HabitableZone, String> {
    let hz = sg_stellar::combined_habitable_zone(&disk.stars).map_err(|err| err.to_string())?;
    Ok(HabitableZone {
        host: disk.host,
        optimistic_inner_au: hz.recent_venus.0,
        conservative_inner_au: hz.runaway_greenhouse.0,
        conservative_outer_au: hz.maximum_greenhouse.0,
        optimistic_outer_au: hz.early_mars.0,
    })
}

/// The zone a disk's planets were confined to, for the snapshot.
pub(crate) fn planet_zone(disk: &DiskPlan, inner_au: f64, outer_au: f64) -> PlanetZone {
    PlanetZone {
        host: disk.host,
        inner_au,
        outer_au,
        limited_inside: disk.inner_limit_au.is_some(),
        limited_outside: disk.outer_limit_au.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sg_core::rng::ModuleId;
    use sg_schema::{OrbitSpec, StarSpec};

    fn star(mass: f64) -> Box<StarNode> {
        Box::new(StarNode::Star(StarSpec { mass_msun: mass }))
    }

    fn pair(a: Box<StarNode>, b: Box<StarNode>, a_au: f64, e: f64) -> Box<StarNode> {
        Box::new(StarNode::Pair {
            a,
            b,
            orbit: OrbitSpec { a_au, e },
        })
    }

    fn run(system: SystemSpec, seed: u64) -> Result<StellarSystem, SimError> {
        let params = SimulationParams {
            seed,
            system,
            ..SimulationParams::default()
        };
        build(
            &params,
            &mut RngStream::new(seed, ModuleId::STELLAR),
            "Test",
        )
    }

    #[allow(clippy::boxed_local)]
    fn explicit(node: Box<StarNode>) -> StellarSystem {
        run(SystemSpec::Explicit(*node), 1).unwrap()
    }

    fn disk(system: &StellarSystem, host: HostRef) -> &DiskPlan {
        system.disks.iter().find(|d| d.host == host).unwrap()
    }

    /// Kepler-16: planets around both stars, no closer than about 0.65 AU;
    /// around each star alone, only within a few hundredths of an AU.
    #[test]
    fn close_binary_gets_a_circumbinary_zone() {
        let system = explicit(pair(star(0.6897), star(0.2026), 0.2243, 0.159));
        assert_eq!(system.stars.len(), 2);
        assert_eq!(system.disks.len(), 3);
        let around_both = disk(&system, HostRef::Barycenter(NodeId(0)));
        assert!((around_both.inner_limit_au.unwrap() - 0.646).abs() < 0.01);
        assert_eq!(around_both.outer_limit_au, None);
        assert!((around_both.central_mass_msun - 0.8923).abs() < 1e-12);
        assert_eq!(around_both.designation, "AB");
        let around_a = disk(&system, HostRef::Star(StarId(0)));
        assert!(around_a.outer_limit_au.unwrap() < 0.08);
        assert_eq!(around_a.designation, "A");
    }

    /// α Centauri AB: each star keeps planets out to roughly 2.5–2.8 AU.
    #[test]
    fn wide_binary_gets_two_circumstellar_zones() {
        let system = explicit(pair(star(1.08), star(0.91), 23.5, 0.52));
        let a = disk(&system, HostRef::Star(StarId(0)));
        let b = disk(&system, HostRef::Star(StarId(1)));
        assert!((a.outer_limit_au.unwrap() - 2.8).abs() < 0.15);
        assert!((b.outer_limit_au.unwrap() - 2.5).abs() < 0.2);
        // Around both, nothing is stable inside about 80 AU.
        assert!(
            disk(&system, HostRef::Barycenter(NodeId(0)))
                .inner_limit_au
                .unwrap()
                > 70.0
        );
        let HierarchyNode::Pair { orbit, .. } = &system.hierarchy else {
            panic!("pair expected");
        };
        // Kepler's third law: 23.5 AU and 1.99 M☉ give 80.8 years.
        assert!((orbit.period_days / 365.25 - 80.8).abs() < 0.3);
    }

    #[test]
    fn triple_limits_nest() {
        let system = explicit(pair(
            pair(star(1.0), star(0.7), 0.25, 0.1),
            star(0.4),
            60.0,
            0.3,
        ));
        assert_eq!(system.stars.len(), 3);
        assert_eq!(system.disks.len(), 5);
        // The inner pair's circumbinary zone runs from its own limit out to
        // the limit set by the third star.
        let inner_pair = disk(&system, HostRef::Barycenter(NodeId(1)));
        let (inner, outer) = (
            inner_pair.inner_limit_au.unwrap(),
            inner_pair.outer_limit_au.unwrap(),
        );
        assert!(inner > 0.5 && inner < 1.0, "{inner}");
        assert!((outer - 14.8).abs() < 0.5, "{outer}");
        // Stars of the inner pair are limited by each other, not by the third.
        assert!(
            disk(&system, HostRef::Star(StarId(0)))
                .outer_limit_au
                .unwrap()
                < 0.1
        );
        // The third star's zone, and the zone around all three.
        assert!(
            (disk(&system, HostRef::Star(StarId(2)))
                .outer_limit_au
                .unwrap()
                - 6.5)
                .abs()
                < 0.5
        );
        assert!(
            disk(&system, HostRef::Barycenter(NodeId(0)))
                .inner_limit_au
                .unwrap()
                > 150.0
        );
        assert_eq!(inner_pair.designation, "AB");
    }

    #[test]
    fn unstable_or_touching_hierarchies_are_rejected() {
        // A third star at 1 AU around a 0.25 AU pair.
        let unstable = pair(pair(star(1.0), star(0.7), 0.25, 0.1), star(0.4), 1.0, 0.3);
        let err = run(SystemSpec::Explicit(*unstable), 1).unwrap_err();
        assert!(err.to_string().contains("not stable"), "{err}");
        // Two Suns 0.01 AU apart: about two solar radii.
        let touching = pair(star(1.0), star(1.0), 0.01, 0.0);
        let err = run(SystemSpec::Explicit(*touching), 1).unwrap_err();
        assert!(err.to_string().contains("interacting"), "{err}");
        assert!(run(SystemSpec::Explicit(*star(0.01)), 1).is_err());
        assert!(run(
            SystemSpec::Explicit(*pair(star(1.0), star(1.0), 5.0, 1.0)),
            1
        )
        .is_err());
    }

    #[test]
    fn stars_are_lettered_by_mass_and_evolved_to_one_age() {
        let system = explicit(pair(star(0.5), star(1.2), 40.0, 0.2));
        let HierarchyNode::Pair { a, b, .. } = &system.hierarchy else {
            panic!("pair expected");
        };
        let (HierarchyNode::Star(light), HierarchyNode::Star(heavy)) = (&**a, &**b) else {
            panic!("two stars expected");
        };
        assert_eq!(
            (light.name.as_str(), heavy.name.as_str()),
            ("Test B", "Test A")
        );
        assert_eq!(light.age_gyr, heavy.age_gyr);
        assert!(heavy.luminosity_lsun > 1.5 && light.luminosity_lsun < 0.1);
        // Younger than the heavier star's main-sequence lifetime.
        assert!(system.age_gyr < heavy.main_sequence_lifetime_gyr);
        assert!(heavy.main_sequence_lifetime_gyr < light.main_sequence_lifetime_gyr);
    }

    #[test]
    fn a_fixed_age_past_the_main_sequence_is_reduced_with_a_warning() {
        let params = SimulationParams {
            age_gyr: Some(8.0),
            system: SystemSpec::Explicit(*star(2.0)),
            ..SimulationParams::default()
        };
        let system = build(&params, &mut RngStream::new(1, ModuleId::STELLAR), "Test").unwrap();
        assert!(system.age_gyr < 1.2, "{}", system.age_gyr);
        assert_eq!(system.warnings.len(), 1);
        assert!(system.warnings[0].contains("main sequence"));
    }

    #[test]
    fn sampled_systems_are_valid_and_follow_the_statistics() {
        let auto = SystemSpec::Auto {
            primary_mass_msun: None,
            multiplicity: None,
        };
        let (mut singles, mut binaries, mut triples) = (0, 0, 0);
        let n = 3000;
        for seed in 1..=n {
            let system = run(auto.clone(), seed).unwrap();
            match system.stars.len() {
                1 => singles += 1,
                2 => binaries += 1,
                3 => triples += 1,
                other => panic!("{other} stars"),
            }
            for star in &system.stars {
                assert!((0.08..=2.5).contains(&star.mass_msun));
                assert!(system.age_gyr <= 0.95 * star.lifetime_gyr + 1e-9);
            }
            assert!(system.age_gyr > 0.0 && system.age_gyr <= 10.0);
            assert_eq!(system, run(auto.clone(), seed).unwrap());
        }
        // Most primaries are red dwarfs, of which about three quarters are
        // single; a few percent of systems are triples.
        let frac = |count: i32| f64::from(count) / n as f64;
        assert!((0.62..0.80).contains(&frac(singles)), "{singles}");
        assert!((0.15..0.32).contains(&frac(binaries)), "{binaries}");
        assert!((0.02..0.10).contains(&frac(triples)), "{triples}");
    }

    #[test]
    fn requested_multiplicity_is_honoured() {
        for (multiplicity, count) in [
            (Multiplicity::Single, 1),
            (Multiplicity::Binary, 2),
            (Multiplicity::Triple, 3),
        ] {
            for seed in 1..60 {
                let system = run(
                    SystemSpec::Auto {
                        primary_mass_msun: Some(1.0),
                        multiplicity: Some(multiplicity),
                    },
                    seed,
                )
                .unwrap();
                assert_eq!(system.stars.len(), count);
                assert!(system.stars.iter().all(|s| s.mass_msun <= 1.0));
            }
        }
    }
}
