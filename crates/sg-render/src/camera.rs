//! Orbit camera and click selection.

use crate::scene::{BodyPlacements, Selection, Target};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::math::DVec3;
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
    /// The point of the system, in double precision, that is drawn at the
    /// renderer's origin. Everything else here and everything drawn is
    /// relative to it, in single precision; [`OrbitCamera::rebase`] moves
    /// it to the focus every frame, so that whatever the camera is looking
    /// at is placed exactly (PLAN.md §8.1).
    pub origin: DVec3,
    pub focus: Vec3,
    pub goal_focus: Vec3,
    /// Radians about the vertical axis.
    pub yaw: f32,
    /// Radians above the orbital plane.
    pub pitch: f32,
    pub distance: f32,
    pub goal_distance: f32,
    /// Yaw and pitch the camera is swinging toward, after a body is
    /// selected, so that it is seen from its day side. Dragging cancels it.
    pub swing: Option<(f32, f32)>,
    /// Pixels the pointer has travelled since the button went down; a press
    /// that moves less than a few pixels counts as a click.
    drag_pixels: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            origin: DVec3::ZERO,
            focus: Vec3::ZERO,
            goal_focus: Vec3::ZERO,
            yaw: 0.6,
            pitch: 0.9,
            distance: 8.0,
            goal_distance: 8.0,
            swing: None,
            drag_pixels: 0.0,
        }
    }
}

pub const MIN_DISTANCE: f32 = 2.0e-4;
/// Far enough to frame the widest binaries drawn (several thousand AU).
pub const MAX_DISTANCE: f32 = 1.0e5;
const CLICK_SLOP_PIXELS: f32 = 4.0;

impl OrbitCamera {
    /// Frames the whole system: `extent` is the scene radius to fit.
    pub fn frame(&mut self, extent: f32) {
        self.goal_focus = self.local(DVec3::ZERO);
        self.goal_distance = (extent * 2.9).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    /// A point of the system relative to the origin.
    pub fn local(&self, world: DVec3) -> Vec3 {
        (world - self.origin).as_vec3()
    }

    /// Moves the origin to the focus, keeping the view the same.
    pub fn rebase(&mut self) {
        self.origin += self.focus.as_dvec3();
        self.goal_focus -= self.focus;
        self.focus = Vec3::ZERO;
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
            far: 1.0e7,
            ..default()
        }),
        bevy::core_pipeline::tonemapping::Tonemapping::TonyMcMapface,
        bevy::post_process::bloom::Bloom::NATURAL,
        Transform::default(),
    ));
}

pub(crate) fn rebase_camera(mut orbit: ResMut<OrbitCamera>) {
    orbit.rebase();
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
            orbit.swing = None;
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
                    (placement.focus_radius * 9.0).clamp(MIN_DISTANCE, MAX_DISTANCE);
                *followed = Some(target);
                if let Target::Body(_) = target {
                    let nearest_star = placements
                        .iter()
                        .filter(|(other, _)| matches!(other, Target::Star(_)))
                        .map(|(_, star)| star.position - placement.position)
                        .min_by(|a, b| a.length_squared().total_cmp(&b.length_squared()));
                    if let Some(to_star) = nearest_star {
                        orbit.swing = Some(day_side_view(to_star));
                    }
                }
            }
        }
    } else {
        *followed = None;
    }
    let ease = 1.0 - (-8.0 * time.delta_secs()).exp();
    if let Some((yaw, pitch)) = orbit.swing {
        let turn = shortest_turn(orbit.yaw, yaw);
        orbit.yaw += turn * ease;
        orbit.pitch += (pitch - orbit.pitch) * ease;
        if turn.abs() < 1e-3 && (pitch - orbit.pitch).abs() < 1e-3 {
            orbit.swing = None;
        }
    }
    let goal_focus = orbit.goal_focus;
    orbit.focus = orbit.focus.lerp(goal_focus, ease);
    // Ease distance in log space so zooming feels even at every scale.
    let log = orbit.distance.ln() + (orbit.goal_distance.ln() - orbit.distance.ln()) * ease;
    orbit.distance = log.exp();

    let eye = orbit.eye();
    **camera = Transform::from_translation(eye).looking_at(orbit.focus, Vec3::Y);
}

/// Angle of the sun behind the camera's shoulder when a body is selected:
/// far enough round to show the terminator.
const DAY_SIDE_PHASE: f32 = 0.7;
/// Height of that view above the orbital plane, radians.
const DAY_SIDE_PITCH: f32 = 0.3;

/// Yaw and pitch from which a body is seen mostly lit, given the direction
/// from it to its star: the camera stands on the star's side, turned
/// [`DAY_SIDE_PHASE`] round, so about four fifths of the disc is in
/// daylight.
pub fn day_side_view(to_star: Vec3) -> (f32, f32) {
    (to_star.x.atan2(to_star.z) + DAY_SIDE_PHASE, DAY_SIDE_PITCH)
}

/// Signed smallest rotation from one angle to another, radians.
pub fn shortest_turn(from: f32, to: f32) -> f32 {
    (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
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
    fn selected_bodies_are_seen_from_their_day_side() {
        for to_star in [Vec3::X, Vec3::new(-3.0, 0.0, 2.0), Vec3::NEG_Z] {
            let (yaw, pitch) = day_side_view(to_star);
            let camera = OrbitCamera {
                yaw,
                pitch,
                distance: 1.0,
                ..OrbitCamera::default()
            };
            // The angle at the body between the star and the camera.
            let phase = camera.eye().normalize().dot(to_star.normalize()).acos();
            assert!(phase > 0.5 && phase < 1.0, "{phase}");
        }
    }

    #[test]
    fn rebasing_keeps_the_view_and_the_precision() {
        // Following something 30 AU out, a metre at a time.
        let far = DVec3::new(30.0, 0.0, -4.0);
        let mut camera = OrbitCamera::default();
        camera.goal_focus = camera.local(far);
        camera.snap();
        let eye_before = camera.origin + camera.eye().as_dvec3();
        camera.rebase();
        assert_eq!(camera.focus, Vec3::ZERO);
        assert!((camera.origin - far).length() < 1e-5);
        let eye_after = camera.origin + camera.eye().as_dvec3();
        assert!((eye_before - eye_after).length() < 1e-4);
        // A point one metre (6.7e-12 AU) from the focus is still resolved,
        // to the few millimetres double precision allows at 30 AU. Single
        // precision there is good to about 300 km.
        let metre = 6.7e-12;
        let near = camera.local(camera.origin + DVec3::new(metre, 0.0, 0.0));
        assert!((f64::from(near.x) / metre - 1.0).abs() < 5e-3);
        // Framing the system looks back at its centre.
        camera.frame(10.0);
        assert!((camera.origin + camera.goal_focus.as_dvec3()).length() < 1e-5);
    }

    #[test]
    fn turns_take_the_short_way_round() {
        assert!((shortest_turn(0.1, 0.3) - 0.2).abs() < 1e-6);
        assert!((shortest_turn(6.2, 0.1) - (0.1 + std::f32::consts::TAU - 6.2)).abs() < 1e-5);
        assert!((shortest_turn(0.1, 6.2) + (0.1 + std::f32::consts::TAU - 6.2)).abs() < 1e-5);
    }

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
