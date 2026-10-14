//! Moons and rings (PLAN.md §5.7).
//!
//! What follows from physics and what is drawn:
//!
//! - **Where a moon can be** is physics: outside the planet's Roche limit
//!   and well inside its Hill sphere, and not around a tidally locked
//!   planet, whose moons spiral in or escape.
//! - **How much mass a giant's moons hold** is a published result: about
//!   one to two ten-thousandths of the planet, set by the balance of
//!   inflow and loss in the disk around the forming planet (Canup & Ward
//!   2006).
//! - **How many moons, their individual masses and their spacing** are
//!   drawn, within what the Solar System's giants show.
//! - **Whether a rocky planet has a large moon** is a probability from
//!   simulations of giant impacts (Elser et al. 2011), and its mass and
//!   distance are drawn.
//! - **Where rings can be** is physics (inside the Roche limit, and of ice
//!   only where ice survives). **Whether a planet has bright rings** is
//!   drawn at one in four, the Solar System's own count: nothing better
//!   is known.
//!
//! Captured irregular moons are not modelled. No moon here has been
//! through any dynamics: these are statistical companions, not the outcome
//! of a simulation of their formation.

use sg_core::math::{cbrt, pow, sqrt};
use sg_core::rng::RngStream;

/// Density of water ice, g/cm³.
const ICE_DENSITY_G_CM3: f64 = 0.92;
/// Earth's mean density, g/cm³.
const EARTH_DENSITY_G_CM3: f64 = 5.514;
const EARTH_RADIUS_KM: f64 = 6378.0;
const KM_PER_AU: f64 = 1.495_978_707e8;
const SOLAR_MASS_IN_EARTH_MASSES: f64 = 332_946.0;

/// Radius of a planet's Hill sphere, km, at the pericentre of its orbit:
/// `a(1 − e)·(m/(3M))^(1/3)` (Hamilton & Burns 1992, Icarus 96, 43).
pub fn hill_radius_km(a_au: f64, e: f64, planet_mearth: f64, star_msun: f64) -> f64 {
    a_au * (1.0 - e.clamp(0.0, 0.99))
        * cbrt(planet_mearth / SOLAR_MASS_IN_EARTH_MASSES / (3.0 * star_msun))
        * KM_PER_AU
}

/// Fraction of the Hill radius within which a prograde moon on a circular
/// orbit is stable: 0.4895 (Domingos, Winter & Yokoyama 2006, MNRAS 373,
/// 1227, eq. 1, for a planet on a circular orbit; the planet's own
/// eccentricity is allowed for in [`hill_radius_km`]).
pub const STABLE_HILL_FRACTION: f64 = 0.4895;

/// Distance, in planetary radii, inside which a fluid body of density
/// `satellite_g_cm3` is pulled apart by a planet of mean density
/// `planet_g_cm3`: `2.44·(ρ_p/ρ_s)^(1/3)` (Roche 1849; Chandrasekhar 1969,
/// *Ellipsoidal Figures of Equilibrium*). Rings lie inside it, moons
/// outside.
pub fn roche_limit_radii(planet_g_cm3: f64, satellite_g_cm3: f64) -> f64 {
    2.44 * cbrt(planet_g_cm3 / satellite_g_cm3)
}

/// Mean density of a planet, g/cm³, from its mass and radius in Earth
/// units.
pub fn mean_density_g_cm3(mass_mearth: f64, radius_rearth: f64) -> f64 {
    EARTH_DENSITY_G_CM3 * mass_mearth / (radius_rearth * radius_rearth * radius_rearth)
}

/// How a moon came to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Grew in the disk around a forming giant.
    CircumplanetaryDisk,
    /// Debris of a collision between a rocky planet and another body.
    GiantImpact,
}

/// A moon to add to a planet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Moon {
    pub mass_mearth: f64,
    /// Semi-major axis of its orbit about the planet, km.
    pub a_km: f64,
    pub e: f64,
    /// Part of it that is water ice.
    pub water_fraction: f64,
    pub origin: Origin,
}

/// The planet a moon or ring belongs to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Host {
    pub mass_mearth: f64,
    pub radius_rearth: f64,
    /// Its Hill radius, km ([`hill_radius_km`]).
    pub hill_km: f64,
    /// Equilibrium temperature, K.
    pub temp_k: f64,
    pub tidally_locked: bool,
}

