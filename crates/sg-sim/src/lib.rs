//! The public simulation API (PLAN.md §6.3).
//!
//! [`simulate`] turns [`SimulationParams`] into a [`SimulationResult`]: the
//! finished system, its formation timeline, and diagnostics. The same
//! parameters and [`MODEL_VERSION`] give the same result on every platform.
//!
//! Tiers available so far: **Classic**, when the `classic` feature is on
//! (the default). Standard and High return [`SimError::TierUnavailable`]
//! until M4 and M7.

#[cfg(feature = "classic")]
mod classic;
pub mod names;
mod presets;

pub use presets::{preset_availability, preset_params, PresetAvailability, ALL_PRESETS};

use sg_core::hash::fnv1a64_hex;
use sg_schema::{EventTimeline, Fidelity, SimulationParams, SystemSnapshot, SCHEMA_VERSION};

/// Version of the physical model. Bump whenever a change alters outputs for
/// existing parameters (CLAUDE.md hard rule 7).
pub const MODEL_VERSION: &str = "0.1.0";

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
    Classic(#[from] sg_classic::ClassicError),
}

/// Runs a simulation.
pub fn simulate(params: &SimulationParams) -> Result<SimulationResult, SimError> {
    simulate_with(params, &mut NoObserver)
}

/// Runs a simulation, reporting progress to `observer`, which may cancel it.
#[cfg_attr(not(feature = "classic"), allow(unused_variables))]
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
    match params.fidelity {
        #[cfg(feature = "classic")]
        Fidelity::Classic => classic::simulate(params, observer),
        #[cfg(not(feature = "classic"))]
        Fidelity::Classic => Err(SimError::TierUnavailable(
            Fidelity::Classic,
            "this build was compiled without the `classic` feature",
        )),
        Fidelity::Standard => Err(SimError::TierUnavailable(
            Fidelity::Standard,
            "planned for milestone M4",
        )),
        Fidelity::High => Err(SimError::TierUnavailable(
            Fidelity::High,
            "planned for milestone M7",
        )),
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
    fn later_tiers_report_their_milestone() {
        for fidelity in [Fidelity::Standard, Fidelity::High] {
            let params = SimulationParams {
                fidelity,
                ..SimulationParams::default()
            };
            assert!(matches!(
                simulate(&params),
                Err(SimError::TierUnavailable(f, _)) if f == fidelity
            ));
        }
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
