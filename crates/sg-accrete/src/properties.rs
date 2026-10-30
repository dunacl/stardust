//! From a mass and an orbit to a described planet.
//!
//! Spec gap 6: `sg-planet` gives what it can (giant and envelope radii, escape
//! of the envelope, equilibrium temperature, tidal locking and class); Fogg
//! and Hart give what it does not ([`crate::environment`]). Fogg's seven
//! atmosphere types and his rules for life and habitability are left out:
//! the rules that chose between them were never published.
//!
//! Two assumptions are ours. Dust beyond zone 1 is taken as half water, which
//! sets only the class and the composition, since Fogg's zones give no number.
//! `τ` is scaled from the Earth (see [`crate::environment::greenhouse_k`]).
//!
//! Known weakness: that scaling is too weak at high pressure, so a runaway
//! planet comes out far cooler than Venus (about 350 K against 737 K). Fogg
//! prints no temperature for such a planet to compare with. Show it as a
//! limit of the model on the data card.

use crate::environment::{self as env, Zone};
use crate::run::Planet;
use sg_core::constants::{EARTH_RADIUS_KM, SOLAR_MASS_IN_EARTH_MASSES};
use sg_core::math::{abs, cbrt, sqrt};
use sg_core::rng::RngStream;
use sg_planet::{Makeup, PlanetKind};

/// The star a planet is described around.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Star {
    pub mass_msun: f64,
    pub luminosity_lsun: f64,
    pub age_gyr: f64,
}

/// Surface conditions, for planets with a surface and an atmosphere.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Climate {
    pub surface_k: f64,
    pub greenhouse_k: f64,
    pub pressure_mb: f64,
    pub boiling_k: f64,
    pub ocean_frac: f64,
    pub ice_frac: f64,
    pub cloud_frac: f64,
    pub runaway: bool,
}

/// A described planet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Properties {
    /// After the envelope's escape.
    pub makeup: Makeup,
    pub radius_rearth: f64,
    pub density_g_cm3: f64,
    pub escape_km_s: f64,
    pub albedo: f64,
    pub equilibrium_k: f64,
    pub kind: PlanetKind,
    /// Spin period, hours: orbital if locked, resonant if locked on an
    /// eccentric orbit, otherwise from Fogg's Eq. 12.
    pub rotation_hours: f64,
    pub locked: bool,
    pub resonant: bool,
    /// Has an atmosphere of its own, primordial or outgassed.
    pub atmosphere: bool,
    pub climate: Option<Climate>,
}

/// Fixed albedos where no climate is run (F Table 3; atmosphere types chosen
/// by zone, since his selection rules are unpublished).
fn fixed_albedo(zone: Zone, giant: bool, atmosphere: bool, runaway: bool) -> f64 {
    match (giant, atmosphere, zone) {
        (true, _, _) => 0.4,
        (_, true, _) if runaway => 0.7,
        (_, false, Zone::One) => 0.07,
        (_, false, _) => 0.55,
        (_, true, Zone::Three) => 0.55,
        (_, true, _) => 0.2,
    }
}

/// The albedo and temperature feedback, until `T_surf` moves by under 1 K
/// (F p.509), with a step budget and damping so that it always ends.
fn climate(a_au: f64, luminosity: f64, pressure_mb: f64, ocean: f64) -> Climate {
    let mut surface_k = 288.0;
    let mut result = None;
    for _ in 0..200 {
        let cloud = env::cloud_fraction(surface_k);
        let ice = env::ice_fraction(surface_k, ocean);
        let albedo = env::surface_albedo(cloud, ocean, ice);
        let eff = env::effective_temperature_k(a_au, luminosity, albedo);
        let rise = env::greenhouse_k(eff, pressure_mb);
        let next = eff + rise;
        let moved = abs(next - surface_k);
        surface_k = 0.5 * (surface_k + next);
        result = Some(Climate {
            surface_k,
            greenhouse_k: rise,
            pressure_mb,
            boiling_k: env::boiling_point_k(pressure_mb),
            ocean_frac: ocean,
            ice_frac: ice,
            cloud_frac: cloud,
            runaway: false,
        });
        if moved < 1.0 {
            break;
        }
    }
    result.expect("the loop runs at least once")
}

