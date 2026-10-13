//! Scale modes (PLAN.md §8.1): how astronomical sizes map to scene units.
//!
//! One scene unit is one AU in the linear modes. Real planets are far too
//! small to see at the scale of their orbits (Earth is 4×10⁻⁵ AU across its
//! orbit of 1 AU), so two modes trade truth for legibility.

/// How distances and body sizes are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ScaleMode {
    /// Everything to scale. Bodies are specks.
    True,
    /// Orbits to scale, bodies enlarged.
    #[default]
    Exaggerated,
    /// Distances log-compressed, bodies enlarged: the whole system fits in
    /// view at once.
    Schematic,
}

/// Kilometres per AU.
const KM_PER_AU: f32 = 1.495_978_7e8;
const EARTH_RADIUS_KM: f32 = 6378.0;
const SOLAR_RADIUS_KM: f32 = 695_700.0;

/// Distance at which the schematic compression sets in, AU.
const SCHEMATIC_KNEE_AU: f32 = 0.3;
/// Scene units per e-fold of distance in schematic mode.
const SCHEMATIC_UNITS_PER_EFOLD: f32 = 2.0;

impl ScaleMode {
    /// Scene distance from the star for an orbital distance in AU.
    /// Monotonic and zero at zero in every mode.
    pub fn distance(self, r_au: f32) -> f32 {
        match self {
            ScaleMode::True | ScaleMode::Exaggerated => r_au,
            ScaleMode::Schematic => {
                SCHEMATIC_UNITS_PER_EFOLD * (1.0 + r_au / SCHEMATIC_KNEE_AU).ln()
            }
        }
    }

    /// Scene radius of a planet of the given real radius.
    pub fn planet_radius(self, radius_km: f32) -> f32 {
        let earths = (radius_km / EARTH_RADIUS_KM).max(0.0);
        match self {
            ScaleMode::True => radius_km / KM_PER_AU,
            // Square root keeps Jupiter (11 R⊕) from swallowing its neighbours.
            ScaleMode::Exaggerated => 0.02 * earths.sqrt(),
            ScaleMode::Schematic => 0.07 * earths.powf(0.4),
        }
    }

    /// Scene radius of a star of the given real radius in solar radii.
    pub fn star_radius(self, radius_rsun: f32) -> f32 {
        match self {
            ScaleMode::True => radius_rsun * SOLAR_RADIUS_KM / KM_PER_AU,
            ScaleMode::Exaggerated => 0.09 * radius_rsun.sqrt(),
            ScaleMode::Schematic => 0.45 * radius_rsun.powf(0.4),
        }
    }

    pub const ALL: [ScaleMode; 3] = [
        ScaleMode::True,
        ScaleMode::Exaggerated,
        ScaleMode::Schematic,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ScaleMode::True => "True scale",
            ScaleMode::Exaggerated => "Enlarged bodies",
            ScaleMode::Schematic => "Schematic",
        }
    }
}

/// An animated transition between two scale modes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScaleBlend {
    pub from: ScaleMode,
    pub to: ScaleMode,
    /// 0 = entirely `from`, 1 = entirely `to`.
    pub progress: f32,
}

impl Default for ScaleBlend {
    fn default() -> Self {
        Self::settled(ScaleMode::default())
    }
}

/// Seconds a scale transition takes.
pub const SCALE_TRANSITION_SECS: f32 = 0.8;

impl ScaleBlend {
    pub fn settled(mode: ScaleMode) -> Self {
        Self {
            from: mode,
            to: mode,
            progress: 1.0,
        }
    }

    /// Starts a transition to `mode` from wherever the blend currently is.
    pub fn go_to(&mut self, mode: ScaleMode) {
        if mode == self.to {
            return;
        }
        // Mid-transition changes restart from the nearer endpoint; the jump
        // is small and transitions are short.
        self.from = if self.progress >= 0.5 {
            self.to
        } else {
            self.from
        };
        self.to = mode;
        self.progress = 0.0;
    }

