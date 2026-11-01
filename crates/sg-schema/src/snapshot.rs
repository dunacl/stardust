//! The finished system (PLAN.md §7.3).

use serde::{Deserialize, Serialize};

/// Identifies a planet, moon or dwarf planet within one system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BodyId(pub u32);

/// Identifies a star within one system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct StarId(pub u32);

/// Identifies a node (star or barycentre) of the stellar hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u32);

/// Identifies a protoplanetary disk within one system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DiskId(pub u32);

/// A system at its present age.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemSnapshot {
    pub schema_version: String,
    /// Version of the physical model; a seed is reproducible only within it.
    pub model_version: String,
    /// Fingerprint of the parameters that produced this snapshot.
    pub params_hash: String,
    pub age_gyr: f64,
    pub hierarchy: HierarchyNode,
    /// Planets, moons and dwarf planets, planets ordered outward.
    pub bodies: Vec<Body>,
    pub belts: Vec<Belt>,
    /// A few comets drawn to show; the counts are in `comet_reservoirs`.
    /// None in the Classic tier.
    pub comets: Vec<Comet>,
    /// What each host's icy belts and planets give in comets.
    pub comet_reservoirs: Vec<CometReservoir>,
    pub disks_at_dispersal: Vec<DiskSummary>,
    pub habitable_zones: Vec<HabitableZone>,
    /// Where planets could form, one entry per star or pair that has room
    /// for any.
    pub planet_zones: Vec<PlanetZone>,
}

impl SystemSnapshot {
    /// The planets, in the order of `bodies` (outward for each host).
    /// `bodies` also holds their moons, after all the planets.
    pub fn planets(&self) -> impl Iterator<Item = &Body> {
        self.bodies.iter().filter(|b| b.kind != BodyKind::Moon)
    }

    /// The moons of the body with this id, in order outward.
    pub fn moons_of(&self, planet: BodyId) -> impl Iterator<Item = &Body> {
        self.bodies
            .iter()
            .filter(move |b| b.kind == BodyKind::Moon && b.host == HostRef::Body(planet))
    }
}

/// Stars and their mutual orbits. A `Pair`'s `orbit` is the orbit of `b`
/// relative to `a`; each moves about their common centre of mass, which is
/// what `HostRef::Barycenter(id)` refers to. All orbits share one plane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HierarchyNode {
    Star(Star),
    Pair {
        id: NodeId,
        a: Box<HierarchyNode>,
        b: Box<HierarchyNode>,
        orbit: Orbit,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Star {
    pub id: StarId,
    /// Generated proper name, e.g. `Velora`. A label only.
    pub name: String,
    pub mass_msun: f64,
    pub luminosity_lsun: f64,
    pub radius_rsun: f64,
    /// Effective temperature.
    pub teff_k: f64,
    /// Harvard class with subclass, e.g. `G2`.
    pub spectral_class: String,
    /// Blackbody colour at `teff_k`: linear sRGB, largest channel 1.
    pub color_linear_rgb: [f64; 3],
    pub age_gyr: f64,
    /// Main-sequence lifetime.
    pub main_sequence_lifetime_gyr: f64,
    /// Share of the star's mass that is hydrogen.
    pub hydrogen_frac: f64,
    pub helium_frac: f64,
    /// Everything heavier.
    pub metal_frac: f64,
}

/// Keplerian orbit about the host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Orbit {
    /// Semi-major axis.
    pub a_au: f64,
    /// Eccentricity.
    pub e: f64,
    /// Inclination to the system's reference plane.
    pub i_deg: f64,
    /// Longitude of the ascending node, Ω.
    pub long_asc_node_deg: f64,
    /// Argument of periapsis, ω.
    pub arg_periapsis_deg: f64,
    /// Mean anomaly at the snapshot epoch, M₀.
    pub mean_anomaly_deg: f64,
    pub period_days: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyKind {
    Planet,
    Moon,
    DwarfPlanet,
}

/// What a body orbits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostRef {
    Star(StarId),
    Barycenter(NodeId),
    Body(BodyId),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Body {
    pub id: BodyId,
    /// Proper name, invented. A label only.
    pub name: String,
    /// Designation, e.g. `Velora b`: the host's name and a letter running
    /// outward from `b`. A label only.
    pub designation: String,
    pub kind: BodyKind,
    pub host: HostRef,
    pub orbit: Orbit,
    /// The likely surface mix by decreasing share, estimated. Empty for a
    /// body with no solid surface.
    pub minerals: Vec<MineralShare>,
    pub mass_mearth: f64,
    /// Equatorial radius.
    pub radius_km: f64,
    /// Mean density.
    pub density_g_cm3: f64,
    /// Surface gravity in Earth gravities; `None` without a solid surface.
    pub surface_gravity_gees: Option<f64>,
    pub escape_velocity_km_s: f64,
    /// Bond albedo.
    pub albedo: f64,
    /// Equilibrium temperature estimate.
    pub equilibrium_temp_k: f64,
    pub composition: Composition,
    pub rotation: Rotation,
    /// `None` for an airless body, and for a planet whose atmosphere the
    /// model does not describe (the Standard tier gives only hydrogen–helium
    /// envelopes).
    pub atmosphere: Option<Atmosphere>,
    /// `None` without a solid or liquid surface, and where the model has
    /// no climate (the Standard tier so far).
    pub climate: Option<Climate>,
    pub class: PlanetClass,
    pub habitability: Habitability,
    pub rings: Option<Rings>,
    pub visual: VisualDescriptor,
    pub formation: FormationSummary,
}

/// Mass fractions; they sum to 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Composition {
    pub metal_frac: f64,
    pub rock_frac: f64,
    pub water_frac: f64,
    pub volatiles_frac: f64,
    /// Hydrogen and helium.
    pub h_he_frac: f64,
}

