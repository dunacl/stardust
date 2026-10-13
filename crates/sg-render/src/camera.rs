//! Orbit camera and click selection.

use crate::scene::{BodyPlacements, Selection, Target};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// Set by the app each frame: true when a UI layer is using the pointer, so
/// the scene should ignore it.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PointerCaptured(pub bool);

/// A camera that orbits a focus point. `goal_*` are where it is heading; the
/// live values ease toward them.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct OrbitCamera {
    pub focus: Vec3,
    pub goal_focus: Vec3,
    /// Radians about the vertical axis.
    pub yaw: f32,
    /// Radians above the orbital plane.
    pub pitch: f32,
    pub distance: f32,
    pub goal_distance: f32,
    /// Pixels the pointer has travelled since the button went down; a press
    /// that moves less than a few pixels counts as a click.
    drag_pixels: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            focus: Vec3::ZERO,
            goal_focus: Vec3::ZERO,
            yaw: 0.6,
            pitch: 0.9,
            distance: 8.0,
            goal_distance: 8.0,
            drag_pixels: 0.0,
        }
    }
}

pub const MIN_DISTANCE: f32 = 2.0e-4;
pub const MAX_DISTANCE: f32 = 600.0;
const CLICK_SLOP_PIXELS: f32 = 4.0;

impl OrbitCamera {
    /// Frames the whole system: `extent` is the scene radius to fit.
    pub fn frame(&mut self, extent: f32) {
        self.goal_focus = Vec3::ZERO;
        self.goal_distance = (extent * 2.9).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    /// Jumps to the goals without easing.
    pub fn snap(&mut self) {
        self.focus = self.goal_focus;
        self.distance = self.goal_distance;
    }

    /// Camera position for the current state.
    pub fn eye(&self) -> Vec3 {
        let rotation = Quat::from_euler(EulerRot::YXZ, self.yaw, -self.pitch, 0.0);
        self.focus + rotation * Vec3::new(0.0, 0.0, self.distance)
    }
}

pub(crate) fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Camera {
            clear_color: ClearColorConfig::Custom(Color::srgb(0.004, 0.005, 0.012)),
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            // Small enough to approach a true-scale planet (radius ~1e-5).
            near: 2.0e-5,
            far: 5.0e3,
            ..default()
        }),
        bevy::core_pipeline::tonemapping::Tonemapping::TonyMcMapface,
        bevy::post_process::bloom::Bloom::NATURAL,
        Transform::default(),
    ));
}

pub(crate) fn control_camera(
    mut orbit: ResMut<OrbitCamera>,
    captured: Res<PointerCaptured>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
) {
    if captured.0 {
        return;
    }
    if buttons.just_pressed(MouseButton::Left) {
        orbit.drag_pixels = 0.0;
    }
    if buttons.pressed(MouseButton::Left) {
        orbit.drag_pixels += motion.delta.length();
        if orbit.drag_pixels > CLICK_SLOP_PIXELS {
            orbit.yaw -= motion.delta.x * 0.005;
            orbit.pitch = (orbit.pitch + motion.delta.y * 0.005).clamp(-1.55, 1.55);
        }
    }
    if scroll.delta.y != 0.0 {
        let notches = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y,
            MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
        };
        orbit.goal_distance =
            (orbit.goal_distance * (-0.12 * notches).exp()).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }
}

/// Eases the camera toward its goals and writes its transform.
pub(crate) fn apply_camera(
    mut orbit: ResMut<OrbitCamera>,
    selection: Res<Selection>,
    placements: Res<BodyPlacements>,
    time: Res<Time>,
    mut camera: Single<&mut Transform, With<Camera3d>>,
    mut followed: Local<Option<Target>>,
) {
    // Follow whatever is selected, and move in on a new selection.
    if let Some(target) = selection.0 {
        if let Some(placement) = placements.get(target) {
            orbit.goal_focus = placement.position;
            if *followed != Some(target) {
                orbit.goal_distance =
                    (placement.body_radius * 9.0).clamp(MIN_DISTANCE, MAX_DISTANCE);
                *followed = Some(target);
            }
        }
    } else {
        *followed = None;
    }
    let ease = 1.0 - (-8.0 * time.delta_secs()).exp();
    let goal_focus = orbit.goal_focus;
    orbit.focus = orbit.focus.lerp(goal_focus, ease);
    // Ease distance in log space so zooming feels even at every scale.
    let log = orbit.distance.ln() + (orbit.goal_distance.ln() - orbit.distance.ln()) * ease;
    orbit.distance = log.exp();

    let eye = orbit.eye();
    **camera = Transform::from_translation(eye).looking_at(orbit.focus, Vec3::Y);
}

/// Nearest intersection of a ray with a sphere, as a distance along the ray.
pub fn ray_sphere(origin: Vec3, direction: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let to_center = center - origin;
    let along = to_center.dot(direction);
    let closest_sq = to_center.length_squared() - along * along;
    let radius_sq = radius * radius;
    if along < 0.0 || closest_sq > radius_sq {
        return None;
    }
    Some(along - (radius_sq - closest_sq).sqrt())
}

/// Selects the body under the pointer on a click (a press and release
/// without dragging). Clicking empty space clears the selection.
pub(crate) fn pick(
    orbit: Res<OrbitCamera>,
    captured: Res<PointerCaptured>,
    buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<Camera3d>>,
    placements: Res<BodyPlacements>,
    mut selection: ResMut<Selection>,
) {
    if captured.0
        || !buttons.just_released(MouseButton::Left)
        || orbit.drag_pixels > CLICK_SLOP_PIXELS
    {
        return;
    }
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let (camera, camera_transform) = *camera;
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor) else {
        return;
    };
    let direction: Vec3 = ray.direction.into();

    let mut best: Option<(f32, Target)> = None;
    for (target, placement) in placements.iter() {
        // Small or distant bodies get a generous click target.
        let distance = (placement.position - ray.origin).length();
        let radius = placement.radius.max(distance * 0.012);
        if let Some(hit) = ray_sphere(ray.origin, direction, placement.position, radius) {
            if best.is_none_or(|(d, _)| hit < d) {
                best = Some((hit, target));
            }
        }
    }
    let picked = best.map(|(_, target)| target);
    if selection.0 != picked {
        selection.0 = picked;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_hits_and_misses_spheres() {
        let origin = Vec3::new(0.0, 0.0, 10.0);
        let forward = Vec3::NEG_Z;
        assert_eq!(ray_sphere(origin, forward, Vec3::ZERO, 1.0), Some(9.0));
        assert_eq!(
            ray_sphere(origin, forward, Vec3::new(2.0, 0.0, 0.0), 1.0),
            None
        );
        // Behind the camera.
        assert_eq!(
            ray_sphere(origin, forward, Vec3::new(0.0, 0.0, 20.0), 1.0),
            None
        );
        // Grazing.
        assert!(ray_sphere(origin, forward, Vec3::new(1.0, 0.0, 0.0), 1.0).is_some());
    }

    #[test]
    fn eye_sits_at_the_orbit_distance_from_the_focus() {
        let mut orbit = OrbitCamera {
            focus: Vec3::new(1.0, 2.0, 3.0),
            ..OrbitCamera::default()
        };
        assert!(((orbit.eye() - orbit.focus).length() - orbit.distance).abs() < 1e-5);
        // Positive pitch looks down on the orbital plane.
        assert!(orbit.eye().y > orbit.focus.y);
        orbit.frame(50.0);
        orbit.snap();
        assert_eq!(orbit.focus, Vec3::ZERO);
        assert_eq!(orbit.distance, 145.0);
    }
}
