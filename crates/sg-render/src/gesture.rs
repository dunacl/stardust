//! Touch gestures for the camera: what the fingers on the screen mean.
//!
//! Pure functions over finger positions, so they are unit-tested without a
//! touch screen. [`crate::camera`] feeds them from Bevy's touch input.
//!
//! - One finger dragged turns the view.
//! - Two fingers moved apart or together zoom, as a pinch does on a map:
//!   the scene under the fingers scales with their separation.
//! - One finger put down and lifted without moving selects what is under
//!   it.

use bevy::math::Vec2;

/// A finger on the screen, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Finger {
    pub id: u64,
    /// Where it was last frame.
    pub previous: Vec2,
    pub position: Vec2,
}

/// What the fingers did this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gesture {
    None,
    /// One finger moved by this many pixels.
    Turn(Vec2),
    /// Two fingers: the camera's distance should be multiplied by `zoom`
    /// (below 1 when the fingers move apart).
    Pinch {
        zoom: f32,
    },
}

/// Fingers closer together than this, pixels, are not read as a pinch: the
/// ratio of two tiny separations is noise.
const MIN_SPREAD_PIXELS: f32 = 12.0;
/// The most one frame of pinching may change the distance, either way.
const MAX_ZOOM_STEP: f32 = 1.5;
/// A finger that has moved less than this, pixels, since it went down has
/// tapped rather than dragged. Fingers are less steady than a mouse, whose
/// allowance is 4.
pub const TAP_SLOP_PIXELS: f32 = 10.0;

/// The gesture of this frame. Three or more fingers mean nothing.
pub fn gesture(fingers: &[Finger]) -> Gesture {
    match fingers {
        [one] => {
            let delta = one.position - one.previous;
            if delta == Vec2::ZERO {
                Gesture::None
            } else {
                Gesture::Turn(delta)
            }
        }
        [a, b] => {
            let before = (a.previous - b.previous).length();
            let now = (a.position - b.position).length();
            if before < MIN_SPREAD_PIXELS || now < MIN_SPREAD_PIXELS {
                return Gesture::None;
            }
            Gesture::Pinch {
                zoom: (before / now).clamp(1.0 / MAX_ZOOM_STEP, MAX_ZOOM_STEP),
            }
        }
        _ => Gesture::None,
    }
}

/// Follows one touch from finger down to finger up, to tell a tap from a
/// drag or a pinch.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TapTracker {
    /// The finger being followed and where it went down.
    first: Option<(u64, Vec2)>,
    /// It moved too far, or another finger joined: no longer a tap.
    spoiled: bool,
}

impl TapTracker {
    /// Call once per frame with the fingers now down and those lifted this
    /// frame (with the position each was lifted at). Returns where a tap
    /// was completed, if one was.
    pub fn update(&mut self, down: &[Finger], lifted: &[(u64, Vec2)]) -> Option<Vec2> {
        if self.first.is_none() {
            if let [only] = down {
                self.first = Some((only.id, only.position));
                self.spoiled = false;
            }
        }
        let (id, start) = self.first?;
        if down.len() > 1 {
            self.spoiled = true;
        }
        if let Some(finger) = down.iter().find(|f| f.id == id) {
            if (finger.position - start).length() > TAP_SLOP_PIXELS {
                self.spoiled = true;
            }
        }
        let mut tap = None;
        if let Some((_, at)) = lifted.iter().find(|(lifted_id, _)| *lifted_id == id) {
            if !self.spoiled && (*at - start).length() <= TAP_SLOP_PIXELS {
                tap = Some(*at);
            }
            self.first = None;
        }
        // Every finger gone without ours being reported: start afresh.
        if down.is_empty() && lifted.is_empty() {
            self.first = None;
        }
        tap
    }