impl Host {
    fn radius_km(&self) -> f64 {
        self.radius_rearth * EARTH_RADIUS_KM
    }

    /// Farthest a moon can orbit, km.
    pub fn outer_limit_km(&self) -> f64 {
        STABLE_HILL_FRACTION * self.hill_km
    }
}

/// Total mass of a giant's regular moons as a fraction of the planet's:
/// the range Canup & Ward (2006, Nature 441, 834) find, 1–2.5 × 10⁻⁴. The
/// Galilean moons hold 2.1 × 10⁻⁴ of Jupiter, Saturn's moons 2.5 × 10⁻⁴ of
/// Saturn, Uranus's 1.1 × 10⁻⁴ of Uranus.
pub const REGULAR_MOON_MASS_FRACTION: (f64, f64) = (1.0e-4, 2.5e-4);
/// Innermost regular moon, planetary radii: Io is at 5.9, Mimas at 3.1,
/// Miranda at 5.1.
const REGULAR_INNER_RADII: (f64, f64) = (4.0, 8.0);
/// Ratio of neighbouring moons' distances: the Galilean moons' are 1.59,
/// 1.60 and 1.76; 1.59 is the 2:1 resonance.
const REGULAR_SPACING: (f64, f64) = (1.5, 1.8);
/// Regular moons are found out to a few tens of planetary radii: Callisto
/// at 26, Iapetus at 59, Oberon at 23.
const REGULAR_OUTER_RADII: f64 = 60.0;
/// Below this temperature ice survives, and moons and rings can be icy.
pub const ICE_STABLE_BELOW_K: f64 = 150.0;

/// The regular moons of a giant planet: up to five, sharing the mass
/// fraction of Canup & Ward (2006), spaced outward from a few planetary
/// radii. None around a tidally locked planet (Barnes & O'Brien 2002, ApJ
/// 575, 1087: tides remove the moons of close-in giants), and none that
/// would lie beyond the stability limit, which leaves hot Jupiters bare.
pub fn regular_moons(host: &Host, rng: &mut RngStream) -> Vec<Moon> {
    // Drawn first and always, so that a planet's moons do not depend on
    // which of them fit.
    let wanted = 1 + (rng.uniform() * 5.0) as usize;
    let fraction = rng.uniform_range(REGULAR_MOON_MASS_FRACTION.0, REGULAR_MOON_MASS_FRACTION.1);
    let mut a_radii = rng.uniform_range(REGULAR_INNER_RADII.0, REGULAR_INNER_RADII.1);
    let mut places = Vec::with_capacity(5);
    let mut weights = Vec::with_capacity(5);
    for _ in 0..5 {
        places.push(a_radii);
        // Heavier moons are commoner farther out, as around Jupiter and
        // Saturn: weights between 0.2 and 1.
        weights.push(rng.uniform_range(0.2, 1.0));
        a_radii *= rng.uniform_range(REGULAR_SPACING.0, REGULAR_SPACING.1);
    }
    let eccentricities: Vec<f64> = (0..5).map(|_| rng.uniform_range(0.0, 0.02)).collect();
    if host.tidally_locked {
        return Vec::new();
    }
    let density = mean_density_g_cm3(host.mass_mearth, host.radius_rearth);
    let icy = host.temp_k < ICE_STABLE_BELOW_K;
    let moon_density = if icy { 1.9 } else { 3.3 };
    let inner_limit = roche_limit_radii(density, moon_density);
    let outer_limit = (host.outer_limit_km() / host.radius_km()).min(REGULAR_OUTER_RADII);
    let kept: Vec<usize> = (0..wanted.min(5))
        .filter(|k| places[*k] > inner_limit && places[*k] < outer_limit)
        .collect();
    let total_weight: f64 = kept.iter().map(|k| weights[*k]).sum();
    kept.iter()
        .map(|k| Moon {
            mass_mearth: host.mass_mearth * fraction * weights[*k] / total_weight,
            a_km: places[*k] * host.radius_km(),
            e: eccentricities[*k],
            // Half ice by mass where ice survives, as Ganymede, Callisto
            // and Titan are; rock otherwise, as Io is.
            water_fraction: if icy { 0.5 } else { 0.0 },
            origin: Origin::CircumplanetaryDisk,
        })
        .collect()
}

