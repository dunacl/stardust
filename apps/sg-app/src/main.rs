//! StarGen-NG application (PLAN.md §9).
//!
//! M2: pick a preset or adjust parameters, press Simulate, watch the system
//! form, then explore it and click bodies for their data cards.
//!
//! ```sh
//! cargo run -p sg-app
//! cargo run -p sg-app -- --seed 42 --preset sun-like
//! cargo run -p sg-app -- --seed 42 --screenshot shot.png --time 0.4
//! ```

mod cards;
mod state;
mod ui;

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::window::{WindowLevel, WindowOccluded, WindowResolution};
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use clap::{Parser, ValueEnum};
use sg_render::{ScaleMode, StarGenRenderPlugin, Target};
use sg_schema::{BodyId, PresetId};
use state::AppState;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "sg-app",
    version,
    about = "StarGen-NG planetary system simulator"
)]
struct Args {
    /// Random seed (non-zero).
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// Starting preset.
    #[arg(long, value_enum, default_value_t = PresetArg::SunLike)]
    preset: PresetArg,
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
    Random,
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

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let preset = match args.preset {
        PresetArg::SunLike => PresetId::SunLike,
        PresetArg::RedDwarf => PresetId::RedDwarf,
        PresetArg::Random => PresetId::Random,
    };
    let scale = match args.scale {
        ScaleArg::True => ScaleMode::True,
        ScaleArg::Enlarged => ScaleMode::Exaggerated,
        ScaleArg::Schematic => ScaleMode::Schematic,
    };

    let mut state = AppState::new(preset, args.seed);
    state.show_ui = !args.no_ui;
    state.start = state::StartOptions {
        scale,
        time_fraction: args.time,
        // Resolved to a body once the first system exists.
        select: args.select,
        frozen: args.screenshot.is_some(),
    };

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "StarGen-NG".into(),
            resolution: WindowResolution::new(1440, 900),
            // A screenshot needs the window drawn, and a covered window is
            // not: keep it in front for the few seconds it exists.
            window_level: if args.screenshot.is_some() {
                WindowLevel::AlwaysOnTop
            } else {
                WindowLevel::Normal
            },
            ..default()
        }),
        ..default()
    }))
    .add_plugins(EguiPlugin::default())
    .add_plugins(StarGenRenderPlugin)
    .insert_resource(state)
    .add_systems(Startup, state::simulate_on_startup)
    .add_systems(EguiPrimaryContextPass, ui::draw);

    if let Some(path) = args.screenshot {
        app.insert_resource(Capture { path, ..default() })
            .add_systems(Update, capture);
    }

    app.run();
    Ok(())
}

/// Resolves `--select N` against a finished system.
pub(crate) fn nth_target(result: &sg_sim::SimulationResult, n: usize) -> Option<Target> {
    if n == 0 {
        return Some(Target::Star);
    }
    let id: BodyId = result.snapshot.bodies.get(n - 1)?.id;
    Some(Target::Body(id))
}
