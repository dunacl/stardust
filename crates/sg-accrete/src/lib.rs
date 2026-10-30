//! Classic fidelity tier: the accretion model of Dole (1969, RAND P-4226; also
//! *Icarus* 13, 494, 1970) with Fogg's (1985, *JBIS* 38, 501) extension to other
//! stars.
//!
//! Nuclei are injected one at a time into a cloud of dust and gas. Each sweeps
//! the dust within its reach and, above a critical mass, gas as well; planets
//! that come close coalesce; the run ends when no dust remains between the
//! bounds. The model has no time axis, no migration and no dynamics. It is
//! kept as a historical method, not as a description of how planets form.
//!
//! Written from `docs/accretion-model-spec.md`, whose references (`D`, `F`) and
//! gap numbers are used in the comments here. Validity: Fogg's range is single
//! stars of 0.6–1.3 M☉; beyond it the scaling is an extrapolation (gap 5).

pub mod bands;
pub mod model;
pub mod run;

pub use model::{critical_mass_msun, dust_density_msun_au3, AccretionParams, Reach};
pub use run::{accrete, Accretion, Event, Planet, RunSpec};

/// Why a run could not be made.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AccretionError {
    #[error("stellar mass and luminosity must be positive and finite")]
    BadStar,
    #[error("bounds must satisfy 0 < inner < outer, finite, within the material limits")]
    BadBounds,
}
