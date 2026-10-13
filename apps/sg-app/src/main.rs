//! Stardust application (PLAN.md §9).
//!
//! Pick a preset or adjust parameters, press Simulate, watch the system
//! form, then explore it and click bodies for their data cards. The same
//! code runs on the desktop and, compiled to WebAssembly, in a browser
//! (`scripts/build-web.sh`).
//!
//! ```sh
//! cargo run -p sg-app
//! cargo run -p sg-app -- --seed 42 --preset sun-like
//! cargo run -p sg-app -- --seed 42 --screenshot shot.png --time 0.4
//! ```

mod cards;
#[cfg(not(target_arch = "wasm32"))]
mod desktop;
mod launch;
mod state;
mod ui;

use bevy::prelude::*;
use bevy::window::{WindowLevel, WindowResolution};
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use launch::Launch;
use sg_render::{StardustRenderPlugin, Target};
use sg_schema::BodyId;
use state::AppState;

/// The `<canvas>` the web build draws into (`web/index.html`).
const WEB_CANVAS: &str = "#stardust-canvas";

#[cfg(not(target_arch = "wasm32"))]
fn launch_options() -> Launch {
    desktop::launch()
}

#[cfg(target_arch = "wasm32")]
fn launch_options() -> Launch {
    let query = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .unwrap_or_default();
    Launch::from_query(&query)
}

fn main() {
    let launch = launch_options();

    let mut state = AppState::new(launch.preset, launch.seed);
    state.show_ui = launch.show_ui;
    state.start = state::StartOptions {
        scale: launch.scale,
        time_fraction: launch.time,
        // Resolved to a body once the first system exists.
        select: launch.select,
        frozen: launch.screenshot.is_some(),
    };

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Stardust".into(),
            resolution: WindowResolution::new(1440, 900),
            // A screenshot needs the window drawn, and a covered window is
            // not: keep it in front for the few seconds it exists.
            window_level: if launch.screenshot.is_some() {
                WindowLevel::AlwaysOnTop
            } else {
                WindowLevel::Normal
            },
            // Web only; ignored elsewhere.
            canvas: Some(WEB_CANVAS.into()),
            fit_canvas_to_parent: true,
            ..default()
        }),
        ..default()
    }))
    .add_plugins(EguiPlugin::default())
    .add_plugins(StardustRenderPlugin)
    .insert_resource(state)
    .add_systems(Startup, state::simulate_on_startup)
    .add_systems(EguiPrimaryContextPass, ui::draw);

    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = launch.screenshot {
        desktop::add_capture(&mut app, path);
    }

    app.run();
}

/// Resolves `--select N` against a finished system: the Nth planet in the
/// snapshot's order, or for 0 the first star.
pub(crate) fn nth_target(result: &sg_sim::SimulationResult, n: usize) -> Option<Target> {
    if n == 0 {
        let stars = sg_render::layout::stars(&result.snapshot.hierarchy);
        return stars.first().map(|star| Target::Star(star.id));
    }
    let id: BodyId = result.snapshot.bodies.get(n - 1)?.id;
    Some(Target::Body(id))
}
