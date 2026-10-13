//! Late-stage dynamics: stability checks, mergers and N-body integration
//! (PLAN.md §5.7).
//!
//! So far this holds only [`kepler`], two-body orbit geometry, which the
//! renderer needs to place bodies on their orbits. Analytic stability
//! arrives in M4, the integrators in M7. This is the only simulation crate
//! allowed to use `wgpu`, headless, behind a `gpu` feature.

pub mod kepler;
