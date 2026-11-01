//! How the app starts: which system to show first and how.
//!
//! On the desktop this comes from the command line (`desktop.rs`); in the
//! browser from the page's query string, e.g.
//! `?seed=7&preset=red-dwarf&tier=classic&scale=schematic`, so a link can
//! point at a particular system.

use crate::i18n::Language;
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
    /// Select the Nth moon, from 1.
    pub moon: Option<usize>,
    /// Look at the Nth belt, from 1.
    pub belt: Option<usize>,
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
    /// Start the spaceship from the selected body.
    pub tour: bool,
    /// Show the frame rate.
    pub fps: bool,
    /// Start with the Help window open.
    pub help: bool,
    /// The interface language.
    pub language: Language,
    /// How much closer than usual to frame the selected body. Above about
    /// 7 the camera is inside it.
    pub zoom: f32,
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
            moon: None,
            belt: None,
            screenshot: None,
            offscreen: false,
            bench: None,
            show_ui: true,
            quality: None,
            size: None,
            drawer: false,
            tour: false,
            fps: false,
            help: false,
            language: Language::En,
            zoom: 1.0,
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
                "moon" => launch.moon = value.parse().ok(),
                "belt" => launch.belt = value.parse().ok(),
                "ui" => launch.show_ui = value != "0",
                "fps" => launch.fps = value != "0",
                "tour" => launch.tour = value != "0",
                "zoom" => {
                    if let Some(zoom) = value
                        .parse()
                        .ok()
                        .filter(|z: &f32| *z >= 1.0 && z.is_finite())
                    {
                        launch.zoom = zoom;
                    }
                }
                "help" => launch.help = value != "0",
                "lang" => {
                    if let Some(language) = Language::from_code(value) {
                        launch.language = language;
                    }
                }
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
            "?seed=7&preset=red-dwarf&tier=classic&scale=schematic&select=3&moon=2&belt=1&time=0.5&ui=0\
             &quality=reduced&fps=1&help=1&lang=fr",
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
                moon: Some(2),
                belt: Some(1),
                screenshot: None,
                offscreen: false,
                bench: None,
                show_ui: false,
                quality: Some(Quality::Reduced),
                size: None,
                drawer: false,
                tour: false,
                fps: true,
                help: true,
                language: Language::Fr,
                zoom: 1.0,
            }
        );
    }

    #[test]
    fn bad_values_are_ignored() {
        let launch = Launch::from_query(
            "seed=0&preset=quasar&tier=best&scale=&time=NaN&select=-1&other=1&novalue&quality=ultra&lang=xx",
        );
        assert_eq!(launch, Launch::default());
        assert_eq!(Launch::from_query("seed=abc&seed=9").seed, 9);
    }
}
