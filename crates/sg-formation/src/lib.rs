//! Planet formation in a gas disk (PLAN.md §5.3–5.6): seeds grow by
//! accreting pebbles, stop at the pebble isolation mass, contract and then
//! run away in gas, and migrate through the disk as they do.
//!
//! [`physics`] holds the rates; [`evolve`] integrates a system of embryos
//! through the life of the disk.

pub mod evolve;
pub mod physics;
