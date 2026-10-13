//! StarGen-NG renderer (PLAN.md §8), as a Bevy plugin.
//!
//! M2 scope: the star (blackbody colour, bloom), planets as spheres coloured
//! by class, orbit lines, the habitable zone, three scale modes with
//! animated transitions, an orbit camera with click selection, and formation
//! playback driven by the timeline's keyframes and events.
//!
//! The renderer draws what the simulation produced (PLAN.md D12). Where it
//! has to choose something the simulation does not provide, the choice is
//! documented at the point it is made ([`display`], [`palette`]).
//!
//! The app owns the UI. It talks to the renderer through resources:
//! [`SystemView`] (what to show), [`Playback`], [`ViewSettings`],
//! [`Selection`], [`OrbitCamera`] and [`PointerCaptured`].

pub mod camera;
pub mod display;
pub mod palette;
pub mod scale;
pub mod scene;

pub use camera::{OrbitCamera, PointerCaptured};
pub use scale::{ScaleBlend, ScaleMode};
pub use scene::{
    BodyPlacements, Placement, Playback, Selection, StarGenRenderPlugin, SystemView, Target,
    ViewSettings,
};
