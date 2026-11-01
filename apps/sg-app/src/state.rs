//! Application state and the "Simulate" action.

use bevy::prelude::*;
use sg_core::hash::fnv1a64;
use sg_render::scene::system_extent;
use sg_render::{
    OrbitCamera, Playback, ScaleBlend, ScaleMode, Selection, SystemView, ViewSettings, Voyage,
};
use sg_schema::{Fidelity, PresetId, SimulationParams};
use sg_sim::{preset_params, SimulationResult};

/// Seconds a full formation playback lasts at the default speed.
pub const DEFAULT_PLAYBACK_SECS: f64 = 12.0;

/// One-off options applied to the first system (from the command line).
#[derive(Debug, Clone, Default)]
pub struct StartOptions {
    pub scale: ScaleMode,
    pub time_fraction: Option<f64>,
    pub select: Option<usize>,
    /// Select the Nth moon, from 1.
    pub moon: Option<usize>,
    /// Select the Nth comet, from 1.
    pub comet: Option<usize>,
    /// Look at the Nth belt, from 1.
    pub belt: Option<usize>,
    /// Freeze the clocks (screenshot mode), so captures are repeatable.
    pub frozen: bool,
    pub quality: sg_render::Quality,
    /// Start the spaceship from the selected body.
    pub tour: bool,
    /// Desktop only: save the view as a `.glb` here and exit.
    pub export: Option<std::path::PathBuf>,
    /// Layers to start hidden, by name.
    pub hide: Vec<String>,
    /// Show the finished system with its star at this age, Gyr.
    pub star_age: Option<f64>,
    /// How much closer than usual to frame the selected body.
    pub zoom: f32,
}

#[derive(Resource)]
pub struct AppState {
    pub preset: PresetId,
    /// The tier the next Simulate uses.
    pub fidelity: Fidelity,
    /// The parameters being edited; used on the next Simulate.
    pub params: SimulationParams,
    /// Play the formation timeline after each Simulate.
    pub play_on_simulate: bool,
    /// Why the last Simulate failed, if it did.
    pub error: Option<String>,
    pub show_ui: bool,
    pub start: StartOptions,
    /// How long the last Simulate took, milliseconds.
    pub last_simulation_ms: Option<f64>,
    /// Whether the control panel is open where it is a drawer (narrow
    /// screens). Unset until the first frame shows how wide the screen is.
    pub panel_open: Option<bool>,
    /// Whether the control panel is shown on a wide screen.
    pub show_panel: bool,
    /// Show the frame rate in a corner.
    pub show_fps: bool,
    /// Where a save dialog leaves its result.
    #[cfg(not(target_arch = "wasm32"))]
    pub save_note: crate::desktop::SaveNote,
    /// What the last export did, for the panel.
    pub export_note: Option<String>,
    /// Save the view on the next frame.
    pub export_wanted: bool,
    /// The star's age to show, once enough time has passed since the last.
    pub age_wanted: Option<f64>,
    /// When the last age was applied, seconds.
    pub age_applied_at: f32,
    /// The age the simulation gave the system, Gyr.
    pub native_age_gyr: f64,
    /// The timeline is playing through the star's life.
    pub life_playing: bool,
    /// How far through it, 0 to 1.
    pub life_share: f64,
    /// Formation was playing a frame ago.
    pub was_forming: bool,
    /// Show the Layers window.
    pub show_layers: bool,
    /// Show the Help window.
    pub show_help: bool,
    /// What the Help window is searching for.
    pub help_search: String,
    pub frame_rate: FrameRate,
}

/// A running average of the frame time, for the frame-rate display.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FrameRate {
    /// Smoothed time per frame, seconds; 0 until the first frame.
    smoothed_secs: f32,
}

impl FrameRate {
    /// Time over which the average forgets, seconds: long enough for a
    /// steady number to read, short enough to follow a change.
    const MEMORY_SECS: f32 = 0.5;

