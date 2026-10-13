//! Data contracts of Stardust (PLAN.md §7).
//!
//! Three documents, all `serde`-serialisable:
//!
//! - [`SimulationParams`]: everything needed to reproduce a run;
//! - [`SystemSnapshot`]: the finished system at its present age;
//! - [`EventTimeline`]: the formation history, for playback.
//!
//! Rules:
//!
//! - Field names carry their unit (`a_au`, `mass_mearth`, `temp_k`).
//! - Every number is finite. A quantity that does not apply (the surface
//!   pressure of a gas giant) is `None`, never infinity or NaN, so that JSON
//!   round-trips exactly.
//! - Any change to these types bumps [`SCHEMA_VERSION`] and PLAN.md §7.
//!
//! This is schema **0.2**: it covers what the Classic tier produces. Types
//! for disks, belts, rings, multiple stars and the modern planet classes are
//! present so consumers can be written against them, and are filled in by
//! later milestones.

mod params;
mod snapshot;
mod timeline;

pub use params::*;
pub use snapshot::*;
pub use timeline::*;

/// Version of the data contracts, semver. Bump on any change to this crate's
/// types.
pub const SCHEMA_VERSION: &str = "0.8.0";
