//! The population against Dole's Tables 3 and 4. Each set there is 40 runs.

use sg_accrete::{accrete, AccretionParams, RunSpec};
use sg_core::rng::{ModuleId, RngStream};

struct Stats {
    planets: f64,
    total_mass_msun: f64,
    largest_mearth: f64,
    spacing: f64,
    bins: [f64; 8],
}

fn stats(p: &AccretionParams, runs: u64) -> Stats {
    let spec = RunSpec::single_star(1.0, 1.0);
    let (mut n, mut total, mut largest, mut sp, mut sp_n) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let mut bins = [0.0; 8];
    for seed in 0..runs {
        let mut rng = RngStream::new(seed, ModuleId::ACCRETION);
        let run = accrete(&mut rng, p, &spec).unwrap();
        assert!(!run.dust_left);
        n += run.planets.len() as f64;
        total += run.planets.iter().map(|b| b.mass_msun()).sum::<f64>();
        let top = run
            .planets
            .iter()
            .map(|b| b.mass_msun())
            .fold(0.0, f64::max);
        largest += top / 3e-6;
        for b in &run.planets {
            let i = (b.mass_msun().log10().floor() + 10.0) as i64;
            if (0..8).contains(&i) {
                bins[i as usize] += 1.0;
            }
        }
        // Spacing excludes bodies below 1e-7 M☉ (D p.38).
        let big: Vec<_> = run
            .planets
            .iter()
            .filter(|b| b.mass_msun() >= 1e-7)
            .collect();
        for w in big.windows(2) {
            sp += w[1].a_au / w[0].a_au;
            sp_n += 1.0;
        }
    }
    let r = runs as f64;
    Stats {
        planets: n / r,
        total_mass_msun: total / r,
        largest_mearth: largest / r,
        spacing: sp / sp_n,
        bins: bins.map(|b| b / r),
    }
}

/// Dole's standard deviation is not printed. His range over 40 runs spans about
/// four of them, so the error of his mean is about `range / (4 √40)`; allow three.
fn within(name: &str, ours: f64, mean: f64, lo: f64, hi: f64) {
    let tolerance = 3.0 * (hi - lo) / (4.0 * 40f64.sqrt());
    assert!(
        (ours - mean).abs() <= tolerance,
        "{name}: {ours:.3} vs {mean} ± {tolerance:.3}"
    );
}

/// Total mass, largest planet and spacing (D Table 3).
#[test]
fn masses_and_spacing_match_doles_table() {
    let s = stats(&AccretionParams::set3(), 2000);
    within("set 3 total", s.total_mass_msun * 1e3, 1.16, 0.58, 1.92);
    within("set 3 largest", s.largest_mearth, 258.0, 90.0, 594.0);
    within("set 3 spacing", s.spacing, 1.73, 1.17, 4.09);
    let s = stats(&AccretionParams::set4(), 2000);
    within("set 4 total", s.total_mass_msun * 1e3, 1.56, 0.43, 3.04);
    within("set 4 largest", s.largest_mearth, 305.0, 63.0, 979.0);
    within("set 4 spacing", s.spacing, 1.84, 1.22, 3.37);
}

/// Known deviation: the mean is about 1.2 planets above Dole's (10.1 and 9.2),
/// well outside the error of his means. This test only holds it inside his
/// ranges, and is not evidence of agreement. See the specification, section 8.
#[test]
fn planet_count_stays_within_doles_range() {
    let s = stats(&AccretionParams::set3(), 2000);
    assert!((7.0..=12.0).contains(&s.planets), "{}", s.planets);
    let s = stats(&AccretionParams::set4(), 2000);
    assert!((7.0..=11.0).contains(&s.planets), "{}", s.planets);
}

/// Most bodies lie between 1e-7 and 1e-3 M☉, spread rather evenly (D p.38).
#[test]
fn mass_bins_have_doles_shape() {
    let b = stats(&AccretionParams::set4(), 2000).bins;
    let middle: f64 = b[3..7].iter().sum();
    assert!(middle / b.iter().sum::<f64>() > 0.8, "{b:?}");
    assert!(b[3..7].iter().all(|&n| n > 0.8 && n < 4.0), "{b:?}");
}