    /// Takes in one frame that lasted `dt_secs`.
    pub fn record(&mut self, dt_secs: f32) {
        if !(dt_secs > 0.0 && dt_secs.is_finite()) {
            return;
        }
        if self.smoothed_secs <= 0.0 {
            self.smoothed_secs = dt_secs;
            return;
        }
        // An exponential average whose weight follows the frame's own
        // length, so the memory is the same at any frame rate.
        let weight = (dt_secs / Self::MEMORY_SECS).min(1.0);
        self.smoothed_secs += (dt_secs - self.smoothed_secs) * weight;
    }

    /// Frames per second, or `None` before any frame.
    pub fn fps(&self) -> Option<f32> {
        (self.smoothed_secs > 0.0).then(|| 1.0 / self.smoothed_secs)
    }

    /// Milliseconds per frame, or `None` before any frame.
    pub fn frame_ms(&self) -> Option<f32> {
        (self.smoothed_secs > 0.0).then_some(self.smoothed_secs * 1000.0)
    }
}

impl AppState {
    pub fn new(preset: PresetId, seed: u64, fidelity: Fidelity) -> Self {
        Self {
            preset,
            fidelity,
            params: preset_params(preset, seed, fidelity),
            play_on_simulate: true,
            error: None,
            show_ui: true,
            start: StartOptions::default(),
            last_simulation_ms: None,
            panel_open: None,
            show_panel: true,
            show_fps: false,
            age_wanted: None,
            age_applied_at: 0.0,
            native_age_gyr: 0.0,
            life_playing: false,
            life_share: 0.0,
            was_forming: false,
            show_layers: false,
            show_help: false,
            #[cfg(not(target_arch = "wasm32"))]
            save_note: Default::default(),
            export_note: None,
            export_wanted: false,
            help_search: String::new(),
            frame_rate: FrameRate::default(),
        }
    }

    /// Switches preset, keeping the seed.
    pub fn apply_preset(&mut self, preset: PresetId) {
        self.preset = preset;
        self.params = preset_params(preset, self.params.seed, self.fidelity);
    }

    /// Switches tier, keeping the seed and, where the tier has it, the
    /// preset.
    pub fn set_fidelity(&mut self, fidelity: Fidelity) {
        self.fidelity = fidelity;
        let available = sg_sim::preset_availability(self.preset, fidelity)
            == sg_sim::PresetAvailability::Available;
        self.apply_preset(if available {
            self.preset
        } else {
            PresetId::SunLike
        });
    }

    /// A different seed, derived from the current one. (UI convenience; the
    /// simulation itself only ever sees the resulting number.)
    pub fn next_seed(&mut self) {
        let next = fnv1a64(&self.params.seed.to_le_bytes()) % 1_000_000;
        self.params.seed = next.max(1);
    }
}

/// Everything the Simulate action touches.
#[derive(bevy::ecs::system::SystemParam)]
pub struct SimulationTargets<'w> {
    pub view: ResMut<'w, SystemView>,
    pub playback: ResMut<'w, Playback>,
    pub settings: ResMut<'w, ViewSettings>,
    pub camera: ResMut<'w, OrbitCamera>,
    pub selection: ResMut<'w, Selection>,
    pub voyage: ResMut<'w, Voyage>,
}

/// Shows the finished system with its star at this age. The view stays where
/// it is, and so does what is selected.
pub fn show_age(state: &mut AppState, targets: &mut SimulationTargets, age_gyr: f64) {
    let Some(result) = targets.view.result() else {
        return;
    };
    match sg_sim::life::at_age(&result.snapshot, age_gyr) {
        Ok(snapshot) => targets.view.set_snapshot(snapshot),
        Err(err) => state.error = Some(err.to_string()),
    }
}

