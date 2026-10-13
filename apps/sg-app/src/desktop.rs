//! Desktop only: command-line options and screenshot mode.

use crate::launch::Launch;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::window::WindowOccluded;
use clap::{Parser, ValueEnum};
use sg_render::ScaleMode;
use sg_schema::{Fidelity, PresetId};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "sg-app",
    version,
    about = "Stardust planetary system simulator"
)]
struct Args {
    /// Random seed (non-zero).
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// Starting preset.
    #[arg(long, value_enum, default_value_t = PresetArg::SunLike)]
    preset: PresetArg,
    /// Formation model: the modern pipeline, or the Classic (StarGen) one.
    #[arg(long, value_enum, default_value_t = TierArg::Standard)]
    tier: TierArg,
    /// Scale mode to start in.
    #[arg(long, value_enum, default_value_t = ScaleArg::Enlarged)]
    scale: ScaleArg,
    /// Start at this fraction of the formation timeline (0 = empty disk,
    /// 1 = finished system) instead of playing it.
    #[arg(long)]
    time: Option<f64>,
    /// Select the Nth planet outward (1-based), or 0 for the star.
    #[arg(long)]
    select: Option<usize>,
    /// Render a few frames, save a screenshot here, and exit. For checking
    /// the visuals without a person at the keyboard.
    #[arg(long)]
    screenshot: Option<PathBuf>,
    /// Hide the UI panels (useful with --screenshot).
    #[arg(long)]
    no_ui: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum PresetArg {
    SunLike,
    RedDwarf,
    HotJupiter,
    MetalPoor,
    MassiveAStar,
    Circumbinary,
    STypeBinary,
    HierarchicalTriple,
    Random,
}

#[derive(Clone, Copy, ValueEnum)]
enum TierArg {
    Standard,
    Classic,
}

#[derive(Clone, Copy, ValueEnum)]
enum ScaleArg {
    True,
    Enlarged,
    Schematic,
}

/// Screenshot mode. The capture waits for the window to have been visible
/// for both a number of frames and an amount of time: shaders compile in the
/// background, and a window hidden behind others is not drawn at all, so a
/// frame count alone can pass with nothing on screen.
#[derive(Resource, Default)]
struct Capture {
    path: PathBuf,
    occluded: bool,
    /// Frames drawn, and the time, since the window was last uncovered.
    visible_frames: u32,
    visible_since_secs: f32,
    /// Seconds since startup at which the screenshot was requested.
    captured_at_secs: Option<f32>,
}

const CAPTURE_AFTER_FRAMES: u32 = 90;
const CAPTURE_AFTER_SECS: f32 = 2.5;
/// Time allowed for the screenshot to be written before exiting.
const EXIT_AFTER_CAPTURE_SECS: f32 = 1.0;
/// Give up if the window stays hidden this long.
const CAPTURE_TIMEOUT_SECS: f32 = 30.0;

fn capture(
    mut commands: Commands,
    mut capture: ResMut<Capture>,
    mut occlusion: MessageReader<WindowOccluded>,
    time: Res<Time<Real>>,
    mut exit: MessageWriter<AppExit>,
) {
    let now = time.elapsed_secs();
    for change in occlusion.read() {
        capture.occluded = change.occluded;
        capture.visible_frames = 0;
        capture.visible_since_secs = now;
    }
    if let Some(at) = capture.captured_at_secs {
        if now >= at + EXIT_AFTER_CAPTURE_SECS {
            exit.write(AppExit::Success);
        }
        return;
    }
    if capture.occluded {
        if now >= CAPTURE_TIMEOUT_SECS {
            error!("no screenshot: the window stayed hidden behind other windows");
            exit.write(AppExit::error());
        }
        return;
    }
    capture.visible_frames += 1;
    if capture.visible_frames >= CAPTURE_AFTER_FRAMES
        && now >= capture.visible_since_secs + CAPTURE_AFTER_SECS
    {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(capture.path.clone()));
        capture.captured_at_secs = Some(now);
    }
}

/// Reads the command line.
pub fn launch() -> Launch {
    let args = Args::parse();
    Launch {
        seed: args.seed,
        preset: match args.preset {
            PresetArg::SunLike => PresetId::SunLike,
            PresetArg::RedDwarf => PresetId::RedDwarf,
            PresetArg::HotJupiter => PresetId::HotJupiter,
            PresetArg::MetalPoor => PresetId::MetalPoor,
            PresetArg::MassiveAStar => PresetId::MassiveAStar,
            PresetArg::Circumbinary => PresetId::Circumbinary,
            PresetArg::STypeBinary => PresetId::STypeBinary,
            PresetArg::HierarchicalTriple => PresetId::HierarchicalTriple,
            PresetArg::Random => PresetId::Random,
        },
        fidelity: match args.tier {
            TierArg::Standard => Fidelity::Standard,
            TierArg::Classic => Fidelity::Classic,
        },
        scale: match args.scale {
            ScaleArg::True => ScaleMode::True,
            ScaleArg::Enlarged => ScaleMode::Exaggerated,
            ScaleArg::Schematic => ScaleMode::Schematic,
        },
        time: args.time,
        select: args.select,
        screenshot: args.screenshot,
        show_ui: !args.no_ui,
    }
}

/// Adds screenshot mode to the app.
pub fn add_capture(app: &mut App, path: PathBuf) {
    app.insert_resource(Capture { path, ..default() })
        .add_systems(Update, capture);
}
