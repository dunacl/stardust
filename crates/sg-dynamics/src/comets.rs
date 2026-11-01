//! How many comets a system has, from its belts and planets.
//!
//! The model does not follow small bodies, so this is an estimate from
//! published scalings, anchored on one system, ours. What is a published
//! result and what is an assumption:
//!
//! - **Published**: the line between planets that eject comets and those
//!   that can feed an Oort cloud, and the other lines of the mass and
//!   distance plane (Wyatt et al. 2017, MNRAS 464, 3385, eqs 2, 6, 8); the
//!   number of active Jupiter-family comets (Brasser & Wang 2015, A&A,
//!   about 300, with large uncertainties); the Kuiper belt's mass (Pitjeva
//!   & Pitjev 2018, Celest. Mech. 130); the 30% of Neptune-crossing objects
//!   that become visible comets (Levison & Duncan 1997, Icarus 127, 13);
//!   the slope of the nucleus sizes (Snodgrass et al. 2011, MNRAS 414, 458).
//! - **Assumed**: that the count scales linearly with the mass of the icy
//!   belts; that each ejector passes the same share, so that two pass the
//!   30% above; that an Oort cloud's mass follows the belts' by the
//!   population ratio Brasser & Morbidelli (2013, Icarus 225, 40) simulate,
//!   though observations suggest a far larger ratio. Counts for systems
//!   unlike ours are orders of magnitude, and cards say so.
//! - **Reach**: only belt mass a planet can reach feeds comets: from the
//!   outermost planet of [`LEAST_SHEPHERD_MEARTH`] or more out to its 2:1
//!   resonance ([`reach_au`]). The Kuiper belt ends there with Neptune, at
//!   47.8 AU, which is what keeps the anchor's mass meaningful. Most of the
//!   model's belt mass lies farther out and feeds nothing. The reach rests
//!   on that one system, and the mass floor is a choice.

use sg_core::math;

/// Active Jupiter-family comets in the Solar System with diameters above
/// [`MIN_DIAMETER_KM`] and perihelia inside 2.5 AU (Brasser & Wang 2015).
pub const SOLAR_ACTIVE_COMETS: f64 = 300.0;
/// The size limit of that count, km.
pub const MIN_DIAMETER_KM: f64 = 2.3;
/// Largest nucleus drawn, km; Hale-Bopp's is about this size.
pub const MAX_DIAMETER_KM: f64 = 60.0;
/// The Kuiper belt, M⊕ (Pitjeva & Pitjev 2018).
pub const SOLAR_RESERVOIR_MEARTH: f64 = 0.0197;
/// Ejectors between the Solar System's belt and its inner planets: Jupiter
/// and Saturn.
pub const SOLAR_EJECTORS: u32 = 2;
/// Share of Neptune-crossing objects that become visible comets past those
/// two (Levison & Duncan 1997).
pub const SOLAR_TRANSMISSION: f64 = 0.3;
/// Oort cloud to scattered disk population, as simulated by Brasser &
/// Morbidelli (2013).
pub const OORT_TO_BELT: f64 = 12.0;
/// Perihelion inside which a comet around a Sun counts as visible, AU.
pub const SOLAR_VISIBLE_PERIHELION_AU: f64 = 2.5;

/// Least mass of a planet that shapes a belt beyond it, M⊕. A choice: it
/// keeps small bodies from setting where the reservoir lies.
pub const LEAST_SHEPHERD_MEARTH: f64 = 1.0;

/// Farthest a planet at `a_au` reaches into a belt: its 2:1 mean-motion
/// resonance, at `2^(2/3)` of its distance from Kepler's third law. The
/// Kuiper belt's outer edge lies at this resonance with Neptune (Trujillo &
/// Brown 2001, ApJ 554, L95, for the edge near 48 AU).
pub fn reach_au(a_au: f64) -> f64 {
    a_au * math::pow(2.0, 2.0 / 3.0)
}

/// Mass between `from_au` and `to_au` of a belt given as segments of
/// `(inner_au, outer_au, mass_mearth)`, each taken as even across its width.
pub fn mass_between(segments: &[(f64, f64, f64)], from_au: f64, to_au: f64) -> f64 {
    segments
        .iter()
        .map(|&(inner, outer, mass)| {
            let overlap = (outer.min(to_au) - inner.max(from_au)).max(0.0);
            mass * overlap / (outer - inner).max(1e-12)
        })
        .sum()
}

