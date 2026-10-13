//! What happens to a planetary system once the gas that kept it orderly is
//! gone (PLAN.md §5.7, Standard tier).
//!
//! A full treatment integrates the planets' orbits for millions of years
//! (the High tier, M7). Here the outcome is approximated: neighbours too
//! close to stay apart are merged, one pair at a time, until the survivors
//! are spaced widely enough to last.
//!
//! Not captured: scattering and ejection (which give giant planets their
//! eccentricities), the slow breaking of resonant chains by chaos, and
//! secular effects.

use sg_core::math::{pow, sqrt};

/// Two planets alone are stable if separated by more than `2√3` mutual Hill
/// radii (Gladman 1993, Icarus 106, 247).
pub const PAIR_STABILITY_HILL: f64 = 3.464;
/// Systems of three or more planets closer than about 10 mutual Hill radii
/// are always unstable on long timescales (Chambers, Wetherill & Boss 1996,
/// Icarus 119, 261).
pub const MULTI_STABILITY_HILL: f64 = 10.0;

/// A planet as the stability step sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Orbiter {
    /// Caller's identifier.
    pub id: u32,
    pub a_au: f64,
    pub mass_msun: f64,
}

/// A merger performed by [`settle`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Merger {
    pub survivor: u32,
    pub absorbed: u32,
}

/// Mutual Hill radius of two neighbours, AU:
/// `((m₁+m₂)/(3M★))^(1/3) · (a₁+a₂)/2` (Chambers et al. 1996, eq. 1).
pub fn mutual_hill_radius(a: &Orbiter, b: &Orbiter, star_mass_msun: f64) -> f64 {
    pow(
        (a.mass_msun + b.mass_msun) / (3.0 * star_mass_msun),
        1.0 / 3.0,
    ) * 0.5
        * (a.a_au + b.a_au)
}

/// Separation of two neighbours in mutual Hill radii.
pub fn separation_in_hill_radii(a: &Orbiter, b: &Orbiter, star_mass_msun: f64) -> f64 {
    (b.a_au - a.a_au).abs() / mutual_hill_radius(a, b, star_mass_msun)
}

