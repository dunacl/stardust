//! One timeline for a system's whole life: formation, then its star's.
//!
//! Formation lasts a few million years and the star billions, so the line
//! is stretched: formation takes [`FORMATION_SHARE`] of its length, linear in
//! time, and the star's life the rest, on a logarithmic scale. Pure, so
//! that it can be tested; the slider itself is in `ui.rs`.

/// How much of the line formation takes.
pub const FORMATION_SHARE: f64 = 0.4;

/// Where the system is on the line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Moment {
    /// Still forming, this far in, Myr.
    Forming(f64),
    /// Finished, with its star at this age, Gyr.
    Aging(f64),
}

/// The line of one system.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lifeline {
    /// When formation ends, Myr.
    pub formation_myr: f64,
    /// The star's age where the line of its life begins, Gyr.
    pub first_gyr: f64,
    /// Where the line ends, Gyr.
    pub last_gyr: f64,
}

impl Lifeline {
    pub fn new(formation_myr: f64, youngest_gyr: f64, oldest_gyr: f64) -> Self {
        let first_gyr = (formation_myr / 1000.0).max(youngest_gyr);
        Self {
            formation_myr,
            first_gyr,
            // A line of no length could not be divided by.
            last_gyr: oldest_gyr.max(first_gyr * 1.001),
        }
    }

    /// The moment at a position from 0 to 1.
    pub fn moment(&self, position: f64) -> Moment {
        let position = position.clamp(0.0, 1.0);
        if position < FORMATION_SHARE {
            Moment::Forming(self.formation_myr * position / FORMATION_SHARE)
        } else {
            Moment::Aging(self.age_at((position - FORMATION_SHARE) / (1.0 - FORMATION_SHARE)))
        }
    }

    /// The age a share from 0 to 1 of the way along the star's life.
    pub fn age_at(&self, share: f64) -> f64 {
        self.first_gyr * (self.last_gyr / self.first_gyr).powf(share.clamp(0.0, 1.0))
    }

    /// How far along the star's life an age is, 0 to 1.
    pub fn share_of(&self, age_gyr: f64) -> f64 {
        let span = (self.last_gyr / self.first_gyr).ln();
        ((age_gyr / self.first_gyr).ln() / span).clamp(0.0, 1.0)
    }

    /// The position of a moment.
    pub fn position(&self, moment: Moment) -> f64 {
        match moment {
            Moment::Forming(t_myr) => {
                FORMATION_SHARE * (t_myr / self.formation_myr).clamp(0.0, 1.0)
            }
            Moment::Aging(age_gyr) => {
                FORMATION_SHARE + (1.0 - FORMATION_SHARE) * self.share_of(age_gyr)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line() -> Lifeline {
        Lifeline::new(2.0, 0.01, 9.5)
    }

    #[test]
    fn the_line_runs_from_birth_to_the_stars_end() {
        assert_eq!(line().moment(0.0), Moment::Forming(0.0));
        let Moment::Aging(last) = line().moment(1.0) else {
            panic!("the end of the line is the star's");
        };
        assert!((last - 9.5).abs() < 1e-9);
    }

    #[test]
    fn formation_ends_where_the_stars_life_begins() {
        let Moment::Aging(age) = line().moment(FORMATION_SHARE) else {
            panic!("at the marker the disk is gone");
        };
        assert!((age - 0.01).abs() < 1e-12);
        let Moment::Forming(t) = line().moment(FORMATION_SHARE - 1e-9) else {
            panic!("just before it the system is forming");
        };
        assert!((t - 2.0).abs() < 1e-6);
    }

    #[test]
    fn a_position_survives_the_trip_to_a_moment_and_back() {
        for k in 0..=20 {
            let position = f64::from(k) / 20.0;
            let back = line().position(line().moment(position));
            assert!((back - position).abs() < 1e-9, "{position} {back}");
        }
    }

    #[test]
    fn a_billion_years_takes_less_room_than_formation() {
        let early = line().position(Moment::Aging(0.1)) - line().position(Moment::Aging(0.01));
        let late = line().position(Moment::Aging(9.5)) - line().position(Moment::Aging(8.5));
        assert!(early > 10.0 * late);
    }

    #[test]
    fn a_star_with_no_life_left_still_has_a_line() {
        let short = Lifeline::new(5.0, 0.01, 0.002);
        assert!(short.last_gyr > short.first_gyr);
        assert!(short.position(Moment::Aging(0.002)).is_finite());
    }
}
