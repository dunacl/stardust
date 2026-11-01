//! Desktop only: command-line options and screenshot mode.

use crate::i18n::Language;
use crate::launch::Launch;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::time::TimeUpdateStrategy;
use bevy::window::WindowOccluded;
use clap::{Parser, ValueEnum};
use sg_render::scene::Playback;
use sg_render::{Quality, ScaleMode};
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
    /// Formation model: the modern pipeline, or the Classic (Dole) one.
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
    /// Select the Nth moon of the system, from 1.
    #[arg(long)]
    moon: Option<usize>,
    /// Select the Nth comet of the system, from 1.
    #[arg(long)]
    comet: Option<usize>,
    /// Look at the Nth belt of the system, from 1.
    #[arg(long)]
    belt: Option<usize>,
    /// Render a few frames, save a screenshot here, and exit. For checking
    /// the visuals without a person at the keyboard.
    #[arg(long)]
    screenshot: Option<PathBuf>,
    /// Hide the UI panels (useful with --screenshot).
    #[arg(long)]
    no_ui: bool,
    /// With --screenshot: draw the scene into an image instead of the
    /// window, so it works with the window covered, the display asleep or
    /// the screen locked.
    #[arg(long)]
    offscreen: bool,
    /// Show the frame rate in a corner (also a box in the panel).
    #[arg(long)]
    fps: bool,
    /// On a narrow window, start with the controls drawer open.
    #[arg(long)]
    drawer: bool,
    /// Start the spaceship from the body given by --select.
    #[arg(long)]
    tour: bool,
    /// Start with the Help window open.
    #[arg(long = "help-window")]
    help_window: bool,
    /// Save the view as a binary glTF file (.glb) and exit.
    #[arg(long)]
    export: Option<PathBuf>,
    /// Play the view and save numbered PNG frames in this folder, off screen.
    #[arg(long)]
    record: Option<PathBuf>,
    /// How many frames to record.
    #[arg(long, default_value_t = 300)]
    frames: u32,
    /// Frames per second of the recording.
    #[arg(long, default_value_t = 30)]
    frame_rate: u32,
    /// Start with the Layers window open.
    #[arg(long)]
    layers: bool,
    /// Layers to start hidden, comma-separated: stars, planets, moons, rings,
    /// belts, comets, tails, orbits, zone, limits, names, marks, disk, sky.
    #[arg(long, value_delimiter = ',')]
    hide: Vec<String>,
    /// Interface language.
    #[arg(long, value_parser = ["en", "es", "fr", "de"])]
    lang: Option<String>,
    /// Frame the body given by --select this many times closer (above about
    /// 7 the camera is inside it).
    #[arg(long, default_value_t = 1.0)]
    zoom: f32,
    /// Effects level: full, or the reduced set meant for phones.
    #[arg(long, value_enum)]
    quality: Option<QualityArg>,
    /// Window size in pixels, as WIDTHxHEIGHT (for example 390x844 to see
    /// the phone layout). With --offscreen, the size of the image.
    #[arg(long, value_parser = parse_size)]
    size: Option<(u32, u32)>,
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
    High,
}

#[derive(Clone, Copy, ValueEnum)]
enum QualityArg {
    Full,
    Reduced,
}