    /// Abandons the touch being followed (it began on a panel, say).
    pub fn cancel(&mut self) {
        self.first = None;
        self.spoiled = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finger(id: u64, previous: (f32, f32), position: (f32, f32)) -> Finger {
        Finger {
            id,
            previous: previous.into(),
            position: position.into(),
        }
    }

    #[test]
    fn one_finger_turns_by_its_movement() {
        let g = gesture(&[finger(1, (100.0, 100.0), (130.0, 90.0))]);
        assert_eq!(g, Gesture::Turn(Vec2::new(30.0, -10.0)));
        assert_eq!(gesture(&[finger(1, (5.0, 5.0), (5.0, 5.0))]), Gesture::None);
        assert_eq!(gesture(&[]), Gesture::None);
    }

    #[test]
    fn spreading_two_fingers_zooms_in_by_the_ratio() {
        // 100 px apart, then 200: the scene doubles, the distance halves...
        let g = gesture(&[
            finger(1, (100.0, 300.0), (50.0, 300.0)),
            finger(2, (200.0, 300.0), (250.0, 300.0)),
        ]);
        // ...but one frame is limited to a factor of 1.5.
        assert_eq!(g, Gesture::Pinch { zoom: 1.0 / 1.5 });
        // A small spread is taken as it is: 100 → 110 px.
        let Gesture::Pinch { zoom } = gesture(&[
            finger(1, (100.0, 300.0), (95.0, 300.0)),
            finger(2, (200.0, 300.0), (205.0, 300.0)),
        ]) else {
            panic!("pinch expected");
        };
        assert!((zoom - 100.0 / 110.0).abs() < 1e-6);
        // Pinching together zooms out.
        let Gesture::Pinch { zoom } = gesture(&[
            finger(1, (100.0, 300.0), (105.0, 300.0)),
            finger(2, (200.0, 300.0), (195.0, 300.0)),
        ]) else {
            panic!("pinch expected");
        };
        assert!(zoom > 1.0);
    }

    #[test]
    fn two_fingers_moving_together_do_not_zoom() {
        let g = gesture(&[
            finger(1, (100.0, 300.0), (140.0, 320.0)),
            finger(2, (200.0, 300.0), (240.0, 320.0)),
        ]);
        assert_eq!(g, Gesture::Pinch { zoom: 1.0 });
        // Fingers almost touching, or three of them, mean nothing.
        let close = gesture(&[
            finger(1, (100.0, 300.0), (100.0, 300.0)),
            finger(2, (105.0, 300.0), (130.0, 300.0)),
        ]);
        assert_eq!(close, Gesture::None);
        let three = [
            finger(1, (0.0, 0.0), (9.0, 0.0)),
            finger(2, (50.0, 0.0), (59.0, 0.0)),
            finger(3, (90.0, 0.0), (99.0, 0.0)),
        ];
        assert_eq!(gesture(&three), Gesture::None);
    }

    #[test]
    fn a_still_finger_taps_and_a_moving_one_does_not() {
        let mut tracker = TapTracker::default();
        let at = Vec2::new(200.0, 400.0);
        assert_eq!(
            tracker.update(&[finger(7, (200.0, 400.0), (200.0, 400.0))], &[]),
            None
        );
        assert_eq!(
            tracker.update(&[finger(7, (200.0, 400.0), (203.0, 402.0))], &[]),
            None
        );
        assert_eq!(
            tracker.update(&[], &[(7, at + Vec2::new(3.0, 2.0))]),
            Some(at + Vec2::new(3.0, 2.0))
        );

        // A drag: far from the start at some point, even if it comes back.
        assert_eq!(
            tracker.update(&[finger(8, (10.0, 10.0), (10.0, 10.0))], &[]),
            None
        );
        assert_eq!(
            tracker.update(&[finger(8, (10.0, 10.0), (60.0, 10.0))], &[]),
            None
        );
        assert_eq!(
            tracker.update(&[finger(8, (60.0, 10.0), (11.0, 10.0))], &[]),
            None
        );
        assert_eq!(tracker.update(&[], &[(8, Vec2::new(11.0, 10.0))]), None);

        // The tracker is ready for the next touch.
        assert_eq!(
            tracker.update(&[finger(9, (5.0, 5.0), (5.0, 5.0))], &[]),
            None
        );
        assert_eq!(
            tracker.update(&[], &[(9, Vec2::new(5.0, 5.0))]),
            Some(Vec2::new(5.0, 5.0))
        );
    }

    #[test]
    fn a_second_finger_spoils_the_tap() {
        let mut tracker = TapTracker::default();
        let a = finger(1, (100.0, 100.0), (100.0, 100.0));
        let b = finger(2, (300.0, 100.0), (300.0, 100.0));
        assert_eq!(tracker.update(&[a], &[]), None);
        assert_eq!(tracker.update(&[a, b], &[]), None);
        assert_eq!(tracker.update(&[a], &[(2, b.position)]), None);
        assert_eq!(tracker.update(&[], &[(1, a.position)]), None);
        // Cancelled touches never tap either.
        assert_eq!(tracker.update(&[a], &[]), None);
        tracker.cancel();
        assert_eq!(tracker.update(&[], &[(1, a.position)]), None);
    }
}
