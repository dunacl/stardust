//! Formation history for playback (PLAN.md §7.4).

use crate::snapshot::{BodyId, DiskId, HostRef};
use serde::{Deserialize, Serialize};

/// The formation history of a system.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct EventTimeline {
    /// Sorted by `t_myr`.
    pub events: Vec<Event>,
    /// Periodic full states, for scrubbing. Sorted by `t_myr`.
    pub keyframes: Vec<Keyframe>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// Time since disk formation.
    pub t_myr: f64,
    pub kind: EventKind,
}

/// A volatile whose ice line is tracked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Species {
    H2o,
    Co2,
    Co,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoonOrigin {
    CircumplanetaryDisk,
    GiantImpact,
    Capture,
}

/// Something that happened during formation. The Classic tier emits
/// `DiskFormed`, `EmbryoInjected`, `DustSwept`, `RunawayGasAccretion`,
/// `Collision` and `DiskDispersed`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    DiskFormed {
        disk: DiskId,
    },
    IceLineMoved {
        disk: DiskId,
        species: Species,
        r_au: f64,
    },
    PlanetesimalsFormed {
        disk: DiskId,
        r_au: f64,
        mass_mearth: f64,
    },
    /// A new embryo (Classic: nucleus) appears.
    EmbryoInjected {
        body: BodyId,
        /// What it orbits.
        host: HostRef,
        a_au: f64,
        e: f64,
    },
    /// Classic: a body clears the dust between `inner_au` and `outer_au`,
    /// gaining `dm_mearth`.
    DustSwept {
        body: BodyId,
        dm_mearth: f64,
        inner_au: f64,
        outer_au: f64,
    },
    PebbleIsolationReached {
        body: BodyId,
    },
    RunawayGasAccretion {
        body: BodyId,
    },
    GapOpened {
        body: BodyId,
    },
    Migrated {
        body: BodyId,
        from_au: f64,
        to_au: f64,
    },
    ResonanceCaptured {
        a: BodyId,
        b: BodyId,
        ratio: (u8, u8),
    },
    /// Bodies `a` and `b` merge into `result` (which may be `a` or `b`).
    Collision {
        a: BodyId,
        b: BodyId,
        result: BodyId,
    },
    Ejected {
        body: BodyId,
    },
    /// A body fell into its star.
    FellIntoStar {
        body: BodyId,
    },
    MoonFormed {
        moon: BodyId,
        host: BodyId,
        mechanism: MoonOrigin,
    },
    DiskDispersed {
        disk: DiskId,
    },
    AtmosphereStripped {
        body: BodyId,
        fraction: f64,
    },
    TidallyLocked {
        body: BodyId,
    },
}

/// The state of every body at one instant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    pub t_myr: f64,
    /// Ordered by body id.
    pub bodies: Vec<KeyframeBody>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyframeBody {
    pub id: BodyId,
    /// What it orbits.
    pub host: HostRef,
    pub a_au: f64,
    pub e: f64,
    pub mass_mearth: f64,
}
