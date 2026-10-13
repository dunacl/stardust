//! Desktop only: command-line options and screenshot mode.

use crate::launch::Launch;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
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
    /// With --screenshot: draw the scene into an image instead of the
    /// window, so it works with the window covered, the display asleep or
    /// the screen locked. The panels are left out; the name labels are drawn.
    #[arg(long)]
    offscreen: bool,
    /// Measure the frame rate for this many seconds and exit: the scene is
    /// drawn off screen at 2560×1440 with no frame-rate cap, orbits running.
    #[arg(long)]
    bench: Option<f32>,
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
    /// Draw into an image rather than the window.
    offscreen: bool,
    /// That image, once the camera has been pointed at it.
    target: Option<Handle<Image>>,
    /// Measure the frame rate for this long instead of saving an image.
    bench_secs: Option<f32>,
    /// Frames drawn and the time when the measurement began.
    bench_from: Option<(u32, f32)>,
    frames: u32,
}

/// Size of an off-screen screenshot, pixels.
const OFFSCREEN_SIZE: (u32, u32) = (1440, 900);
/// Size of the image the frame rate is measured at: the desktop budget of
/// PLAN.md §8.6 is set at 1440p.
const BENCH_SIZE: (u32, u32) = (2560, 1440);
/// Seconds before the measurement starts: shaders compile in the first few.
const BENCH_WARM_UP_SECS: f32 = 4.0;

/// Points the scene camera at an image, once it exists.
fn redirect_camera(
    mut commands: Commands,
    mut capture: ResMut<Capture>,
    mut images: ResMut<Assets<Image>>,
    camera: Query<Entity, With<Camera3d>>,
) {
    if !capture.offscreen || capture.target.is_some() {
        return;
    }
    let Ok(camera) = camera.single() else {
        return;
    };
    let size = if capture.bench_secs.is_some() {
        BENCH_SIZE
    } else {
        OFFSCREEN_SIZE
    };
    let image = images.add(Image::new_target_texture(
        size.0,
        size.1,
        TextureFormat::Rgba8UnormSrgb,
        None,
    ));
    commands
        .entity(camera)
        .insert(RenderTarget::Image(image.clone().into()));
    capture.target = Some(image);
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
    if capture.offscreen && capture.target.is_none() {
        return;
    }
    if let Some(secs) = capture.bench_secs {
        capture.frames += 1;
        match capture.bench_from {
            None if now >= BENCH_WARM_UP_SECS => capture.bench_from = Some((capture.frames, now)),
            Some((first, since)) if now >= since + secs => {
                let frames = capture.frames - first;
                let elapsed = now - since;
                println!(
                    "bench: {frames} frames in {elapsed:.1} s at {}x{}: {:.2} ms per frame, {:.0} fps",
                    BENCH_SIZE.0,
                    BENCH_SIZE.1,
                    1000.0 * elapsed / frames as f32,
                    frames as f32 / elapsed
                );
                exit.write(AppExit::Success);
            }
            _ => {}
        }
        return;
    }
    if capture.occluded && !capture.offscreen {
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
        let shot = match &capture.target {
            Some(image) => Screenshot::image(image.clone()),
            None => Screenshot::primary_window(),
        };
        commands
            .spawn(shot)
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
        offscreen: args.offscreen || args.bench.is_some(),
        bench: args.bench,
        show_ui: !args.no_ui && !args.offscreen && args.bench.is_none(),
    }
}

/// Adds screenshot mode to the app.
pub fn add_capture(app: &mut App, path: PathBuf, offscreen: bool, bench_secs: Option<f32>) {
    app.insert_resource(Capture {
        path,
        offscreen,
        bench_secs,
        ..default()
    })
    .add_systems(Update, (redirect_camera, capture).chain());
}