/// Merges neighbours until every pair is separated by at least the
/// stability limit for the number of planets left. The closest pair (in
/// Hill radii) merges first; the heavier body keeps its identity; the
/// merged body takes the orbit with the pair's combined angular momentum
/// (for circular orbits, `√a` weighted by mass).
///
/// Returns the survivors ordered outward and the mergers in order.
pub fn settle(mut planets: Vec<Orbiter>, star_mass_msun: f64) -> (Vec<Orbiter>, Vec<Merger>) {
    let mut mergers = Vec::new();
    planets.sort_by(|x, y| x.a_au.total_cmp(&y.a_au).then(x.id.cmp(&y.id)));
    loop {
        let limit = if planets.len() > 2 {
            MULTI_STABILITY_HILL
        } else {
            PAIR_STABILITY_HILL
        };
        let closest = planets
            .windows(2)
            .enumerate()
            .map(|(i, pair)| {
                (
                    i,
                    separation_in_hill_radii(&pair[0], &pair[1], star_mass_msun),
                )
            })
            .filter(|(_, spacing)| *spacing < limit)
            .min_by(|x, y| x.1.total_cmp(&y.1));
        let Some((i, _)) = closest else {
            return (planets, mergers);
        };
        let (inner, outer) = (planets[i], planets[i + 1]);
        let mass = inner.mass_msun + outer.mass_msun;
        let momentum = inner.mass_msun * sqrt(inner.a_au) + outer.mass_msun * sqrt(outer.a_au);
        let (survivor, absorbed) = if outer.mass_msun > inner.mass_msun {
            (outer.id, inner.id)
        } else {
            (inner.id, outer.id)
        };
        planets[i] = Orbiter {
            id: survivor,
            a_au: pow(momentum / mass, 2.0),
            mass_msun: mass,
        };
        planets.remove(i + 1);
        mergers.push(Merger { survivor, absorbed });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const EARTH: f64 = 3.003e-6;

    fn planet(id: u32, a_au: f64, earths: f64) -> Orbiter {
        Orbiter {
            id,
            a_au,
            mass_msun: earths * EARTH,
        }
    }

    /// Earth and Venus: mutual Hill radius 0.0101 AU, 26 of them apart.
    #[test]
    fn hill_separation_of_venus_and_earth() {
        let (venus, earth) = (planet(0, 0.723, 0.815), planet(1, 1.0, 1.0));
        let r = mutual_hill_radius(&venus, &earth, 1.0);
        assert!((r - 0.0105).abs() < 5e-4, "{r}");
        let spacing = separation_in_hill_radii(&venus, &earth, 1.0);
        assert!((spacing - 26.3).abs() < 1.0, "{spacing}");
    }

    #[test]
    fn the_solar_system_terrestrials_are_left_alone() {
        let system = vec![
            planet(0, 0.387, 0.055),
            planet(1, 0.723, 0.815),
            planet(2, 1.0, 1.0),
            planet(3, 1.524, 0.107),
        ];
        let (after, mergers) = settle(system.clone(), 1.0);
        assert_eq!(after, system);
        assert!(mergers.is_empty());
    }

    #[test]
    fn crowded_planets_merge_until_stable() {
        // Five 3 M⊕ planets 8 mutual Hill radii apart: too close for five.
        let mut system = Vec::new();
        let mut a = 0.1;
        for id in 0..5 {
            system.push(planet(id, a, 3.0));
            a *= 1.0 + 8.0 * (2.0f64 * 3.0 * EARTH / 3.0).cbrt();
        }
        let total: f64 = system.iter().map(|p| p.mass_msun).sum();
        let (after, mergers) = settle(system, 1.0);
        assert!(after.len() < 5 && !mergers.is_empty());
        assert_eq!(after.len() + mergers.len(), 5);
        let limit = if after.len() > 2 {
            MULTI_STABILITY_HILL
        } else {
            PAIR_STABILITY_HILL
        };
        for pair in after.windows(2) {
            assert!(separation_in_hill_radii(&pair[0], &pair[1], 1.0) >= limit);
        }
        let kept: f64 = after.iter().map(|p| p.mass_msun).sum();
        assert!((kept / total - 1.0).abs() < 1e-12);
    }

    #[test]
    fn a_lone_pair_only_needs_gladmans_limit() {
        // 6 mutual Hill radii apart: fine for two, not for three.
        let hill = (2.0f64 * EARTH / 3.0).cbrt();
        let two = vec![
            planet(0, 1.0, 1.0),
            planet(1, 1.0 * (1.0 + 6.2 * hill), 1.0),
        ];
        assert!(settle(two.clone(), 1.0).1.is_empty());
        let mut three = two;
        three.push(planet(2, 1.0 * (1.0 + 6.2 * hill).powi(2), 1.0));
        assert!(!settle(three, 1.0).1.is_empty());
    }

    #[test]
    fn the_heavier_body_survives_a_merger() {
        let (after, mergers) = settle(vec![planet(7, 1.0, 1.0), planet(9, 1.01, 5.0)], 1.0);
        assert_eq!(
            mergers,
            vec![Merger {
                survivor: 9,
                absorbed: 7
            }]
        );
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].id, 9);
        assert!(after[0].a_au > 1.0 && after[0].a_au < 1.01);
    }

    proptest! {
        #[test]
        fn settling_conserves_mass_and_ends_stable(
            orbits in proptest::collection::vec((0.05f64..30.0, 0.01f64..400.0), 0..12),
        ) {
            let system: Vec<Orbiter> = orbits
                .iter()
                .zip(0u32..)
                .map(|((a, m), id)| planet(id, *a, *m))
                .collect();
            let total: f64 = system.iter().map(|p| p.mass_msun).sum();
            let n = system.len();
            let (after, mergers) = settle(system, 1.0);
            prop_assert_eq!(after.len() + mergers.len(), n);
            let kept: f64 = after.iter().map(|p| p.mass_msun).sum();
            prop_assert!((kept - total).abs() <= 1e-12 * total.max(1e-30));
            let limit = if after.len() > 2 { MULTI_STABILITY_HILL } else { PAIR_STABILITY_HILL };
            for pair in after.windows(2) {
                prop_assert!(pair[0].a_au <= pair[1].a_au);
                prop_assert!(separation_in_hill_radii(&pair[0], &pair[1], 1.0) >= limit);
            }
        }
    }
}