/// Share of rocky planets that end with a massive moon from a giant
/// impact: 1 in 12, with a range of 1 in 45 to 1 in 4 (Elser, Moore,
/// Stadel & Morishima 2011, Icarus 214, 357).
pub const IMPACT_MOON_PROBABILITY: f64 = 1.0 / 12.0;
/// Mass of such a moon relative to its planet. The Moon is 0.0123; Elser
/// et al. count moons above half a lunar mass.
const IMPACT_MOON_MASS_RATIO: (f64, f64) = (0.003, 0.03);
/// Its distance after tides have pushed it outward, planetary radii: the
/// Moon formed near 3 and is now at 60.
const IMPACT_MOON_RADII: (f64, f64) = (10.0, 60.0);

/// The large moon of a rocky planet, if it has one: it must have been
/// through at least one collision, not be tidally locked (Sasaki, Barnes &
/// O'Brien 2012, ApJ 754, 51), and have room inside its Hill sphere.
pub fn impact_moon(host: &Host, collisions: u32, rng: &mut RngStream) -> Option<Moon> {
    let lucky = rng.uniform() < IMPACT_MOON_PROBABILITY;
    let ratio = IMPACT_MOON_MASS_RATIO.0
        * pow(
            IMPACT_MOON_MASS_RATIO.1 / IMPACT_MOON_MASS_RATIO.0,
            rng.uniform(),
        );
    let a_radii = rng.uniform_range(IMPACT_MOON_RADII.0, IMPACT_MOON_RADII.1);
    let e = rng.uniform_range(0.0, 0.08);
    if !lucky || collisions == 0 || host.tidally_locked {
        return None;
    }
    let outer_limit = host.outer_limit_km() / host.radius_km();
    // Outside the Roche limit by construction (it is under 3 radii for
    // rock); inside the stability limit or not at all.
    (a_radii < outer_limit).then(|| Moon {
        mass_mearth: host.mass_mearth * ratio,
        a_km: a_radii * host.radius_km(),
        e,
        water_fraction: 0.0,
        origin: Origin::GiantImpact,
    })
}

/// Rings bright enough to see.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ring {
    pub inner_km: f64,
    pub outer_km: f64,
    /// Optical depth looking straight through: Saturn's B ring is 1–2, its
    /// A ring about 0.5, its C ring 0.1.
    pub optical_depth: f64,
}

/// Share of cold giants given bright rings: one of the Solar System's four
/// has them. A count of four is all there is to go on.
pub const BRIGHT_RING_PROBABILITY: f64 = 0.25;

/// Bright rings of ice around a giant, if it has them. They need ice to
/// survive (so a cold planet), lie between 1.2 planetary radii and the
/// Roche limit for ice, and need that limit to be inside the Hill sphere.
/// Whether a planet that could have them does is drawn.
pub fn rings(host: &Host, rng: &mut RngStream) -> Option<Ring> {
    let lucky = rng.uniform() < BRIGHT_RING_PROBABILITY;
    let optical_depth = rng.uniform_range(0.3, 1.5);
    if !lucky || host.temp_k >= ICE_STABLE_BELOW_K {
        return None;
    }
    let density = mean_density_g_cm3(host.mass_mearth, host.radius_rearth);
    let outer = roche_limit_radii(density, ICE_DENSITY_G_CM3);
    let inner = 1.2;
    (outer > inner + 0.2 && outer * host.radius_km() < host.outer_limit_km()).then(|| Ring {
        inner_km: inner * host.radius_km(),
        outer_km: outer * host.radius_km(),
        optical_depth,
    })
}

