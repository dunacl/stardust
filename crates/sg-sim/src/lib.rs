//! The public simulation API (PLAN.md §6.3).
//!
//! [`simulate`] turns [`SimulationParams`] into a [`SimulationResult`]: the
//! finished system, its formation timeline, and diagnostics. The same
//! parameters and [`MODEL_VERSION`] give the same result on every platform.
//!
//! Tiers available so far:
//!
//! - **Classic**, when the `classic` feature is on (the default): Dole's
//!   accretion model around modern stars, with stability limits for
//!   multiple-star systems (`classic.rs`).
//! - **Standard**: the modern pipeline of disk evolution, pebble and gas
//!   accretion, migration and atmospheric escape (`standard.rs`).
//!
//! High returns [`SimError::TierUnavailable`] until M7.

#[cfg(feature = "classic")]
mod classic;
mod comets;
mod high;
mod moons;
pub mod names;
mod presets;
mod stability;
mod standard;
pub use stability::{long_term_stability, HostStability};
mod stars;

pub use presets::{preset_availability, preset_params, PresetAvailability, ALL_PRESETS};

use sg_core::hash::fnv1a64_hex;
use sg_schema::{EventTimeline, Fidelity, SimulationParams, SystemSnapshot, SCHEMA_VERSION};

/// Version of the physical model. Bump whenever a change alters outputs for
/// existing parameters (CLAUDE.md hard rule 7).
pub const MODEL_VERSION: &str = "0.16.0";

/// Everything a run produces.
#[derive(Debug, Clone, PartialEq)]
pub struct SimulationResult {
    /// The system at its present age.
    pub snapshot: SystemSnapshot,
    /// Formation history, for playback.
    pub timeline: EventTimeline,
    pub diagnostics: Diagnostics,
}

/// Facts about a run that are not part of the system itself.
///
/// Wall-clock timings are deliberately absent: simulation code never reads
/// the clock (it is unavailable on `wasm32-unknown-unknown` and would make
/// results differ between runs). Callers that want timings measure around
/// [`simulate`].
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostics {
    pub fidelity: Fidelity,
    /// Where the stars' luminosities and radii came from.
    pub stellar_model: StellarModel,
    /// False when a tier cannot promise identical bits everywhere (GPU
    /// N-body, PLAN.md §6.4).
    pub bit_deterministic: bool,
    /// Random draws consumed by the formation stage.
    pub rng_draws: u64,
    /// Candidate systems discarded and resampled.
    pub rejected_samples: u32,
    /// Things the user should know, e.g. a quantity outside a model's
    /// validity range.
    pub warnings: Vec<String>,
}

/// The stellar relations a run used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StellarModel {
    /// Main-sequence evolution fits of Tout et al. (1996) and Hurley, Pols
    /// & Tout (2000). Used for everything else.
    HurleyPolsTout2000,
}

/// Pipeline stages, reported to a [`SimObserver`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Stellar,
    Formation,
    Planets,
    Assembly,
}

/// Returned by an observer to continue or stop a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Continue,
    Cancel,
}

/// Receives progress and may cancel a run. Called at stage boundaries;
/// long-running tiers will call it within stages too.
pub trait SimObserver {
    /// `fraction` is overall progress in `[0, 1]`.
    fn progress(&mut self, stage: Stage, fraction: f64) -> Control;
}

/// An observer that never cancels.
struct NoObserver;