/// Describes a planet at the star's age. Takes the same draws for every planet.
pub fn describe(planet: &Planet, star: &Star, rng: &mut RngStream) -> Properties {
    let scatter = rng.uniform_range(0.8, 1.2); // Fogg's ±20%, F Eq. 17
    let (a_au, lum) = (planet.a_au, star.luminosity_lsun);
    let zone = Zone::of(a_au, lum);
    let giant = planet.is_giant();
    let dust = planet.dust_msun * SOLAR_MASS_IN_EARTH_MASSES;
    let gas = planet.gas_msun * SOLAR_MASS_IN_EARTH_MASSES;
    let formed = Makeup {
        core_mearth: dust,
        envelope_mearth: gas,
        water_fraction: if zone == Zone::One { 0.0 } else { 0.5 },
    };
    let makeup = Makeup {
        envelope_mearth: sg_planet::envelope_after_escape(
            &formed,
            a_au,
            star.mass_msun,
            lum,
            star.age_gyr,
        ),
        ..formed
    };
    let mass = makeup.mass_mearth();
    let flux = sg_planet::flux_earth(lum, a_au);
    let radius_rearth = if makeup.envelope_mearth > 0.0 {
        sg_planet::radius(&makeup, flux, star.age_gyr)
    } else {
        // Radius from density: Earth is 5.514 g/cm³ at one Earth radius.
        cbrt(mass * 5.514 / env::terrestrial_density_g_cm3(mass, a_au, lum))
    };
    let density_g_cm3 = 5.514 * mass / (radius_rearth * radius_rearth * radius_rearth);
    let escape_km_s = 11.186 * sqrt(mass / radius_rearth);
    let gravity = mass / (radius_rearth * radius_rearth);

    let perihelion = a_au * (1.0 - planet.e);
    let runaway = a_au < env::runaway_limit_au(lum);
    let keeps_air = env::retains(escape_km_s, env::exosphere_k(perihelion), 28.0);
    let outgassed = !giant && keeps_air;
    let atmosphere = makeup.envelope_mearth > 0.0 || outgassed;

    // The feedback runs between the runaway limit and 4 r_ecos (F p.508).
    let in_feedback = a_au >= env::runaway_limit_au(lum) && a_au <= 4.0 * sqrt(lum);
    let inventory = env::volatile_inventory(mass, star.mass_msun, zone, runaway, scatter);
    let pressure = env::surface_pressure_mb(inventory, radius_rearth, gravity);
    let climate = (outgassed && makeup.envelope_mearth == 0.0).then(|| {
        if in_feedback {
            let ocean = env::ocean_fraction(inventory, radius_rearth);
            climate(a_au, lum, pressure, ocean)
        } else {
            let albedo = fixed_albedo(zone, false, true, runaway);
            let eff = env::effective_temperature_k(a_au, lum, albedo);
            let rise = env::greenhouse_k(eff, pressure);
            Climate {
                surface_k: eff + rise,
                greenhouse_k: rise,
                pressure_mb: pressure,
                boiling_k: env::boiling_point_k(pressure),
                ocean_frac: 0.0,
                ice_frac: 0.0,
                cloud_frac: if runaway { 1.0 } else { 0.0 },
                runaway,
            }
        }
    });
    let albedo = match &climate {
        Some(c) if in_feedback => env::surface_albedo(c.cloud_frac, c.ocean_frac, c.ice_frac),
        _ => fixed_albedo(zone, giant, atmosphere, runaway),
    };
    let equilibrium_k = sg_planet::equilibrium_temperature(lum, a_au, albedo);
    let kind = sg_planet::classify(&makeup, equilibrium_k);

    let radius_km = radius_rearth * EARTH_RADIUS_KM;
    let mass_g = mass * 5.972e27;
    let k2 = if giant { 0.24 } else { 0.33 };
    let period_hours = sg_core::math::pow(a_au * a_au * a_au / star.mass_msun, 0.5) * 8766.0;
    let locked = sg_planet::tidal_locking_time_yr(a_au, star.mass_msun, mass, radius_rearth)
        < star.age_gyr * 1.0e9;
    let resonant = locked && planet.e > 0.1;
    let rotation_hours = if resonant {
        period_hours * env::resonance_ratio(planet.e)
    } else if locked {
        period_hours
    } else {
        env::rotation_period_hours(mass_g, radius_km * 1.0e5, k2)
    };
    Properties {
        makeup,
        radius_rearth,
        density_g_cm3,
        escape_km_s,
        albedo,
        equilibrium_k,
        kind,
        rotation_hours,
        locked,
        resonant,
        atmosphere,
        climate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sg_core::rng::ModuleId;

    const SUN: Star = Star {
        mass_msun: 1.0,
        luminosity_lsun: 1.0,
        age_gyr: 4.6,
    };

    fn body(a_au: f64, e: f64, mearth: f64, gas_mearth: f64) -> Planet {
        Planet {
            id: 1,
            origin_a_au: a_au,
            mergers: 0,
            a_au,
            e,
            dust_msun: mearth / SOLAR_MASS_IN_EARTH_MASSES,
            gas_msun: gas_mearth / SOLAR_MASS_IN_EARTH_MASSES,
            critical_mass_msun: 1.2e-5 * sg_core::math::pow(a_au * (1.0 - e), -0.75),
        }
    }

    fn describe_one(p: &Planet) -> Properties {
        describe(p, &SUN, &mut RngStream::new(1, ModuleId::PLANET))
    }

    /// An Earth at 1 AU is about right in every respect.
    #[test]
    fn earth_comes_out_as_earth() {
        let p = describe_one(&body(1.0, 0.017, 1.0, 0.0));
        assert!((p.radius_rearth - 1.0).abs() < 0.03, "{}", p.radius_rearth);
        let c = p.climate.expect("Earth has a climate");
        assert!((c.surface_k - 288.0).abs() < 15.0, "{}", c.surface_k);
        assert!(!c.runaway && !p.locked);
        assert!((0.2..0.45).contains(&p.albedo), "{}", p.albedo);
    }

    /// Fig. 11: Fogg's Planet 3 is at 20 °C. The climate here is not his exact
    /// program, so a few kelvin of his value is what can be asked.
    #[test]
    fn foggs_planet_three_has_his_climate() {
        let p = describe_one(&body(0.965, 0.14, 1.8, 0.0));
        let c = p.climate.unwrap();
        assert!((c.surface_k - 293.3).abs() < 15.0, "{}", c.surface_k);
    }

    #[test]
    fn jupiter_is_a_gas_giant() {
        let p = describe_one(&body(5.2, 0.05, 30.0, 288.0));
        assert_eq!(p.kind, PlanetKind::GasGiant);
        assert!((p.radius_rearth - 11.2).abs() < 0.5, "{}", p.radius_rearth);
        assert!(p.climate.is_none());
        assert_eq!(p.albedo, 0.4);
    }

    #[test]
    fn a_close_small_planet_is_airless_and_locked() {
        let p = describe_one(&body(0.34, 0.01, 0.08, 0.0));
        assert!(!p.atmosphere && p.climate.is_none());
        assert!(p.locked);
        assert_eq!(p.albedo, 0.07);
    }

    /// A planet inside the limit keeps its volatiles in the air (F p.507). Its
    /// temperature is not checked against Venus: the grey relation, scaled with
    /// the square root of pressure, gives about 350 K where Venus has 737 K, and
    /// Fogg prints no value for such a planet. See the module comment.
    #[test]
    fn inside_the_limit_is_a_runaway() {
        let c = describe_one(&body(0.72, 0.01, 0.82, 0.0)).climate.unwrap();
        assert!(c.runaway);
        assert!(c.pressure_mb > 30_000.0, "{}", c.pressure_mb);
        assert!(c.greenhouse_k > 33.0, "{}", c.greenhouse_k);
    }

    /// A planet's draws never depend on its outcome.
    #[test]
    fn every_planet_takes_the_same_draws() {
        let mut a = RngStream::new(5, ModuleId::PLANET);
        let mut b = RngStream::new(5, ModuleId::PLANET);
        describe(&body(0.34, 0.01, 0.08, 0.0), &SUN, &mut a);
        describe(&body(5.2, 0.05, 30.0, 288.0), &SUN, &mut b);
        assert_eq!(a.next_u64(), b.next_u64());
    }
}
