//! Stardust application (PLAN.md §9).
//!
//! Pick a preset or adjust parameters, press Simulate, watch the system
//! form, then explore it and click bodies for their data cards. The same
//! code runs on the desktop and, compiled to WebAssembly, in a browser
//! (`scripts/build-web.sh`). It is a library with one entry point, [`run`],
//! so that each platform's own start-up code can be a few lines.
//!
//! ```sh
//! cargo run -p sg-app
//! cargo run -p sg-app -- --seed 42 --preset sun-like
//! cargo run -p sg-app -- --seed 42 --screenshot shot.png --time 0.4
//! ```

mod cards;
#[cfg(not(target_arch = "wasm32"))]
mod desktop;
mod help;
mod i18n;
mod launch;
mod state;
mod strings;
mod ui;

use bevy::prelude::*;
use bevy::window::{WindowLevel, WindowResolution};
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use launch::Launch;
use sg_render::{Quality, StardustRenderPlugin, Target};
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

/// The effects level a platform starts at when none is asked for: reduced
/// on phones and in browsers without WebGPU, full elsewhere (PLAN.md §8.6).
fn default_quality() -> Quality {
    if cfg!(any(target_os = "android", target_os = "ios"))
        || cfg!(all(target_arch = "wasm32", not(feature = "webgpu")))
    {
        Quality::Reduced
    } else {
        Quality::Full
    }
}

/// Starts the app and runs it until it is closed.
pub fn run() {
    let launch = launch_options();
    i18n::set_language(launch.language);

    let mut state = AppState::new(launch.preset, launch.seed, launch.fidelity);
    state.show_ui = launch.show_ui;
    state.show_fps = launch.fps;
    state.show_help = launch.help;
    if launch.drawer {
        state.panel_open = Some(true);
    }
    state.start = state::StartOptions {
        scale: launch.scale,
        time_fraction: launch.time,
        // Resolved to a body once the first system exists.
        select: launch.select,
        moon: launch.moon,
        belt: launch.belt,
        frozen: launch.screenshot.is_some(),
        quality: launch.quality.unwrap_or_else(default_quality),
        tour: launch.tour,
        zoom: launch.zoom,
    };
    let (width, height) = launch.size.unwrap_or((1440, 900));

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Stardust".into(),
            resolution: WindowResolution::new(width, height),
            // A screenshot needs the window drawn, and a covered window is
            // not: keep it in front for the few seconds it exists.
            window_level: if launch.screenshot.is_some() {
                WindowLevel::AlwaysOnTop
            } else {
                WindowLevel::Normal
            },
            // Web only; ignored elsewhere.
            // Measuring the frame rate: do not wait for the display.
            present_mode: if launch.bench.is_some() {
                bevy::window::PresentMode::AutoNoVsync
            } else {
                bevy::window::PresentMode::AutoVsync
            },
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
    if launch.screenshot.is_some() || launch.bench.is_some() {
        desktop::add_capture(
            &mut app,
            launch.screenshot.unwrap_or_default(),
            launch.offscreen,
            launch.bench,
            launch.size,
        );
    }

    app.run();
}

/// Resolves `--moon N`: the Nth moon in the snapshot's order, from 1.
pub(crate) fn nth_moon(snapshot: &sg_schema::SystemSnapshot, n: usize) -> Option<Target> {
    let moon = snapshot
        .bodies
        .iter()
        .filter(|body| matches!(body.host, sg_schema::HostRef::Body(_)))
        .nth(n.checked_sub(1)?)?;
    Some(Target::Body(moon.id))
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

#[cfg(test)]
mod tests {
    use super::*;
    use sg_schema::{HostRef, SimulationParams};

    #[test]
    fn moons_are_counted_from_one_among_moons_only() {
        let result = (1..50)
            .map(|seed| {
                sg_sim::simulate(&SimulationParams {
                    seed,
                    ..SimulationParams::default()
                })
                .unwrap()
            })
            .find(|result| {
                result
                    .snapshot
                    .bodies
                    .iter()
                    .any(|b| matches!(b.host, HostRef::Body(_)))
            })
            .expect("a system with a moon");
        let snapshot = &result.snapshot;
        let Some(Target::Body(id)) = nth_moon(snapshot, 1) else {
            panic!("no first moon");
        };
        let moon = snapshot.bodies.iter().find(|b| b.id == id).unwrap();
        assert!(matches!(moon.host, HostRef::Body(_)));
        assert_eq!(nth_moon(snapshot, 0), None);
        assert_eq!(nth_moon(snapshot, 9999), None);
    }
}