/// Runs the simulation for the current parameters and shows the result.
pub fn run_simulation(state: &mut AppState, targets: &mut SimulationTargets) {
    let started = bevy::platform::time::Instant::now();
    let outcome = sg_sim::simulate(&state.params);
    state.last_simulation_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
    let result: SimulationResult = match outcome {
        Ok(result) => result,
        Err(err) => {
            state.error = Some(err.to_string());
            return;
        }
    };
    state.error = None;
    state.native_age_gyr = result.snapshot.age_gyr;
    state.life_playing = false;
    state.age_wanted = None;

    let end = sg_render::display::timeline_end_myr(&result);
    targets.playback.myr_per_second = end / DEFAULT_PLAYBACK_SECS;
    if state.play_on_simulate && end > 0.0 {
        targets.playback.t_myr = 0.0;
        targets.playback.playing = true;
    } else {
        targets.playback.t_myr = end;
        targets.playback.playing = false;
    }
    targets
        .camera
        .frame(system_extent(&result, &targets.settings.scale));
    targets.selection.0 = None;
    targets.voyage.stop();
    targets.view.set(result);
}

pub fn simulate_on_startup(mut state: ResMut<AppState>, mut targets: SimulationTargets) {
    let start = state.start.clone();
    targets.settings.scale = ScaleBlend::settled(start.scale);
    targets.settings.quality = start.quality;
    for layer in &start.hide {
        crate::ui::hide_layer(&mut targets.settings, layer);
    }
    run_simulation(&mut state, &mut targets);

    let end = targets.view.timeline_end_myr();
    if let Some(fraction) = start.time_fraction {
        targets.playback.t_myr = end * fraction.clamp(0.0, 1.0);
        targets.playback.playing = false;
    }
    // Asking for something in the finished system means starting there:
    // a comet does not exist yet while the system forms, and an age would
    // be overwritten when playback reached the star's life.
    let wants_finished = start.select.is_some()
        || start.moon.is_some()
        || start.comet.is_some()
        || start.belt.is_some()
        || start.star_age.is_some();
    if wants_finished && start.time_fraction.is_none() && !start.tour {
        targets.playback.t_myr = end;
        targets.playback.playing = false;
    }
    if start.frozen {
        targets.playback.playing = false;
        targets.playback.orbits_running = false;
        if start.time_fraction.is_none() {
            targets.playback.t_myr = end;
        }
    }
    if let Some(age) = start.star_age {
        show_age(&mut state, &mut targets, age);
    }
    targets.camera.snap();
    targets.camera.select_zoom = start.zoom;
    if let (Some(n), Some(result)) = (start.moon, targets.view.result()) {
        targets.selection.0 = crate::nth_moon(&result.snapshot, n);
    }
    if let (Some(n), Some(result)) = (start.comet, targets.view.result()) {
        targets.selection.0 = crate::nth_comet(&result.snapshot, n);
    }
    if let (Some(n), Some(result)) = (start.belt, targets.view.result()) {
        let view = n.checked_sub(1).and_then(|zone| {
            sg_render::belt::view_of(
                &result.snapshot,
                &targets.settings.scale,
                targets.playback.orbit_days,
                zone,
            )
        });
        if let Some((focus, distance)) = view {
            targets.camera.look_at(focus, distance / start.zoom);
        }
    }
    if let (Some(n), Some(result)) = (start.select, targets.view.result()) {
        if let Some(target) = crate::nth_target(result, n) {
            targets.selection.0 = Some(target);
            if start.tour {
                targets.voyage.start(&result.snapshot, Some(target));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_rate_settles_on_a_steady_rate_and_follows_a_change() {
        let mut rate = FrameRate::default();
        assert_eq!(rate.fps(), None);
        // Ignored: nonsense intervals.
        rate.record(0.0);
        rate.record(f32::NAN);
        assert_eq!(rate.fps(), None);
        for _ in 0..120 {
            rate.record(1.0 / 60.0);
        }
        assert!((rate.fps().unwrap() - 60.0).abs() < 0.01);
        assert!((rate.frame_ms().unwrap() - 16.667).abs() < 0.01);
        // Dropping to 30 fps shows within about a second...
        for _ in 0..30 {
            rate.record(1.0 / 30.0);
        }
        assert!((rate.fps().unwrap() - 30.0).abs() < 5.0);
        // ...and three seconds after one long hitch the reading is back.
        rate.record(0.4);
        for _ in 0..90 {
            rate.record(1.0 / 30.0);
        }
        assert!((rate.fps().unwrap() - 30.0).abs() < 3.0);
    }
}
