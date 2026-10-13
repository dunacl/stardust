//! The main-sequence fits against the authors' own code.
//!
//! `data/sse_ms_reference.txt` was produced by the SSE package (Hurley, Pols
//! & Tout 2000) for 12 masses, 3 metallicities and 6 ages each; see
//! `reference/sse/README.md`. Agreement is expected to the last few digits:
//! both evaluate the same formulae with the same coefficients.

use sg_core::units::SolarMass;
use sg_stellar::evolution::MainSequence;

const REFERENCE: &str = include_str!("data/sse_ms_reference.txt");
const TOLERANCE: f64 = 1.0e-10;

fn close(got: f64, want: f64) -> bool {
    (got - want).abs() <= TOLERANCE * want.abs()
}

#[test]
fn main_sequence_matches_sse() {
    let mut rows = 0;
    for line in REFERENCE.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<f64> = line
            .split_whitespace()
            .map(|x| x.parse().unwrap())
            .collect();
        let [mass, z, age_myr, t_ms, luminosity, radius, _type] = f[..] else {
            panic!("bad row: {line}");
        };
        let ms = MainSequence::new(z).unwrap();
        let lifetime = ms.lifetime_myr(SolarMass(mass)).unwrap();
        assert!(close(lifetime, t_ms), "t_MS: {line}\n got {lifetime:e}");
        let state = ms.state(SolarMass(mass), age_myr).unwrap();
        assert!(
            close(state.luminosity.0, luminosity),
            "L: {line}\n got {:e}",
            state.luminosity.0
        );
        assert!(
            close(state.radius.0, radius),
            "R: {line}\n got {:e}",
            state.radius.0
        );
        rows += 1;
    }
    assert_eq!(rows, 12 * 3 * 6 + 1);
}