/// A kind of material on a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mineral {
    Silicates,
    IronOxides,
    Sulfides,
    Carbonates,
    WaterIce,
    /// Nitrogen, methane and the like.
    VolatileIces,
    MoltenRock,
}

/// One part of a surface; an estimate (`sg_planet::minerals`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MineralShare {
    pub mineral: Mineral,
    /// Share of the surface, 0 to 1.
    pub share: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rotation {
    /// Sidereal rotation period. `None` where the model does not give one
    /// (the Standard tier, unless the planet is tidally locked).
    pub period_hours: Option<f64>,
    /// `None` where the model does not give one.
    pub obliquity_deg: Option<f64>,
    /// Rotation period equals the orbital period.
    pub tidally_locked: bool,
    /// Caught in a spin–orbit resonance other than 1:1.
    pub spin_orbit_resonance: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Atmosphere {
    /// `None` for a gas giant, which has no surface.
    pub surface_pressure_mbar: Option<f64>,
    /// Lightest molecular weight the body retains, g/mol.
    pub min_molecular_weight: f64,
    /// By decreasing partial pressure. Empty if not computed.
    pub gases: Vec<GasFraction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GasFraction {
    /// Chemical symbol or formula, e.g. `N`, `CO2`.
    pub symbol: String,
    pub partial_pressure_mbar: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Climate {
    /// Mean surface temperature.
    pub surface_temp_k: f64,
    pub day_temp_k: f64,
    pub night_temp_k: f64,
    /// Hottest: summer day.
    pub max_temp_k: f64,
    /// Coldest: winter night.
    pub min_temp_k: f64,
    /// Warming due to the greenhouse effect.
    pub greenhouse_rise_k: f64,
    pub runaway_greenhouse: bool,
    /// Boiling point of water at the surface; `None` if airless.
    pub water_boiling_point_k: Option<f64>,
    /// Fraction of the surface covered by liquid water.
    pub ocean_frac: f64,
    pub ice_frac: f64,
    pub cloud_frac: f64,
}

/// Planet class. The first twelve are the Classic tier's categories; the
/// rest are the Standard tier's (PLAN.md §5.9), assigned from composition
/// and equilibrium temperature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanetClass {
    /// Airless rubble.
    Asteroids,
    /// Airless rock.
    Rock,
    /// Thick atmosphere, runaway greenhouse.
    Venusian,
    /// Liquid water and land.
    Terrestrial,
    /// Thin atmosphere.
    Martian,
    /// Ocean world.
    Water,
    /// Ice world.
    Ice,
    /// Rotationally locked, with an atmosphere.
    OneFace,
    /// Small planet with a minor hydrogen envelope.
    GasDwarf,
    /// Neptune-like.
    SubJovian,
    /// Jupiter-like.
    Jovian,
    Unknown,
    /// Standard tier: bare rock under about 2 Earth masses.
    Rocky,
    /// Standard tier: bare rock of 2 Earth masses or more.
    SuperEarth,
    /// Standard tier: bare rock hot enough to melt on the dayside.
    LavaWorld,
    /// Standard tier: a tenth or more water, not all frozen.
    WaterWorld,
    /// Standard tier: a tenth or more water, frozen.
    IceWorld,
    /// Standard tier: a core under a hydrogen–helium envelope of under 10%.
    SubNeptune,
    /// Standard tier: envelope of 10–50%, like Uranus and Neptune.
    IceGiant,
    /// Standard tier: mostly hydrogen and helium.
    GasGiant,
    /// Standard tier: a gas giant hotter than 1000 K.
    HotJupiter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Habitability {
    /// Within the conservative habitable zone of its star.
    pub in_conservative_hz: bool,
    /// Within the optimistic habitable zone of its star.
    pub in_optimistic_hz: bool,
    /// The model finds liquid water on the surface.
    pub surface_liquid_water: bool,
}

/// Planetary rings. Not produced before M4.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rings {
    pub inner_radius_km: f64,
    pub outer_radius_km: f64,
    pub optical_depth: f64,
}

/// What the renderer needs to draw a body (PLAN.md §5.9, D12). Schema 0.1
/// carries only what the Classic tier knows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualDescriptor {
    /// Seed for procedural textures, derived from the run seed and body id.
    pub texture_seed: u64,
    pub ocean_frac: f64,
    pub ice_frac: f64,
    pub cloud_frac: f64,
}

/// How a body came to be.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormationSummary {
    /// Where its first embryo appeared.
    pub origin_a_au: f64,
    /// Other embryos it absorbed.
    pub mergers: u32,
    /// It grew past the critical mass and captured gas.
    pub captured_gas: bool,
}

