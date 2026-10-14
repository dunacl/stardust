//! How the app starts: which system to show first and how.
//!
//! On the desktop this comes from the command line (`desktop.rs`); in the
//! browser from the page's query string, e.g.
//! `?seed=7&preset=red-dwarf&tier=classic&scale=schematic`, so a link can
//! point at a particular system.

use sg_render::{Quality, ScaleMode};
use sg_schema::{Fidelity, PresetId};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct Launch {
    pub seed: u64,
    pub preset: PresetId,
    pub fidelity: Fidelity,
    pub scale: ScaleMode,
    /// Start at this fraction of the formation timeline instead of playing it.
    pub time: Option<f64>,
    /// Select the Nth planet outward (1-based), or 0 for the star.
    pub select: Option<usize>,
    /// Desktop only: save a screenshot here and exit.
    pub screenshot: Option<PathBuf>,
    /// Desktop only: draw that screenshot off screen.
    pub offscreen: bool,
    /// Desktop only: measure the frame rate for this many seconds and exit.
    pub bench: Option<f32>,
    pub show_ui: bool,
    /// Effects level; `None` leaves it to the platform's default.
    pub quality: Option<Quality>,
    /// Desktop only: window (or off-screen image) size in pixels.
    pub size: Option<(u32, u32)>,
    /// Start with the controls drawer open on a narrow screen.
    pub drawer: bool,
}

impl Default for Launch {
    fn default() -> Self {
        Self {
            seed: 42,
            preset: PresetId::SunLike,
            fidelity: Fidelity::Standard,
            scale: ScaleMode::Exaggerated,
            time: None,
            select: None,
            screenshot: None,
            offscreen: false,
            bench: None,
            show_ui: true,
            quality: None,
            size: None,
            drawer: false,
        }
    }
}

impl Launch {
    /// Reads a URL query string (with or without the leading `?`). Unknown
    /// keys and values that do not parse are ignored: a mistyped link still
    /// opens the app.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn from_query(query: &str) -> Self {
        let mut launch = Self::default();
        for pair in query.trim_start_matches('?').split('&') {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            match key {
                "seed" => {
                    if let Some(seed) = value.parse().ok().filter(|s| *s != 0) {
                        launch.seed = seed;
                    }
                }
                "preset" => match value {
                    "sun-like" => launch.preset = PresetId::SunLike,
                    "red-dwarf" => launch.preset = PresetId::RedDwarf,
                    "hot-jupiter" => launch.preset = PresetId::HotJupiter,
                    "metal-poor" => launch.preset = PresetId::MetalPoor,
                    "massive-a-star" => launch.preset = PresetId::MassiveAStar,
                    "circumbinary" => launch.preset = PresetId::Circumbinary,
                    "s-type-binary" => launch.preset = PresetId::STypeBinary,
                    "hierarchical-triple" => launch.preset = PresetId::HierarchicalTriple,
                    "random" => launch.preset = PresetId::Random,
                    _ => {}
                },
                "tier" => match value {
                    "classic" => launch.fidelity = Fidelity::Classic,
                    "standard" => launch.fidelity = Fidelity::Standard,
                    "high" => launch.fidelity = Fidelity::High,
                    _ => {}
                },
                "scale" => match value {
                    "true" => launch.scale = ScaleMode::True,
                    "enlarged" => launch.scale = ScaleMode::Exaggerated,
                    "schematic" => launch.scale = ScaleMode::Schematic,
                    _ => {}
                },
                "time" => launch.time = value.parse().ok().filter(|t: &f64| t.is_finite()),
                "select" => launch.select = value.parse().ok(),
                "ui" => launch.show_ui = value != "0",
                "quality" => match value {
                    "full" => launch.quality = Some(Quality::Full),
                    "reduced" => launch.quality = Some(Quality::Reduced),
                    _ => {}
                },
                _ => {}
            }
        }
        launch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_gives_the_defaults() {
        assert_eq!(Launch::from_query(""), Launch::default());
        assert_eq!(Launch::from_query("?"), Launch::default());
    }

    #[test]
    fn query_sets_the_starting_system() {
        let launch = Launch::from_query(
            "?seed=7&preset=red-dwarf&tier=classic&scale=schematic&select=3&time=0.5&ui=0\
             &quality=reduced",
        );
        assert_eq!(
            launch,
            Launch {
                seed: 7,
                preset: PresetId::RedDwarf,
                fidelity: Fidelity::Classic,
                scale: ScaleMode::Schematic,
                time: Some(0.5),
                select: Some(3),
                screenshot: None,
                offscreen: false,
                bench: None,
                show_ui: false,
                quality: Some(Quality::Reduced),
                size: None,
                drawer: false,
            }
        );
    }

    #[test]
    fn bad_values_are_ignored() {
        let launch = Launch::from_query(
            "seed=0&preset=quasar&tier=best&scale=&time=NaN&select=-1&other=1&novalue&quality=ultra",
        );
        assert_eq!(launch, Launch::default());
        assert_eq!(Launch::from_query("seed=abc&seed=9").seed, 9);
    }
}