/// Orbital period of a moon about its planet, days: Kepler's third law
/// with the planet's mass.
pub fn moon_period_days(a_km: f64, planet_mearth: f64) -> f64 {
    // G·M⊕ = 3.986 004 × 10⁵ km³/s².
    let mu = 3.986_004e5 * planet_mearth;
    core::f64::consts::TAU * sqrt(a_km * a_km * a_km / mu) / 86_400.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use sg_core::rng::ModuleId;

    fn jupiter() -> Host {
        Host {
            mass_mearth: 317.8,
            radius_rearth: 11.21,
            hill_km: hill_radius_km(5.203, 0.048, 317.8, 1.0),
            temp_k: 110.0,
            tidally_locked: false,
        }
    }

    fn saturn() -> Host {
        Host {
            mass_mearth: 95.2,
            // Mean radius: Saturn is flattened (9.45 at the equator).
            radius_rearth: 9.14,
            hill_km: hill_radius_km(9.537, 0.054, 95.2, 1.0),
            temp_k: 81.0,
            tidally_locked: false,
        }
    }

    fn earth() -> Host {
        Host {
            mass_mearth: 1.0,
            radius_rearth: 1.0,
            hill_km: hill_radius_km(1.0, 0.017, 1.0, 1.0),
            temp_k: 255.0,
            tidally_locked: false,
        }
    }

    #[test]
    fn hill_radii_of_the_earth_and_jupiter() {
        // The Earth's is 1.5 million km (0.01 AU); Jupiter's 0.34 AU at
        // pericentre (0.355 AU at its mean distance).
        assert!((hill_radius_km(1.0, 0.0, 1.0, 1.0) / 1.497e6 - 1.0).abs() < 0.01);
        assert!((jupiter().hill_km / KM_PER_AU - 0.338).abs() < 0.005);
        // The Moon, at 384 400 km, is well inside the stable half.
        assert!(384_400.0 < earth().outer_limit_km());
    }

    #[test]
    fn saturns_rings_end_at_its_roche_limit_for_ice() {
        // Saturn's mean density is 0.687 g/cm³: the limit for ice is 2.21
        // radii. The bright rings end at 2.27 (the A ring's outer edge).
        let density = mean_density_g_cm3(95.2, 9.14);
        assert!((density - 0.687).abs() < 0.01, "{density}");
        let limit = roche_limit_radii(0.687, ICE_DENSITY_G_CM3);
        assert!((limit - 2.21).abs() < 0.02, "{limit}");
        // For the Earth and rock of the Moon's density it is 2.9 radii.
        assert!((roche_limit_radii(5.514, 3.34) - 2.88).abs() < 0.02);
    }

    #[test]
    fn the_moon_takes_a_month() {
        assert!((moon_period_days(384_400.0, 1.0) - 27.45).abs() < 0.2);
        // Io: 421 800 km from Jupiter, 1.77 days.
        assert!((moon_period_days(421_800.0, 317.8) - 1.77).abs() < 0.01);
    }

    #[test]
    fn a_giants_moons_hold_the_published_mass_fraction() {
        let mut most = 0;
        for seed in 1..200 {
            let mut rng = RngStream::new(seed, ModuleId::PLANET);
            let moons = regular_moons(&jupiter(), &mut rng);
            assert!(!moons.is_empty() && moons.len() <= 5);
            most = most.max(moons.len());
            let total: f64 = moons.iter().map(|m| m.mass_mearth).sum();
            let fraction = total / 317.8;
            assert!((1.0e-4..=2.5e-4).contains(&fraction), "{fraction}");
            let radius = 11.21 * EARTH_RADIUS_KM;
            for pair in moons.windows(2) {
                assert!(pair[1].a_km > 1.4 * pair[0].a_km);
            }
            for moon in &moons {
                // Between a few radii and the regular-moon limit, icy at
                // Jupiter's temperature, on near-circular orbits.
                assert!(moon.a_km > 3.9 * radius && moon.a_km < 60.0 * radius);
                assert_eq!(moon.water_fraction, 0.5);
                assert!(moon.e < 0.02 && moon.origin == Origin::CircumplanetaryDisk);
            }
        }
        assert_eq!(most, 5);
    }

    #[test]
    fn hot_and_locked_giants_have_no_moons() {
        // A Jupiter at 0.05 AU: its stable zone ends at 3.6 radii, inside
        // where the first moon would be.
        let hot = Host {
            hill_km: hill_radius_km(0.05, 0.0, 317.8, 1.0),
            temp_k: 1400.0,
            ..jupiter()
        };
        assert!(hot.outer_limit_km() / (11.21 * EARTH_RADIUS_KM) < 4.0);
        let locked = Host {
            tidally_locked: true,
            ..jupiter()
        };
        for seed in 1..50 {
            let mut rng = RngStream::new(seed, ModuleId::PLANET);
            assert!(regular_moons(&hot, &mut rng).is_empty());
            assert!(regular_moons(&locked, &mut rng).is_empty());
        }
        // A warm giant's moons are rock, like Io.
        let warm = Host {
            temp_k: 300.0,
            hill_km: hill_radius_km(1.0, 0.0, 317.8, 1.0),
            ..jupiter()
        };
        let mut rng = RngStream::new(3, ModuleId::PLANET);
        assert!(regular_moons(&warm, &mut rng)
            .iter()
            .all(|m| m.water_fraction == 0.0));
    }

    #[test]
    fn one_rocky_planet_in_twelve_gets_a_large_moon() {
        let mut with_moon = 0;
        let trials = 6000;
        for seed in 1..=trials {
            let mut rng = RngStream::new(seed, ModuleId::PLANET);
            if let Some(moon) = impact_moon(&earth(), 2, &mut rng) {
                with_moon += 1;
                let ratio = moon.mass_mearth;
                assert!((0.003..=0.03).contains(&ratio), "{ratio}");
                assert!(moon.a_km > 9.9 * EARTH_RADIUS_KM && moon.a_km < 60.1 * EARTH_RADIUS_KM);
                assert_eq!(moon.origin, Origin::GiantImpact);
            }
        }
        let share = f64::from(with_moon) / trials as f64;
        assert!((share - 1.0 / 12.0).abs() < 0.012, "{share}");
        // No collision, no moon; nor around a locked planet; nor where the
        // Hill sphere is too small (an Earth at 0.05 AU: 5 radii).
        let close = Host {
            hill_km: hill_radius_km(0.05, 0.0, 1.0, 1.0),
            ..earth()
        };
        let locked = Host {
            tidally_locked: true,
            ..earth()
        };
        for seed in 1..400 {
            let mut rng = RngStream::new(seed, ModuleId::PLANET);
            assert!(impact_moon(&earth(), 0, &mut rng).is_none());
            assert!(impact_moon(&close, 3, &mut rng).is_none());
            assert!(impact_moon(&locked, 3, &mut rng).is_none());
        }
    }

    #[test]
    fn rings_are_icy_inside_the_roche_limit_and_one_in_four() {
        let mut ringed = 0;
        let trials = 4000;
        for seed in 1..=trials {
            let mut rng = RngStream::new(seed, ModuleId::PLANET);
            if let Some(ring) = rings(&saturn(), &mut rng) {
                ringed += 1;
                let radius = 9.14 * EARTH_RADIUS_KM;
                assert!((ring.inner_km / radius - 1.2).abs() < 1e-9);
                // The fluid limit for pure ice is 2.21 mean radii; Saturn's
                // bright rings end at 136 800 km, 2.35 mean radii.
                let outer = ring.outer_km / radius;
                assert!((outer - 2.21).abs() < 0.02, "{outer}");
                assert!((ring.outer_km / 136_800.0 - 1.0).abs() < 0.08);
                assert!((0.3..=1.5).contains(&ring.optical_depth));
            }
        }
        let share = f64::from(ringed) / trials as f64;
        assert!((share - 0.25).abs() < 0.03, "{share}");
        // Too warm for ice: none.
        let warm = Host {
            temp_k: 300.0,
            ..saturn()
        };
        for seed in 1..200 {
            let mut rng = RngStream::new(seed, ModuleId::PLANET);
            assert!(rings(&warm, &mut rng).is_none());
        }
    }

    #[test]
    fn draws_do_not_depend_on_the_outcome() {
        // The stream is left in the same place whether or not moons fit,
        // so one planet's luck does not change the next planet's.
        let hot = Host {
            hill_km: hill_radius_km(0.05, 0.0, 317.8, 1.0),
            ..jupiter()
        };
        let after = |host: &Host| {
            let mut rng = RngStream::new(9, ModuleId::PLANET);
            let _ = regular_moons(host, &mut rng);
            let _ = rings(host, &mut rng);
            let _ = impact_moon(host, 1, &mut rng);
            rng.next_u64()
        };
        assert_eq!(after(&jupiter()), after(&hot));
    }
}
