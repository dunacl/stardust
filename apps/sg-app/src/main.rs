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
use bevy::window::WindowResolution;
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

/// Screenshot mode: frames to wait before capturing, then before exiting.
#[derive(Resource)]
struct Capture {
    path: PathBuf,
    frame: u32,
}

const CAPTURE_AT_FRAME: u32 = 90;
const EXIT_AT_FRAME: u32 = 130;

fn capture(mut commands: Commands, mut capture: ResMut<Capture>, mut exit: MessageWriter<AppExit>) {
    capture.frame += 1;
    if capture.frame == CAPTURE_AT_FRAME {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(capture.path.clone()));
    }
    if capture.frame >= EXIT_AT_FRAME {
        exit.write(AppExit::Success);
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
        app.insert_resource(Capture { path, frame: 0 })
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