fn parse_size(text: &str) -> Result<(u32, u32), String> {
    let parsed = text
        .split_once(['x', 'X'])
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .filter(|(w, h): &(u32, u32)| (64..=8192).contains(w) && (64..=8192).contains(h));
    parsed.ok_or_else(|| format!("expected WIDTHxHEIGHT between 64 and 8192, got `{text}`"))
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
    /// Size of the off-screen image.
    size: (u32, u32),
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

/// Recording a video as numbered frames. Time advances by a fixed step, so
/// the result does not depend on how fast the machine draws.
#[derive(Resource)]
struct Recording {
    folder: PathBuf,
    frames: u32,
    /// Frames waited for shaders to compile.
    warmed: u32,
    /// Frames requested so far.
    requested: u32,
    /// Replay the formation from its start, rather than from `--time`.
    from_start: bool,
    target: Option<Handle<Image>>,
    size: (u32, u32),
}

/// Frames drawn before the first one is kept: shaders compile in the first
/// few, and a body whose shader is not ready is not drawn.
const RECORD_WARM_UP_FRAMES: u32 = 120;

/// Name of the Nth frame of a recording, from 1.
pub fn frame_name(index: u32) -> String {
    format!("frame-{index:05}.png")
}

/// Sets up `--record`.
pub fn add_recording(
    app: &mut App,
    folder: PathBuf,
    frames: u32,
    fps: u32,
    size: Option<(u32, u32)>,
    from_start: bool,
) {
    if let Err(error) = std::fs::create_dir_all(&folder) {
        eprintln!("record: {}: {error}", folder.display());
    }
    let step = std::time::Duration::from_secs_f64(1.0 / f64::from(fps.max(1)));
    app.insert_resource(TimeUpdateStrategy::ManualDuration(step))
        .insert_resource(Recording {
            folder,
            frames,
            warmed: 0,
            requested: 0,
            from_start,
            target: None,
            size: size.unwrap_or(OFFSCREEN_SIZE),
        })
        .add_systems(Update, (redirect_recording, record).chain());
}

fn redirect_recording(
    mut commands: Commands,
    mut recording: ResMut<Recording>,
    mut images: ResMut<Assets<Image>>,
    camera: Query<Entity, With<Camera3d>>,
) {
    if recording.target.is_some() {
        return;
    }
    let Ok(camera) = camera.single() else {
        return;
    };
    let size = recording.size;
    let image = images.add(Image::new_target_texture(
        size.0,
        size.1,
        TextureFormat::Rgba8UnormSrgb,
        None,
    ));
    commands
        .entity(camera)
        .insert(RenderTarget::Image(image.clone().into()));
    recording.target = Some(image);
}

fn record(
    mut commands: Commands,
    mut recording: ResMut<Recording>,
    mut playback: ResMut<Playback>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(target) = recording.target.clone() else {
        return;
    };
    if recording.warmed < RECORD_WARM_UP_FRAMES {
        recording.warmed += 1;
        if recording.warmed == RECORD_WARM_UP_FRAMES {
            if recording.from_start {
                playback.t_myr = 0.0;
                playback.playing = true;
            }
            playback.orbits_running = true;
        }
        return;
    }
    if recording.requested < recording.frames {
        recording.requested += 1;
        let path = recording.folder.join(frame_name(recording.requested));
        commands
            .spawn(Screenshot::image(target))
            .observe(save_to_disk(path));
        return;
    }
    // Frames are written in the background: wait for the last of them.
    let written = (1..=recording.frames)
        .filter(|index| recording.folder.join(frame_name(*index)).exists())
        .count() as u32;
    if written == recording.frames {
        println!("record: {written} frames in {}", recording.folder.display());
        exit.write(AppExit::Success);
    }
}

/// What a save dialog left: the note for the panel, once it is done.
pub type SaveNote = std::sync::Arc<std::sync::Mutex<Option<Result<String, String>>>>;

/// Asks where to save, then saves, away from the frame loop: the dialog
/// stays open as long as the person likes, and the view keeps drawing.
/// Cancelling leaves no note.
pub fn save_with_dialog(name: String, bytes: Vec<u8>, note: SaveNote) {
    std::thread::spawn(move || {
        let chosen = bevy::tasks::block_on(
            rfd::AsyncFileDialog::new()
                .set_file_name(&name)
                .add_filter("glTF binary", &["glb"])
                .save_file(),
        );
        let Some(chosen) = chosen else {
            return;
        };
        let path = chosen.path().to_path_buf();
        let saved = std::fs::write(&path, bytes)
            .map(|()| path.display().to_string())
            .map_err(|error| error.to_string());
        if let Ok(mut slot) = note.lock() {
            *slot = Some(saved);
        }
    });
}

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
    let size = capture.size;
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
            TierArg::High => Fidelity::High,
        },
        scale: match args.scale {
            ScaleArg::True => ScaleMode::True,
            ScaleArg::Enlarged => ScaleMode::Exaggerated,
            ScaleArg::Schematic => ScaleMode::Schematic,
        },
        time: args.time,
        select: args.select,
        moon: args.moon,
        comet: args.comet,
        belt: args.belt,
        screenshot: args.screenshot,
        offscreen: args.offscreen || args.bench.is_some(),
        bench: args.bench,
        show_ui: !args.no_ui && args.bench.is_none(),
        quality: args.quality.map(|quality| match quality {
            QualityArg::Full => Quality::Full,
            QualityArg::Reduced => Quality::Reduced,
        }),
        size: args.size,
        drawer: args.drawer,
        tour: args.tour,
        help: args.help_window,
        layers: args.layers,
        hide: args.hide,
        record: args.record,
        record_frames: args.frames,
        record_fps: args.frame_rate,
        export: args.export,
        zoom: args.zoom,
        language: args
            .lang
            .as_deref()
            .and_then(Language::from_code)
            .unwrap_or_default(),
        fps: args.fps,
    }
}

/// Adds screenshot mode to the app.
pub fn add_capture(
    app: &mut App,
    path: PathBuf,
    offscreen: bool,
    bench_secs: Option<f32>,
    size: Option<(u32, u32)>,
) {
    let default_size = if bench_secs.is_some() {
        BENCH_SIZE
    } else {
        OFFSCREEN_SIZE
    };
    app.insert_resource(Capture {
        path,
        offscreen,
        bench_secs,
        size: size.unwrap_or(default_size),
        ..default()
    })
    .add_systems(Update, (redirect_camera, capture).chain());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `scripts/make-video.sh` expects these names.
    #[test]
    fn frames_are_numbered_from_one_in_five_digits() {
        assert_eq!(frame_name(1), "frame-00001.png");
        assert_eq!(frame_name(12345), "frame-12345.png");
    }
}
