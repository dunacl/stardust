//! Validation (PLAN.md §5.10): generate a population of systems and compare
//! its statistics with what surveys of real exoplanets find.
//!
//! The comparison is deliberately coarse. The model's planets are counted
//! as they are, with no simulation of which ones a survey would have
//! detected, and each observed number carries its own uncertainty and
//! selection effects. A metric passes if the model lands inside a stated
//! tolerance band around the observation; the bands are wide, and are part
//! of the report.

use sg_schema::{Body, Fidelity, Multiplicity, SimulationParams, SystemSpec};
use sg_sim::SimulationResult;
use std::fmt::Write as _;

const EARTH_RADIUS_KM: f64 = 6378.1;
/// 0.3 Jupiter masses in Earth masses: the usual lower bound for "giant".
const GIANT_MASS_MEARTH: f64 = 0.3 * 317.8;

/// One system's contribution to the statistics.
#[derive(Debug, Clone, PartialEq)]
pub struct SystemStats {
    pub feh: f64,
    /// `(period_days, radius_rearth, mass_mearth)` of every planet.
    pub planets: Vec<(f64, f64, f64)>,
}

impl SystemStats {
    pub fn from_result(result: &SimulationResult, feh: f64) -> Self {
        Self {
            feh,
            // Planets only: moons are bodies too, and orbit in days.
            planets: result
                .snapshot
                .bodies
                .iter()
                .filter(|b| b.kind == sg_schema::BodyKind::Planet)
                .map(planet)
                .collect(),
        }
    }

    fn any(&self, test: impl Fn(&(f64, f64, f64)) -> bool) -> bool {
        self.planets.iter().any(test)
    }

    /// Has a planet of 1–4 R⊕ inside 100 days: a "Kepler-like" system.
    pub fn has_small_close_planet(&self) -> bool {
        self.any(|(p, r, _)| *p < 100.0 && (1.0..4.0).contains(r))
    }

    /// Has a giant inside 10 days.
    pub fn has_hot_jupiter(&self) -> bool {
        self.any(|(p, _, m)| *p < 10.0 && *m >= GIANT_MASS_MEARTH)
    }

    /// Has a giant with a period of 10 to 2000 days.
    pub fn has_cool_giant(&self) -> bool {
        self.any(|(p, _, m)| (10.0..2000.0).contains(p) && *m >= GIANT_MASS_MEARTH)
    }

    /// A giant of 0.3 Jupiter masses or more between 3 and 7 AU of a
    /// Sun-like star (periods of 1898 to 6765 days): a "Jupiter analogue"
    /// as Wittenmyer et al. (2016) count them.
    pub fn has_jupiter_analogue(&self) -> bool {
        self.planets
            .iter()
            .any(|(p, _, m)| *m >= GIANT_MASS_MEARTH && (1898.0..=6765.0).contains(p))
    }

    pub fn has_giant(&self) -> bool {
        self.any(|(p, _, m)| *p < 2000.0 && *m >= GIANT_MASS_MEARTH)
    }
}

fn planet(body: &Body) -> (f64, f64, f64) {
    (
        body.orbit.period_days,
        body.radius_km / EARTH_RADIUS_KM,
        body.mass_mearth,
    )
}

/// Parameters of the validation population: Sun-like single stars, with
/// metallicity, disk and age left to be drawn.
pub fn population_params(seed: u64) -> SimulationParams {
    SimulationParams {
        seed,
        fidelity: Fidelity::Standard,
        system: SystemSpec::Auto {
            primary_mass_msun: Some(1.0),
            multiplicity: Some(Multiplicity::Single),
        },
        ..SimulationParams::default()
    }
}

/// A comparison of one statistic with its observed value.
#[derive(Debug, Clone, PartialEq)]
pub struct Metric {
    pub name: &'static str,
    pub model: f64,
    /// One-sigma sampling uncertainty of the model value.
    pub model_error: f64,
    pub observed: &'static str,
    pub source: &'static str,
    /// The model passes inside `[low, high]`.
    pub tolerance: (f64, f64),
    pub unit: &'static str,
}