/// Mass above which a planet ejects comets rather than sending them to an
/// Oort cloud, M⊕ (Wyatt et al. 2017, eq. 6, with their reference stellar
/// density of 0.1 M☉ pc⁻³). Saturn and Jupiter lie above it; Uranus and
/// Neptune below. Valid to about 10⁴ AU and for 0.1–2 M☉.
pub fn ejector_mass_mearth(a_au: f64, star_mass_msun: f64) -> f64 {
    1.5 * math::pow(star_mass_msun, 5.0 / 7.0) * math::pow(a_au, 6.0 / 7.0)
}

/// Whether a planet ejects what crosses its orbit.
pub fn is_ejector(mass_mearth: f64, a_au: f64, star_mass_msun: f64) -> bool {
    mass_mearth > ejector_mass_mearth(a_au, star_mass_msun)
}

/// Least mass that scatters comets within `age_gyr`, M⊕ (Wyatt et al. 2017,
/// eq. 2; the paper gives it to order of magnitude, with no prefactor).
pub fn scattering_mass_mearth(a_au: f64, star_mass_msun: f64, age_gyr: f64) -> f64 {
    math::pow(star_mass_msun * a_au, 0.75) / math::pow(age_gyr, 0.5)
}

/// Mass under which what a planet scatters is out of the reach of the
/// galaxy's tides, so that no Oort cloud forms, M⊕ (Wyatt et al. 2017, eq. 8,
/// reference stellar density).
pub fn least_oort_mass_mearth(a_au: f64, star_mass_msun: f64) -> f64 {
    0.8e-3 * math::pow(star_mass_msun, 0.5) * math::pow(a_au, 1.5)
}

/// Whether a planet can build an Oort cloud in `age_gyr`: heavy enough to
/// scatter comets in that time, and not so heavy that it ejects them.
pub fn feeds_oort_cloud(mass_mearth: f64, a_au: f64, star_mass_msun: f64, age_gyr: f64) -> bool {
    let least = scattering_mass_mearth(a_au, star_mass_msun, age_gyr)
        .max(least_oort_mass_mearth(a_au, star_mass_msun));
    mass_mearth >= least && !is_ejector(mass_mearth, a_au, star_mass_msun)
}

/// Share of comets that pass one ejector: two pass
/// [`SOLAR_TRANSMISSION`]. An assumption, see the module's notes.
fn share_past_an_ejector() -> f64 {
    math::pow(SOLAR_TRANSMISSION, 1.0 / f64::from(SOLAR_EJECTORS))
}

/// Active comets in steady state, scaled from the Solar System: linear in
/// the mass of the icy belts, and less for each ejector on the way in,
/// which is why a chain of planets with no ejector gives the most (Wyatt et
/// al. 2017). Same size limit as [`SOLAR_ACTIVE_COMETS`].
pub fn active_comets(reservoir_mearth: f64, ejectors: u32) -> f64 {
    let shares = f64::from(ejectors) - f64::from(SOLAR_EJECTORS);
    SOLAR_ACTIVE_COMETS
        * (reservoir_mearth / SOLAR_RESERVOIR_MEARTH)
        * math::pow(share_past_an_ejector(), shares)
}

/// Mass of an Oort cloud fed from belts of this mass, M⊕.
pub fn oort_mass_mearth(reservoir_mearth: f64) -> f64 {
    OORT_TO_BELT * reservoir_mearth
}

/// Perihelion inside which a comet is visible, AU: where its star's light
/// is what the Sun's is at [`SOLAR_VISIBLE_PERIHELION_AU`].
pub fn visible_perihelion_au(luminosity_lsun: f64) -> f64 {
    SOLAR_VISIBLE_PERIHELION_AU * math::sqrt(luminosity_lsun)
}

/// Tisserand parameter of an orbit with respect to a planet at `planet_a_au`
/// on a circular orbit; `i_rad` is measured from that planet's plane
/// (Murray & Dermott 1999, eq. 3.45, in the usual approximation). Between 2
/// and 3 with respect to Jupiter is what makes a Jupiter-family comet
/// (Levison 1996).
pub fn tisserand(a_au: f64, e: f64, i_rad: f64, planet_a_au: f64) -> f64 {
    planet_a_au / a_au + 2.0 * math::cos(i_rad) * math::sqrt((a_au / planet_a_au) * (1.0 - e * e))
}