    pub fn advance(&mut self, dt_secs: f32) {
        self.progress = (self.progress + dt_secs / SCALE_TRANSITION_SECS).min(1.0);
    }

    pub fn is_settled(&self) -> bool {
        self.progress >= 1.0
    }

    /// Smoothstep-eased weight of `to`.
    fn weight(&self) -> f32 {
        let t = self.progress.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    fn mix(&self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.weight()
    }

    pub fn distance(&self, r_au: f32) -> f32 {
        self.mix(self.from.distance(r_au), self.to.distance(r_au))
    }

    /// Body radii are blended geometrically: they differ by orders of
    /// magnitude between modes.
    pub fn planet_radius(&self, radius_km: f32) -> f32 {
        geometric_mix(
            self.from.planet_radius(radius_km),
            self.to.planet_radius(radius_km),
            self.weight(),
        )
    }

    pub fn star_radius(&self, radius_rsun: f32) -> f32 {
        geometric_mix(
            self.from.star_radius(radius_rsun),
            self.to.star_radius(radius_rsun),
            self.weight(),
        )
    }
}

fn geometric_mix(a: f32, b: f32, w: f32) -> f32 {
    if a <= 0.0 || b <= 0.0 {
        return a + (b - a) * w;
    }
    (a.ln() + (b.ln() - a.ln()) * w).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn true_scale_uses_real_sizes() {
        // Earth: 6378 km = 4.26e-5 AU. Sun: 4.65e-3 AU.
        assert!((ScaleMode::True.planet_radius(6378.0) - 4.263e-5).abs() < 1e-7);
        assert!((ScaleMode::True.star_radius(1.0) - 4.650e-3).abs() < 1e-5);
        assert_eq!(ScaleMode::True.distance(5.2), 5.2);
    }

    #[test]
    fn distances_are_monotonic_in_every_mode() {
        for mode in ScaleMode::ALL {
            assert_eq!(mode.distance(0.0), 0.0);
            let mut last = 0.0;
            for i in 1..500 {
                let d = mode.distance(i as f32 * 0.1);
                assert!(d > last, "{mode:?} at {i}");
                last = d;
            }
        }
    }

    /// Schematic mode brings Neptune's orbit within a few times Earth's.
    #[test]
    fn schematic_compresses_the_outer_system() {
        let earth = ScaleMode::Schematic.distance(1.0);
        let neptune = ScaleMode::Schematic.distance(30.1);
        assert!(neptune / earth < 3.5, "{}", neptune / earth);
    }

    #[test]
    fn enlarged_bodies_keep_their_order_and_stay_apart() {
        for mode in [ScaleMode::Exaggerated, ScaleMode::Schematic] {
            let earth = mode.planet_radius(6378.0);
            let jupiter = mode.planet_radius(71_492.0);
            assert!(jupiter > earth && jupiter < 4.0 * earth);
            // An Earth at 1 AU and a Venus at 0.72 AU must not touch.
            let gap = mode.distance(1.0) - mode.distance(0.72);
            assert!(gap > 2.0 * earth, "{mode:?}");
            assert!(mode.star_radius(1.0) > jupiter);
        }
    }

    #[test]
    fn blend_moves_from_one_mode_to_the_other() {
        let mut blend = ScaleBlend::settled(ScaleMode::True);
        assert!(blend.is_settled());
        blend.go_to(ScaleMode::Schematic);
        assert_eq!(blend.distance(10.0), ScaleMode::True.distance(10.0));
        blend.advance(SCALE_TRANSITION_SECS / 2.0);
        let mid = blend.distance(10.0);
        assert!(mid < ScaleMode::True.distance(10.0) && mid > ScaleMode::Schematic.distance(10.0));
        let r = blend.planet_radius(6378.0);
        assert!(
            r > ScaleMode::True.planet_radius(6378.0)
                && r < ScaleMode::Schematic.planet_radius(6378.0)
        );
        blend.advance(SCALE_TRANSITION_SECS);
        assert!(blend.is_settled());
        assert_eq!(blend.distance(10.0), ScaleMode::Schematic.distance(10.0));
    }
}
