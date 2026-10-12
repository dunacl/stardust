//! Unit newtypes.
//!
//! Physics code passes these instead of bare `f64`s so that a mass in Earth
//! masses cannot be handed to a function expecting solar masses. The schema
//! (`sg-schema`) stores plain numbers and carries the unit in the field name;
//! convert at that boundary with `.0` / the constructor.

use crate::constants::{DAYS_PER_YEAR, SOLAR_MASS_IN_EARTH_MASSES};

macro_rules! unit {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
        pub struct $name(pub f64);

        impl $name {
            /// The underlying number.
            #[inline]
            pub fn value(self) -> f64 {
                self.0
            }
        }
    };
}

unit!(
    /// Mass in solar masses.
    SolarMass
);
unit!(
    /// Mass in Earth masses.
    EarthMass
);
unit!(
    /// Luminosity in solar luminosities.
    SolarLuminosity
);
unit!(
    /// Radius in solar radii.
    SolarRadius
);
unit!(
    /// Length in astronomical units.
    Au
);
unit!(
    /// Length in kilometres.
    Km
);
unit!(
    /// Temperature in kelvin.
    Kelvin
);
unit!(
    /// Time in gigayears.
    Gyr
);
unit!(
    /// Time in years.
    Years
);
unit!(
    /// Time in days.
    Days
);

impl From<SolarMass> for EarthMass {
    fn from(m: SolarMass) -> Self {
        EarthMass(m.0 * SOLAR_MASS_IN_EARTH_MASSES)
    }
}

impl From<EarthMass> for SolarMass {
    fn from(m: EarthMass) -> Self {
        SolarMass(m.0 / SOLAR_MASS_IN_EARTH_MASSES)
    }
}

impl From<Years> for Gyr {
    fn from(t: Years) -> Self {
        Gyr(t.0 / 1.0e9)
    }
}

impl From<Gyr> for Years {
    fn from(t: Gyr) -> Self {
        Years(t.0 * 1.0e9)
    }
}

impl From<Years> for Days {
    fn from(t: Years) -> Self {
        Days(t.0 * DAYS_PER_YEAR)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mass_conversions_round_trip() {
        let sun: EarthMass = SolarMass(1.0).into();
        assert!((sun.0 - 332_946.0).abs() < 0.1);
        let back: SolarMass = sun.into();
        assert!((back.0 - 1.0).abs() < 1e-15);
    }

    #[test]
    fn time_conversions() {
        assert_eq!(Gyr::from(Years(4.6e9)).0, 4.6);
        assert_eq!(Years::from(Gyr(1.0)).0, 1.0e9);
        assert_eq!(Days::from(Years(1.0)).0, 365.25);
    }
}