impl Metric {
    pub fn passes(&self) -> bool {
        self.model >= self.tolerance.0 && self.model <= self.tolerance.1
    }
}

/// Fraction and its binomial standard error, in percent.
fn percent(hits: usize, total: usize) -> (f64, f64) {
    if total == 0 {
        return (0.0, 0.0);
    }
    let p = hits as f64 / total as f64;
    (100.0 * p, 100.0 * (p * (1.0 - p) / total as f64).sqrt())
}

/// Planets per star per unit of log radius, in a radius range, among
/// planets inside 100 days.
fn radius_density(systems: &[SystemStats], lo: f64, hi: f64) -> f64 {
    let count = systems
        .iter()
        .flat_map(|s| &s.planets)
        .filter(|(p, r, _)| *p < 100.0 && *r >= lo && *r < hi)
        .count();
    count as f64 / systems.len().max(1) as f64 / (hi / lo).ln()
}

/// Depth of the radius valley: the planet density at 1.5–2.0 R⊕ over the
/// lower of the densities at the two peaks either side (1.0–1.5 and
/// 2.0–3.0 R⊕). Below 1 there is a valley. `None` if either peak is empty.
pub fn radius_valley_ratio(systems: &[SystemStats]) -> Option<f64> {
    let valley = radius_density(systems, 1.5, 2.0);
    let below = radius_density(systems, 1.0, 1.5);
    let above = radius_density(systems, 2.0, 3.0);
    let peak = below.min(above);
    (peak > 0.0).then(|| valley / peak)
}

/// Number of planets of 1–1.75 R⊕ for each one of 1.75–4 R⊕, among planets
/// inside 100 days: the two populations either side of the radius valley.
/// `None` if there are no sub-Neptunes.
pub fn size_class_ratio(systems: &[SystemStats]) -> Option<f64> {
    let count = |lo: f64, hi: f64| {
        systems
            .iter()
            .flat_map(|s| &s.planets)
            .filter(|(p, r, _)| *p < 100.0 && *r >= lo && *r < hi)
            .count() as f64
    };
    let sub_neptunes = count(1.75, 4.0);
    (sub_neptunes > 0.0).then(|| count(1.0, 1.75) / sub_neptunes)
}

/// Planets per star smaller than the Earth (0.5–1 R⊕) inside 100 days.
/// Surveys cannot yet count these reliably, so this is reported, not
/// scored.
pub fn sub_earths_per_star(systems: &[SystemStats]) -> f64 {
    close_in_per_star(systems, 0.5, 1.0)
}

/// Planets per star inside 100 days with radii in `[low, high)` R⊕.
pub fn close_in_per_star(systems: &[SystemStats], low: f64, high: f64) -> f64 {
    let count = systems
        .iter()
        .flat_map(|s| &s.planets)
        .filter(|(p, r, _)| *p < 100.0 && (low..high).contains(r))
        .count();
    count as f64 / systems.len().max(1) as f64
}

/// Where, between 1.25 and 3 R⊕, the model has fewest close-in planets:
/// the bin `(low, high)` with the lowest density per unit log radius, and
/// that density relative to the highest in the range. For describing the
/// model's own valley, wherever it falls.
pub fn deepest_dip(systems: &[SystemStats]) -> Option<((f64, f64), f64)> {
    const EDGES: [f64; 8] = [1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 2.75, 3.0];
    let densities: Vec<((f64, f64), f64)> = EDGES
        .windows(2)
        .map(|w| ((w[0], w[1]), radius_density(systems, w[0], w[1])))
        .collect();
    let highest = densities.iter().map(|d| d.1).fold(0.0f64, f64::max);
    let lowest = densities
        .iter()
        .copied()
        .min_by(|x, y| x.1.total_cmp(&y.1))?;
    (highest > 0.0).then(|| (lowest.0, lowest.1 / highest))
}