impl SimObserver for NoObserver {
    fn progress(&mut self, _stage: Stage, _fraction: f64) -> Control {
        Control::Continue
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SimError {
    #[error("parameters use schema {found}, this build understands {expected}")]
    SchemaVersion { found: String, expected: String },
    #[error("the {0:?} tier is not available: {1}")]
    TierUnavailable(Fidelity, &'static str),
    #[error("unsupported by the {0:?} tier: {1}")]
    Unsupported(Fidelity, String),
    #[error("invalid parameters: {0}")]
    InvalidParams(String),
    #[error("the run was cancelled")]
    Cancelled,
    #[cfg(feature = "classic")]
    #[error(transparent)]
    Classic(#[from] sg_accrete::AccretionError),
}

/// Runs a simulation.
pub fn simulate(params: &SimulationParams) -> Result<SimulationResult, SimError> {
    simulate_with(params, &mut NoObserver)
}

/// Runs a simulation, reporting progress to `observer`, which may cancel it.
pub fn simulate_with(
    params: &SimulationParams,
    observer: &mut dyn SimObserver,
) -> Result<SimulationResult, SimError> {
    if params.schema_version != SCHEMA_VERSION {
        return Err(SimError::SchemaVersion {
            found: params.schema_version.clone(),
            expected: SCHEMA_VERSION.to_string(),
        });
    }
    let mut result = match params.fidelity {
        #[cfg(feature = "classic")]
        Fidelity::Classic => classic::simulate(params, observer),
        #[cfg(not(feature = "classic"))]
        Fidelity::Classic => Err(SimError::TierUnavailable(
            Fidelity::Classic,
            "this build was compiled without the `classic` feature",
        )),
        Fidelity::Standard => standard::simulate(params, observer),
        // The High tier is the Standard tier with its late stage
        // integrated (`high.rs`).
        Fidelity::High => standard::simulate(params, observer),
    }?;
    names::assign(&mut result.snapshot, params.seed);
    assign_minerals(&mut result.snapshot);
    if params.fidelity != Fidelity::Classic {
        comets::assign(&mut result.snapshot, params.seed);
    }
    Ok(result)
}

/// Estimates every body's surface from what the model gave it. Nothing
/// reads the result back, so no draw or physics depends on it.
fn assign_minerals(snapshot: &mut SystemSnapshot) {
    for body in &mut snapshot.bodies {
        let c = &body.composition;
        body.minerals = sg_planet::minerals::surface_mix(
            c.metal_frac,
            c.rock_frac,
            c.water_frac,
            c.h_he_frac,
            body.equilibrium_temp_k,
        );
    }
}

/// Fingerprint of a parameter set: FNV-1a of its canonical JSON.
pub fn params_hash(params: &SimulationParams) -> String {
    hash_json(params)
}

/// Fingerprint of a snapshot: FNV-1a of its canonical JSON. Equal on every
/// platform for equal parameters and model version; this is what the
/// determinism checks compare.
pub fn snapshot_hash(snapshot: &SystemSnapshot) -> String {
    hash_json(snapshot)
}

/// Fingerprint of a whole result (snapshot and timeline).
pub fn result_hash(result: &SimulationResult) -> String {
    hash_json(&(&result.snapshot, &result.timeline))
}

fn hash_json<T: serde::Serialize>(value: &T) -> String {
    // Serialising these plain data types cannot fail: no maps with
    // non-string keys, no fallible `Serialize` impls.
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    fnv1a64_hex(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_schema_version_is_rejected() {
        let params = SimulationParams {
            schema_version: "9.9.9".into(),
            ..SimulationParams::default()
        };
        assert!(matches!(
            simulate(&params),
            Err(SimError::SchemaVersion { .. })
        ));
    }

    #[test]
    fn the_high_tier_runs_and_says_which_tier_it_was() {
        let mut params = SimulationParams {
            fidelity: Fidelity::High,
            seed: 3,
            ..SimulationParams::default()
        };
        // A short integration: this is a smoke test, not a result.
        params.formation.high.integration_myr = 0.001;
        params.formation.high.max_steps = 200_000;
        let result = simulate(&params).expect("high tier");
        assert_eq!(result.diagnostics.fidelity, Fidelity::High);
        assert_eq!(result.snapshot.model_version, MODEL_VERSION);
        assert_eq!(simulate(&params).expect("again").snapshot, result.snapshot);
    }

    #[cfg(not(feature = "classic"))]
    #[test]
    fn classic_is_unavailable_without_the_feature() {
        assert!(matches!(
            simulate(&SimulationParams::default()),
            Err(SimError::TierUnavailable(Fidelity::Classic, _))
        ));
    }

    #[test]
    fn params_hash_tracks_content() {
        let a = SimulationParams::default();
        let b = SimulationParams {
            seed: 2,
            ..SimulationParams::default()
        };
        assert_eq!(params_hash(&a), params_hash(&a.clone()));
        assert_ne!(params_hash(&a), params_hash(&b));
    }
}
