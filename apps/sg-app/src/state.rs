//! Application state and the "Simulate" action.

use bevy::prelude::*;
use sg_core::hash::fnv1a64;
use sg_render::scene::system_extent;
use sg_render::{
    OrbitCamera, Playback, ScaleBlend, ScaleMode, Selection, SystemView, ViewSettings,
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
    /// Freeze the clocks (screenshot mode), so captures are repeatable.
    pub frozen: bool,
    pub quality: sg_render::Quality,
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
    targets.view.set(result);
}

pub fn simulate_on_startup(mut state: ResMut<AppState>, mut targets: SimulationTargets) {
    let start = state.start.clone();
    targets.settings.scale = ScaleBlend::settled(start.scale);
    targets.settings.quality = start.quality;
    run_simulation(&mut state, &mut targets);

    let end = targets.view.timeline_end_myr();
    if let Some(fraction) = start.time_fraction {
        targets.playback.t_myr = end * fraction.clamp(0.0, 1.0);
        targets.playback.playing = false;
    }
    if start.frozen {
        targets.playback.playing = false;
        targets.playback.orbits_running = false;
        if start.time_fraction.is_none() {
            targets.playback.t_myr = end;
        }
    }
    targets.camera.snap();
    if let (Some(n), Some(result)) = (start.select, targets.view.result()) {
        if let Some(target) = crate::nth_target(result, n) {
            targets.selection.0 = Some(target);
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