/// Computes every metric for a population.
pub fn metrics(systems: &[SystemStats]) -> Vec<Metric> {
    let n = systems.len();
    let count = |test: fn(&SystemStats) -> bool| systems.iter().filter(|s| test(s)).count();

    let (small, small_err) = percent(count(SystemStats::has_small_close_planet), n);
    let (hot, hot_err) = percent(count(SystemStats::has_hot_jupiter), n);
    let (cool, cool_err) = percent(count(SystemStats::has_cool_giant), n);
    let (analogue, analogue_err) = percent(count(SystemStats::has_jupiter_analogue), n);

    let rich: Vec<&SystemStats> = systems.iter().filter(|s| s.feh > 0.1).collect();
    let poor: Vec<&SystemStats> = systems.iter().filter(|s| s.feh < -0.1).collect();
    let giant_rate = |group: &[&SystemStats]| {
        group.iter().filter(|s| s.has_giant()).count() as f64 / group.len().max(1) as f64
    };
    let (rate_rich, rate_poor) = (giant_rate(&rich), giant_rate(&poor));
    // With no giants around metal-poor stars the ratio is unbounded; report
    // it against half a system so the number stays finite and conservative.
    let ratio = rate_rich / rate_poor.max(0.5 / poor.len().max(1) as f64);
    let ratio_error = if rate_rich > 0.0 && rate_poor > 0.0 {
        ratio
            * (1.0 / (rate_rich * rich.len() as f64) + 1.0 / (rate_poor * poor.len() as f64)).sqrt()
    } else {
        f64::NAN
    };

    vec![
        Metric {
            name: "Stars with a planet of 1–4 R⊕ inside 100 days",
            model: small,
            model_error: small_err,
            observed: "about 30%",
            source: "Zhu et al. 2018, ApJ 860, 101 (30 ± 3% of Sun-like stars)",
            tolerance: (15.0, 60.0),
            unit: "%",
        },
        Metric {
            name: "Stars with a hot Jupiter (≥ 0.3 M_J inside 10 days)",
            model: hot,
            model_error: hot_err,
            observed: "0.4–1.2%",
            source: "Howard et al. 2012, ApJS 201, 15; Wright et al. 2012, ApJ 753, 160",
            tolerance: (0.2, 3.0),
            unit: "%",
        },
        Metric {
            name: "Stars with a giant (≥ 0.3 M_J) at 10–2000 days",
            model: cool,
            model_error: cool_err,
            observed: "about 10%",
            source: "Cumming et al. 2008, PASP 120, 531 (10.5% for 0.3–10 M_J inside 2000 days)",
            tolerance: (4.0, 25.0),
            unit: "%",
        },
        Metric {
            name: "Stars with a Jupiter analogue (≥ 0.3 M_J at 3–7 AU)",
            model: analogue,
            model_error: analogue_err,
            observed: "about 6%",
            source: "Wittenmyer et al. 2016, ApJ 819, 28 (6.2 +2.8/−1.6% of Sun-like stars)",
            tolerance: (2.0, 15.0),
            unit: "%",
        },
        Metric {
            name: "Giant-planet rate, [Fe/H] > +0.1 over [Fe/H] < −0.1",
            model: ratio,
            model_error: ratio_error,
            observed: "3 or more",
            source: "Fischer & Valenti 2005, ApJ 622, 1102 (rate ∝ 10^(2[Fe/H]))",
            tolerance: (2.0, f64::INFINITY),
            unit: "×",
        },
        Metric {
            name: "Radius valley: planets at 1.5–2.0 R⊕ relative to the peaks either side",
            model: radius_valley_ratio(systems).unwrap_or(f64::NAN),
            model_error: f64::NAN,
            observed: "about 0.5",
            source: "Fulton et al. 2017, AJ 154, 109 (a deficit of about a factor of two)",
            tolerance: (0.0, 0.8),
            unit: "",
        },
        Metric {
            name: "Super-Earths (1–1.75 R⊕) per sub-Neptune (1.75–4 R⊕), inside 100 days",
            model: size_class_ratio(systems).unwrap_or(f64::NAN),
            model_error: f64::NAN,
            observed: "about 1",
            source: "Fulton et al. 2017, AJ 154, 109 (two peaks of similar height)",
            tolerance: (0.5, 2.0),
            unit: "",
        },
    ]
}

