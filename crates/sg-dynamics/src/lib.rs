//! Late-stage dynamics: stability checks, mergers and N-body integration
//! (PLAN.md §5.7).
//!
//! So far: [`kepler`], two-body orbit geometry, which the renderer needs to
//! place bodies on their orbits, and [`stability`], the analytic limits for
//! hierarchical stars and for planets in binaries. Planet–planet stability
//! is in [`late`]; [`nbody`] integrates orbits directly (M7). This is the only simulation crate
//! allowed to use `wgpu`, headless, behind a `gpu` feature.

pub mod kepler;
pub mod late;
pub mod nbody;
pub mod stability;
