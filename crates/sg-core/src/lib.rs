//! Foundations shared by every Stardust simulation crate.
//!
//! - [`math`]: transcendental functions routed through `libm`, so native and
//!   WASM builds produce identical bits.
//! - [`rng`]: ChaCha streams derived from `(seed, module id)`.
//! - [`units`]: newtypes that keep physical units out of bare `f64`s.
//! - [`constants`]: physical and astronomical constants with sources.
//! - [`hash`]: a stable 64-bit hash for parameter and snapshot fingerprints.
//!
//! This crate depends on nothing project-specific (PLAN.md §6.2).

pub mod constants;
pub mod hash;
pub mod math;
pub mod rng;
pub mod units;