/// A nucleus diameter from a uniform draw `u` in (0, 1], km: the cumulative
/// law N(>D) ∝ D⁻² of Snodgrass et al. (2011, a slope near 1.92) inverted
/// and cut at [`MAX_DIAMETER_KM`].
pub fn nucleus_diameter_km(u: f64) -> f64 {
    (MIN_DIAMETER_KM / math::sqrt(u.max(1.0e-9))).min(MAX_DIAMETER_KM)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Neptune's 2:1 resonance is at 47.8 AU.
    #[test]
    fn the_reach_is_the_two_to_one_resonance() {
        assert!((reach_au(30.1) - 47.8).abs() < 0.05);
    }

    #[test]
    fn only_what_lies_in_range_counts() {
        let belt = [(10.0, 20.0, 1.0), (20.0, 200.0, 18.0)];
        assert!((mass_between(&belt, 15.0, 30.0) - 1.5).abs() < 1e-12);
        assert_eq!(mass_between(&belt, 1.0, 5.0), 0.0);
        assert!((mass_between(&belt, 0.0, 1000.0) - 19.0).abs() < 1e-12);
    }

    #[test]
    fn the_solar_system_divides_as_wyatt_finds() {
        for (mass, a) in [(317.8, 5.2), (95.2, 9.58)] {
            assert!(is_ejector(mass, a, 1.0));
            assert!(!feeds_oort_cloud(mass, a, 1.0, 4.5));
        }
        for (mass, a) in [(14.5, 19.2), (17.1, 30.1)] {
            assert!(!is_ejector(mass, a, 1.0));
            assert!(feeds_oort_cloud(mass, a, 1.0, 4.5));
        }
    }

    /// Wyatt et al.: for a 40 Myr old 2 M☉ star the blue line lies above
    /// the green one, so such a star has no Oort cloud.
    #[test]
    fn a_young_massive_star_has_no_oort_cloud() {
        for a in [1.0, 5.0, 30.0, 100.0] {
            assert!(scattering_mass_mearth(a, 2.0, 0.04) > ejector_mass_mearth(a, 2.0));
            for mass in [0.1, 1.0, 10.0, 100.0, 1000.0] {
                assert!(!feeds_oort_cloud(mass, a, 2.0, 0.04));
            }
        }
    }

    #[test]
    fn the_anchor_comes_back_and_ejectors_cost_comets() {
        let solar = active_comets(SOLAR_RESERVOIR_MEARTH, SOLAR_EJECTORS);
        assert!((solar - SOLAR_ACTIVE_COMETS).abs() < 1e-9);
        let none = active_comets(SOLAR_RESERVOIR_MEARTH, 0);
        assert!((none / solar - 1.0 / SOLAR_TRANSMISSION).abs() < 1e-9);
        assert!(active_comets(SOLAR_RESERVOIR_MEARTH, 3) < solar);
        let double = active_comets(2.0 * SOLAR_RESERVOIR_MEARTH, 2);
        assert!((double / solar - 2.0).abs() < 1e-9);
    }

    /// Comet 2P/Encke has T_J = 3.03, just outside the family; 67P is 2.75
    /// (JPL Small-Body Database).
    #[test]
    fn tisserand_matches_known_comets() {
        let i_67p = 7.04_f64.to_radians();
        assert!((tisserand(3.46, 0.64, i_67p, 5.204) - 2.75).abs() < 0.02);
        let i_encke = 11.8_f64.to_radians();
        assert!((tisserand(2.22, 0.848, i_encke, 5.204) - 3.03).abs() < 0.02);
    }

    #[test]
    fn nuclei_follow_their_law_and_stop_at_the_limit() {
        assert_eq!(nucleus_diameter_km(1.0), MIN_DIAMETER_KM);
        assert!((nucleus_diameter_km(0.25) - 2.0 * MIN_DIAMETER_KM).abs() < 1e-9);
        assert_eq!(nucleus_diameter_km(1e-12), MAX_DIAMETER_KM);
    }

    #[test]
    fn visibility_follows_the_stars_light() {
        assert_eq!(visible_perihelion_au(1.0), SOLAR_VISIBLE_PERIHELION_AU);
        assert_eq!(
            visible_perihelion_au(4.0),
            2.0 * SOLAR_VISIBLE_PERIHELION_AU
        );
    }
}
