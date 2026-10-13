//! Simulation parameters (PLAN.md §7.1).

use crate::SCHEMA_VERSION;
use serde::{Deserialize, Serialize};

/// Everything that defines a run. Together with the model version, equal
/// parameters give equal output on every platform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SimulationParams {
    pub schema_version: String,
    pub seed: u64,
    pub fidelity: Fidelity,
    /// Starting point for defaults; explicit fields win.
    pub preset: Option<PresetId>,
    pub system: SystemSpec,
    /// Age of the system in Gyr. `None` = sampled by the model.
    pub age_gyr: Option<f64>,
    /// Metallicity [Fe/H] in dex. `None`: solar in the Classic tier, drawn
    /// from the solar neighbourhood's distribution in the Standard tier.
    pub metallicity_feh: Option<f64>,
    pub disk: DiskParams,
    pub formation: FormationParams,
    pub dynamics: DynamicsParams,
    pub toggles: Toggles,
    pub timeline: TimelineParams,
}

impl Default for SimulationParams {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_string(),
            seed: 1,
            fidelity: Fidelity::Classic,
            preset: None,
            system: SystemSpec::default(),
            age_gyr: None,
            metallicity_feh: None,
            disk: DiskParams::default(),
            formation: FormationParams::default(),
            dynamics: DynamicsParams::default(),
            toggles: Toggles::default(),
            timeline: TimelineParams::default(),
        }
    }
}

/// Fidelity tier (PLAN.md §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fidelity {
    /// Port of Accrete/StarGen.
    Classic,
    /// Modern semi-analytic pipeline.
    Standard,
    /// Standard plus N-body late stage (M7).
    High,
}

/// Named starting configurations (PLAN.md §9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetId {
    #[default]
    SunLike,
    RedDwarf,
    HotJupiter,
    MassiveAStar,
    MetalPoor,
    Circumbinary,
    STypeBinary,
    HierarchicalTriple,
    Random,
}

/// Which stars the system has.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemSpec {
    /// Sampled by the model, optionally constrained.
    Auto {
        primary_mass_msun: Option<f64>,
        multiplicity: Option<Multiplicity>,
    },
    /// Given explicitly as a hierarchy.
    Explicit(StarNode),
}

impl Default for SystemSpec {
    fn default() -> Self {
        SystemSpec::Auto {
            primary_mass_msun: Some(1.0),
            multiplicity: Some(Multiplicity::Single),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Multiplicity {
    Single,
    Binary,
    Triple,
}

/// A stellar hierarchy: a star, or a pair of sub-hierarchies in mutual orbit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StarNode {
    Star(StarSpec),
    Pair {
        a: Box<StarNode>,
        b: Box<StarNode>,
        orbit: OrbitSpec,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StarSpec {
    pub mass_msun: f64,
}

/// Mutual orbit of a pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrbitSpec {
    pub a_au: f64,
    pub e: f64,
}

/// Protoplanetary disk parameters (Standard tier). `None` means "drawn by
/// the model from its distribution".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiskParams {
    /// Initial gas mass as a fraction of the stellar mass.
    pub mass_fraction: Option<f64>,
    /// Median of the distribution `mass_fraction` is drawn from.
    pub mass_fraction_median: f64,
    /// Time until the gas begins to clear, Myr.
    pub lifetime_myr: Option<f64>,
    /// Shakura–Sunyaev α of the disk's global evolution.
    pub alpha_viscous: f64,
}

impl Default for DiskParams {
    fn default() -> Self {
        Self {
            mass_fraction: None,
            mass_fraction_median: 0.012,
            lifetime_myr: None,
            alpha_viscous: 5.0e-3,
        }
    }
}

/// Formation parameters, per tier.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FormationParams {
    pub classic: ClassicParams,
    pub standard: StandardParams,
}

/// Standard-tier formation parameters (PLAN.md §5.3–5.6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StandardParams {
    /// Turbulent α at the disk midplane: pebble layer thickness and
    /// isolation mass.
    pub alpha_turbulent: f64,
    /// α that refills a planet's gap: gap depth, type II migration and the
    /// limit on gas accretion. By default the same as `alpha_turbulent`.
    pub alpha_gap: f64,
    /// Stokes number of drift-limited pebbles.
    pub stokes_drift: f64,
    /// Fraction of the pebbles entering a pressure trap that become
    /// planetesimals (Lenz et al. 2019).
    pub planetesimal_efficiency: f64,
    /// Time for the envelope of a 1 Earth-mass planet to cool and contract,
    /// years; scales with the envelope's (uncertain) opacity.
    pub envelope_cooling_time_yr: f64,
    /// Factor applied to the disk migration rate; below 1 it stands in for
    /// torques the model leaves out.
    pub migration_efficiency: f64,
}

impl Default for StandardParams {
    fn default() -> Self {
        Self {
            alpha_turbulent: 1.0e-4,
            alpha_gap: 1.0e-4,
            stokes_drift: 0.05,
            planetesimal_efficiency: 0.1,
            envelope_cooling_time_yr: 1.0e9,
            migration_efficiency: 0.2,
        }
    }
}

/// Classic-tier parameters (PLAN.md §7.2). Defaults are StarGen's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClassicParams {
    /// Dole's *A*: dust density scale, M☉ AU⁻³ (multiplied by √M★).
    pub dust_density_coeff: f64,
    /// Dole's *K*: gas-to-dust mass ratio.
    pub gas_dust_ratio: f64,
    /// Dole's *B*: critical mass for gas capture, M☉.
    pub crit_mass_coeff_msun: f64,
    /// Dole's *α* in the dust density falloff `exp(-α·a^(1/n))`.
    pub alpha: f64,
    /// Dole's *n* in the dust density falloff.
    pub n: f64,
    /// Exponent of the nucleus eccentricity distribution.
    pub eccentricity_coeff: f64,
    /// Eccentricity of the dust cloud.
    pub cloud_eccentricity: f64,
    /// Mass of an injected nucleus, M☉.
    pub protoplanet_mass_msun: f64,
}

impl Default for ClassicParams {
    fn default() -> Self {
        Self {
            dust_density_coeff: 2.0e-3,
            gas_dust_ratio: 50.0,
            crit_mass_coeff_msun: 1.2e-5,
            alpha: 5.0,
            n: 3.0,
            eccentricity_coeff: 0.077,
            cloud_eccentricity: 0.2,
            protoplanet_mass_msun: 1.0e-15,
        }
    }
}

/// Late-dynamics parameters. Empty until M4/M7.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DynamicsParams {}

/// Optional model components.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Toggles {
    /// Compute atmospheric composition.
    pub atmospheres: bool,
    pub migration: bool,
    pub moons: bool,
    pub rings: bool,
    pub late_instability: bool,
}

impl Default for Toggles {
    fn default() -> Self {
        Self {
            atmospheres: true,
            migration: true,
            moons: true,
            rings: true,
            late_instability: true,
        }
    }
}

/// How much formation history to record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TimelineParams {
    /// Record individual events. Keyframes are recorded either way.
    pub record_events: bool,
    /// Number of evenly spaced full-state keyframes.
    pub keyframe_count: u32,
    /// The Classic model has no time axis. Its events are spread evenly, in
    /// order, over this nominal window so that playback has a clock.
    pub classic_window_myr: f64,
}

impl Default for TimelineParams {
    fn default() -> Self {
        Self {
            record_events: true,
            keyframe_count: 32,
            classic_window_myr: 10.0,
        }
    }
}