/// A belt of small bodies. Not produced before M4.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Belt {
    /// e.g. `Velora Outer Belt`. A label only.
    pub name: String,
    pub host: HostRef,
    pub inner_au: f64,
    pub outer_au: f64,
    pub mass_mearth: f64,
    /// Where that mass lies, inward to outward. Most belts are far wider
    /// than the part of them any planet reaches.
    pub segments: Vec<BeltSegment>,
}

/// A stretch of a belt.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BeltSegment {
    pub inner_au: f64,
    pub outer_au: f64,
    pub mass_mearth: f64,
}

/// Identifies a comet within one snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CometId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CometFamily {
    /// Short period, low tilt, held by the system's giant.
    JupiterFamily,
    /// From an Oort cloud, on a long thin orbit tilted any way.
    LongPeriod,
}

/// One comet, drawn from what its family looks like: the model follows no
/// small body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comet {
    pub id: CometId,
    /// Proper name, invented. A label only.
    pub name: String,
    /// e.g. `3P/Velora` or `C/Velora`. A label only.
    pub designation: String,
    pub host: HostRef,
    pub family: CometFamily,
    pub orbit: Orbit,
    /// Diameter of the nucleus.
    pub diameter_km: f64,
}

/// What one host's belts and planets give in comets; orders of magnitude.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CometReservoir {
    pub host: HostRef,
    /// Icy belt mass within reach of the outermost planet of a few Earth
    /// masses or more (`sg_dynamics::comets::reach_au`).
    pub belt_mass_mearth: f64,
    /// Planets on the way in that eject most of what reaches them.
    pub ejectors: u32,
    /// Active comets above 2.3 km with perihelia in the visible range.
    pub active_comets: f64,
    /// `None` if nothing here can build an Oort cloud.
    pub oort_mass_mearth: Option<f64>,
}

/// A protoplanetary disk as the Standard tier modelled it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiskSummary {
    pub id: DiskId,
    pub host: HostRef,
    pub inner_au: f64,
    pub outer_au: f64,
    /// Time from the disk's formation until its gas began to clear.
    pub lifetime_myr: f64,
    /// Gas mass at the start.
    pub initial_mass_msun: f64,
    /// Dust-to-gas mass ratio at the start.
    pub dust_to_gas: f64,
    /// Where the water ice line was when the gas cleared.
    pub final_ice_line_au: f64,
    /// Radius `R₁` of the self-similar gas profile: most of the mass lies
    /// within it at the start.
    pub characteristic_radius_au: f64,
    /// Viscous time at `R₁`: how fast the gas drains onto the star and
    /// spreads outward.
    pub viscous_time_myr: f64,
}

/// The range of orbits open to planets of one host: inside the limit a
/// companion star imposes, or outside the limit a host pair imposes on its
/// own planets (Holman & Wiegert 1999), within the formation model's reach.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanetZone {
    pub host: HostRef,
    pub inner_au: f64,
    pub outer_au: f64,
    /// The inner edge is a stability limit (the host is a pair of stars).
    pub limited_inside: bool,
    /// The outer edge is a stability limit (a companion star).
    pub limited_outside: bool,
}

/// Habitable zone of one host (Kopparapu et al. 2014 limits). For a pair,
/// the zone of the two stars' combined light.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HabitableZone {
    pub host: HostRef,
    /// Optimistic inner edge (recent Venus).
    pub optimistic_inner_au: f64,
    /// Conservative inner edge (runaway greenhouse).
    pub conservative_inner_au: f64,
    /// Conservative outer edge (maximum greenhouse).
    pub conservative_outer_au: f64,
    /// Optimistic outer edge (early Mars).
    pub optimistic_outer_au: f64,
}
