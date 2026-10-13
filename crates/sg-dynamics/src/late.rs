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

/// A planet as the sweep of leftover planetesimals sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sweeper {
    pub a_au: f64,
    pub mass_msun: f64,
    /// Physical radius, AU.
    pub radius_au: f64,
}

/// What becomes of the planetesimals in one ring after the gas has gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingFate {
    /// Swept up by the planet with this index.
    Accreted(usize),
    /// Flung out of the system by a planet.
    Ejected,
    /// Still there at the system's age: a belt.
    Remains,
}

/// Largest fractional distance at which a planet stirs planetesimals onto
/// orbits that cross its own. Beyond about half its orbital radius the
/// picture of a planet working on its own neighbourhood no longer holds.
const MAX_REACH: f64 = 0.5;

/// Decides the fate of the planetesimals in a ring at `ring_a_au` holding
/// `ring_mass_msun` at surface density `ring_sigma` (M☉/AU²), over
/// `age_yr`, given the planets present.
///
/// A planet stirs the planetesimals near it to random speeds of about its
/// own escape speed (Safronov 1969), which carries them `v_esc/v_K` of the
/// way to its orbit; a ring within that reach belongs to the nearest such
/// planet. If the planet's escape speed exceeds the orbital speed it throws
/// them out of the system instead of accreting them (the Safronov number
/// exceeds one: the giants of the outer Solar System). Otherwise it sweeps
/// the ring at the rate `π R² Σ Ω (1 + v_esc²/v²)` (Safronov 1969; Lissauer
/// 1993, ARA&A 31, 129, eq. 2 with gravitational focusing), with the
/// encounter speed `v` set by how far the ring is from the planet, and the
/// ring is accreted if that takes less than the system's age.
pub fn ring_fate(
    ring_a_au: f64,
    ring_mass_msun: f64,
    ring_sigma: f64,
    planets: &[Sweeper],
    star_mass_msun: f64,
    age_yr: f64,
) -> RingFate {
    const G: f64 = 4.0 * core::f64::consts::PI * core::f64::consts::PI;
    let mut best: Option<(f64, usize, f64)> = None;
    for (index, planet) in planets.iter().enumerate() {
        // (v_esc/v_K)² = 2 (M_p/M★) (a/R).
        let speed_ratio =
            sqrt(2.0 * planet.mass_msun / star_mass_msun * planet.a_au / planet.radius_au);
        let offset = (ring_a_au - planet.a_au).abs() / planet.a_au;
        if offset <= speed_ratio.min(MAX_REACH) && best.map_or(true, |(o, _, _)| offset < o) {
            best = Some((offset, index, speed_ratio));
        }
    }
    let Some((offset, index, speed_ratio)) = best else {
        return RingFate::Remains;
    };
    if speed_ratio > 1.0 {
        return RingFate::Ejected;
    }
    let planet = &planets[index];
    // Encounter speed in units of the orbital speed: the eccentricity that
    // brings the planetesimals to the planet, at least its Hill radius.
    let hill = pow(planet.mass_msun / (3.0 * star_mass_msun), 1.0 / 3.0);
    let encounter = offset.max(hill);
    let focusing = 1.0 + (speed_ratio / encounter) * (speed_ratio / encounter);
    let omega = sqrt(G * star_mass_msun / (ring_a_au * ring_a_au * ring_a_au));
    let rate =
        core::f64::consts::PI * planet.radius_au * planet.radius_au * ring_sigma * omega * focusing;
    if rate > 0.0 && ring_mass_msun / rate < age_yr {
        RingFate::Accreted(index)
    } else {
        RingFate::Remains
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

    const EARTH_RADIUS_AU: f64 = 6378.0 / 1.496e8;

    fn sweeper(a_au: f64, earths: f64, earth_radii: f64) -> Sweeper {
        Sweeper {
            a_au,
            mass_msun: earths * EARTH,
            radius_au: earth_radii * EARTH_RADIUS_AU,
        }
    }

    /// 10 g/cm² in M☉/AU².
    const TEN_G_CM2: f64 = 10.0 / 8.887e6;

    /// The Earth clears its own neighbourhood in tens of millions of years
    /// (v_esc/v_K = 11.2/29.8 = 0.38 of the way to 1 AU), but not a ring at
    /// 2.5 AU, which is the asteroid belt.
    #[test]
    fn earth_sweeps_its_zone_and_leaves_the_asteroid_belt() {
        let earth = [sweeper(1.0, 1.0, 1.0)];
        let ring_mass = 0.1 * EARTH;
        assert_eq!(
            ring_fate(1.2, ring_mass, TEN_G_CM2, &earth, 1.0, 4.6e9),
            RingFate::Accreted(0)
        );
        assert_eq!(
            ring_fate(2.5, ring_mass, TEN_G_CM2, &earth, 1.0, 4.6e9),
            RingFate::Remains
        );
        // Too young a system has not had time.
        assert_eq!(
            ring_fate(1.3, ring_mass, TEN_G_CM2, &earth, 1.0, 1.0e3),
            RingFate::Remains
        );
        assert_eq!(
            ring_fate(1.2, ring_mass, TEN_G_CM2, &[], 1.0, 4.6e9),
            RingFate::Remains
        );
    }

    /// Jupiter's escape speed (60 km/s) is far above its orbital speed
    /// (13 km/s): it ejects what it stirs. A hot Jupiter, orbiting at
    /// 150 km/s, accretes instead.
    #[test]
    fn cold_giants_eject_and_hot_ones_accrete() {
        let jupiter = [sweeper(5.2, 318.0, 11.2)];
        assert_eq!(
            ring_fate(6.5, EARTH, TEN_G_CM2, &jupiter, 1.0, 4.6e9),
            RingFate::Ejected
        );
        // Beyond its reach (half its orbital radius) the ring is untouched.
        assert_eq!(
            ring_fate(9.0, EARTH, TEN_G_CM2, &jupiter, 1.0, 4.6e9),
            RingFate::Remains
        );
        let hot = [sweeper(0.04, 318.0, 11.2)];
        assert_eq!(
            ring_fate(0.045, 0.01 * EARTH, TEN_G_CM2, &hot, 1.0, 4.6e9),
            RingFate::Accreted(0)
        );
    }

    #[test]
    fn a_ring_goes_to_the_nearest_planet_that_reaches_it() {
        let planets = [sweeper(0.7, 0.8, 0.95), sweeper(1.0, 1.0, 1.0)];
        assert_eq!(
            ring_fate(0.8, 0.01 * EARTH, TEN_G_CM2, &planets, 1.0, 4.6e9),
            RingFate::Accreted(0)
        );
        assert_eq!(
            ring_fate(0.95, 0.01 * EARTH, TEN_G_CM2, &planets, 1.0, 4.6e9),
            RingFate::Accreted(1)
        );
    }
}