fn histogram(title: &str, edges: &[f64], values: &[f64], per: usize, out: &mut String) {
    let _ = writeln!(out, "\n### {title}\n\n```");
    let counts: Vec<usize> = edges
        .windows(2)
        .map(|w| values.iter().filter(|v| **v >= w[0] && **v < w[1]).count())
        .collect();
    let most = counts.iter().copied().max().unwrap_or(0).max(1);
    for (w, count) in edges.windows(2).zip(&counts) {
        let bar = "#".repeat(count * 50 / most);
        let _ = writeln!(
            out,
            "{:>8.2} – {:<8.2} {:>7.3}  {bar}",
            w[0],
            w[1],
            *count as f64 / per.max(1) as f64
        );
    }
    let _ = writeln!(out, "```\n\nCounts are planets per star.");
}

/// The report, as Markdown.
pub fn report(
    systems: &[SystemStats],
    first_seed: u64,
    model_version: &str,
    mean_ms: f64,
    max_ms: f64,
) -> String {
    let n = systems.len();
    let all = metrics(systems);
    let passed = all.iter().filter(|m| m.passes()).count();
    let planets: usize = systems.iter().map(|s| s.planets.len()).sum();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# Validation report: Standard tier, model {model_version}\n"
    );
    let _ = writeln!(
        out,
        "{n} Sun-like single stars (seeds {first_seed}–{}), with metallicity, disk mass, disk \
         lifetime and age drawn from the model's distributions. {planets} planets in all, \
         {:.1} per star.\n",
        first_seed + n as u64 - 1,
        planets as f64 / n.max(1) as f64
    );
    let _ = writeln!(
        out,
        "**{passed} of {} metrics inside tolerance.**\n",
        all.len()
    );
    let _ = writeln!(
        out,
        "The model's planets are counted as generated, with no simulation of what a survey \
         would detect. Tolerances are wide on purpose: they test whether the model is in the \
         right regime, not whether it fits.\n"
    );
    let _ = writeln!(out, "| Metric | Model | Observed | Tolerance | Result |");
    let _ = writeln!(out, "|---|---|---|---|---|");
    for m in &all {
        let error = if m.model_error.is_finite() {
            format!(" ± {:.2}", m.model_error)
        } else {
            String::new()
        };
        let high = if m.tolerance.1.is_finite() {
            format!("{}–{}{}", m.tolerance.0, m.tolerance.1, m.unit)
        } else {
            format!("≥ {}{}", m.tolerance.0, m.unit)
        };
        let _ = writeln!(
            out,
            "| {} | {:.2}{}{error} | {} | {high} | {} |",
            m.name,
            m.model,
            m.unit,
            m.observed,
            if m.passes() { "pass" } else { "**fail**" }
        );
    }
    if let Some(((low, high), depth)) = deepest_dip(systems) {
        let _ = writeln!(
            out,
            "\nFor reference, between 1.25 and 3 R⊕ the model has fewest close-in planets at \
             {low}–{high} R⊕, where their density is {depth:.2} of the highest in that range. \
             The observed valley is centred near 1.8 R⊕."
        );
    }
    let _ = writeln!(
        out,
        "\nNot scored: the model has {:.1} planets smaller than the Earth (0.5–1 R⊕) inside 100 \
         days per star, formed where they are from local planetesimals. Surveys are too \
         incomplete at these sizes to give a firm number; it is probably too many (the Solar \
         System has one, Mercury).",
        sub_earths_per_star(systems)
    );
    let _ = writeln!(
        out,
        "\nNot scored: the model has {:.2} planets of 1.25–1.75 R⊕ inside 100 days per star, \
         against {:.2} of 1–1.25 R⊕. Surveys find the lower of the two size peaks at about \
         1.3 R⊕ (Fulton et al. 2017), so the model's rocky planets are too small: it makes \
         few bare cores of 2–5 Earth masses.",
        close_in_per_star(systems, 1.25, 1.75),
        close_in_per_star(systems, 1.0, 1.25),
    );
    let _ = writeln!(out, "\nSources:\n");
    for m in &all {
        let _ = writeln!(out, "- {}: {}.", m.name, m.source);
    }
    let _ = writeln!(
        out,
        "\n## Speed\n\nMean {mean_ms:.1} ms per system, slowest {max_ms:.1} ms, on the machine \
         that produced this report (one thread). The target is under 2 s on a mid-range phone; \
         that has not been measured."
    );

    let radii: Vec<f64> = systems
        .iter()
        .flat_map(|s| &s.planets)
        .filter(|(p, _, _)| *p < 100.0)
        .map(|(_, r, _)| *r)
        .collect();
    let _ = writeln!(out, "\n## Distributions");
    histogram(
        "Radii of planets inside 100 days (R⊕)",
        &[
            0.5, 0.7, 1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 6.0, 10.0, 16.0,
        ],
        &radii,
        n,
        &mut out,
    );
    let periods: Vec<f64> = systems
        .iter()
        .flat_map(|s| &s.planets)
        .map(|(p, _, _)| *p)
        .collect();
    histogram(
        "Orbital periods of all planets (days)",
        &[
            1.0, 3.0, 10.0, 30.0, 100.0, 300.0, 1000.0, 3000.0, 1.0e4, 1.0e5, 1.0e6,
        ],
        &periods,
        n,
        &mut out,
    );
    let masses: Vec<f64> = systems
        .iter()
        .flat_map(|s| &s.planets)
        .map(|(_, _, m)| *m)
        .collect();
    histogram(
        "Masses of all planets (M⊕)",
        &[
            0.01, 0.1, 0.3, 1.0, 3.0, 10.0, 30.0, 100.0, 300.0, 1000.0, 1.0e4,
        ],
        &masses,
        n,
        &mut out,
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system(feh: f64, planets: &[(f64, f64, f64)]) -> SystemStats {
        SystemStats {
            feh,
            planets: planets.to_vec(),
        }
    }

    #[test]
    fn systems_are_sorted_into_the_right_categories() {
        let kepler_like = system(0.0, &[(20.0, 2.5, 6.0)]);
        assert!(kepler_like.has_small_close_planet() && !kepler_like.has_giant());
        let hot = system(0.2, &[(3.0, 12.0, 300.0)]);
        assert!(hot.has_hot_jupiter() && hot.has_giant() && !hot.has_cool_giant());
        let cool = system(0.2, &[(800.0, 11.0, 400.0)]);
        assert!(cool.has_cool_giant() && !cool.has_hot_jupiter());
        // A sub-Saturn, a far giant and a wide small planet count as none.
        let none = system(
            0.0,
            &[(5.0, 6.0, 60.0), (5000.0, 11.0, 400.0), (300.0, 2.0, 4.0)],
        );
        assert!(!none.has_giant() && !none.has_small_close_planet());
    }

    #[test]
    fn metrics_count_fractions_with_binomial_errors() {
        let mut systems = vec![system(0.0, &[]); 60];
        systems.extend(vec![system(0.0, &[(20.0, 2.5, 6.0)]); 40]);
        let all = metrics(&systems);
        assert!((all[0].model - 40.0).abs() < 1e-9);
        assert!((all[0].model_error - 4.899).abs() < 1e-3);
        assert!(all[0].passes());
        // No hot Jupiters: outside tolerance.
        assert_eq!(all[1].model, 0.0);
        assert!(!all[1].passes());
    }

    #[test]
    fn metallicity_ratio_compares_rich_with_poor_stars() {
        let giant = [(500.0, 11.0, 300.0)];
        let mut systems = Vec::new();
        for k in 0..100 {
            systems.push(system(0.2, if k < 20 { &giant } else { &[] }));
            systems.push(system(-0.2, if k < 5 { &giant } else { &[] }));
            systems.push(system(0.0, &giant));
        }
        let all = metrics(&systems);
        let ratio = all
            .iter()
            .find(|m| m.name.starts_with("Giant-planet rate"))
            .expect("the metallicity metric");
        assert!((ratio.model - 4.0).abs() < 1e-9);
        assert!(ratio.passes());
    }

    #[test]
    fn valley_ratio_is_the_dip_between_two_peaks() {
        let mut planets = Vec::new();
        planets.extend(vec![(10.0, 1.3, 2.0); 40]);
        planets.extend(vec![(10.0, 1.7, 3.0); 10]);
        planets.extend(vec![(10.0, 2.4, 6.0); 40]);
        let systems = vec![system(0.0, &planets)];
        let ratio = radius_valley_ratio(&systems).unwrap();
        // Densities per log radius: 40/ln 1.5, 10/ln(4/3), 40/ln 1.5.
        let expected = (10.0 / (2.0f64 / 1.5).ln()) / (40.0 / 1.5f64.ln());
        assert!((ratio - expected).abs() < 1e-9);
        assert!(radius_valley_ratio(&[system(0.0, &[])]).is_none());
    }

    #[test]
    fn size_classes_are_counted_either_side_of_the_valley() {
        let mut planets = vec![(10.0, 1.4, 3.0); 30];
        planets.extend(vec![(10.0, 2.5, 6.0); 20]);
        planets.extend(vec![(10.0, 0.8, 0.4); 50]);
        // Beyond 100 days, and giants, are not counted.
        planets.extend(vec![(300.0, 1.4, 3.0); 9]);
        planets.extend(vec![(10.0, 11.0, 300.0); 9]);
        let systems = vec![system(0.0, &planets), system(0.0, &[])];
        assert!((size_class_ratio(&systems).unwrap() - 1.5).abs() < 1e-12);
        assert!((sub_earths_per_star(&systems) - 25.0).abs() < 1e-12);
        assert!(size_class_ratio(&[system(0.0, &[])]).is_none());
    }

    #[test]
    fn deepest_dip_finds_the_emptiest_bin() {
        let mut planets = Vec::new();
        planets.extend(vec![(10.0, 1.3, 2.0); 40]);
        planets.extend(vec![(10.0, 1.6, 3.0); 30]);
        planets.extend(vec![(10.0, 2.1, 5.0); 3]);
        planets.extend(vec![(10.0, 2.6, 7.0); 40]);
        // Every bin is populated except those left empty on purpose, so add
        // a few planets to the rest.
        for r in [1.85, 2.35, 2.9] {
            planets.extend(vec![(10.0, r, 5.0); 20]);
        }
        let ((low, high), depth) = deepest_dip(&[system(0.0, &planets)]).unwrap();
        assert_eq!((low, high), (2.0, 2.25));
        assert!(depth < 0.2, "{depth}");
        assert!(deepest_dip(&[system(0.0, &[])]).is_none());
    }

    #[test]
    fn report_states_the_outcome() {
        let mut systems = vec![system(0.0, &[(20.0, 2.5, 6.0)]); 4];
        systems.extend(vec![system(0.0, &[]); 6]);
        let text = report(&systems, 1, "9.9.9", 1.5, 3.0);
        assert!(text.contains("model 9.9.9"));
        assert!(text.contains("10 Sun-like single stars (seeds 1–10)"));
        assert!(text.contains("metrics inside tolerance"));
        assert!(text.contains("| pass |") && text.contains("| **fail** |"));
        assert!(text.contains("Not scored"));
    }
}
