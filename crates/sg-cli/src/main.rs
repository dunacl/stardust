//! `sg-cli`: run simulations from the command line.
//!
//! ```sh
//! sg-cli simulate --seed 42 --fidelity classic
//! sg-cli simulate --seed 42 --json > system.json
//! sg-cli batch --first-seed 1 --count 1000 --out systems.jsonl
//! sg-cli compare-golden
//! sg-cli hash --seed 42
//! ```

mod golden;
mod validate;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use sg_schema::{
    Body, BodyKind, Fidelity, HierarchyNode, HostRef, Multiplicity, PlanetClass, PresetId,
    SimulationParams, SystemSnapshot, SystemSpec,
};
use sg_sim::{SimulationResult, MODEL_VERSION};
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "sg-cli", version, about = "Stardust command-line simulator")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Simulate one system and print it.
    Simulate {
        #[command(flatten)]
        run: RunArgs,
        /// Random seed (non-zero).
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Print the full result (snapshot, timeline, diagnostics) as JSON.
        #[arg(long)]
        json: bool,
        /// Write to this file instead of standard output.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Simulate many systems; one JSON snapshot per line.
    Batch {
        #[command(flatten)]
        run: RunArgs,
        /// First seed (non-zero).
        #[arg(long, default_value_t = 1)]
        first_seed: u64,
        /// Number of consecutive seeds.
        #[arg(long, default_value_t = 100)]
        count: u64,
        /// Output file (JSON Lines).
        #[arg(long)]
        out: PathBuf,
    },
    /// Time the simulation of consecutive seeds on this machine.
    Bench {
        #[command(flatten)]
        run: RunArgs,
        #[arg(long, default_value_t = 1)]
        first_seed: u64,
        /// Number of consecutive seeds.
        #[arg(long, default_value_t = 100)]
        count: u64,
    },
    /// Compare the Classic tier with the golden outputs of the C reference.
    CompareGolden {
        /// Directory holding manifest.json and the .jsonl files.
        #[arg(long, default_value = "reference/golden")]
        golden: PathBuf,
        /// Relative tolerance for per-seed agreement.
        #[arg(long, default_value_t = 1e-9)]
        tolerance: f64,
        /// Require only statistical agreement. Use on platforms other than
        /// the one the goldens were generated on (PLAN.md D17).
        #[arg(long)]
        statistical_only: bool,
    },
    /// Generate a population of Sun-like systems with the Standard tier and
    /// compare its statistics with exoplanet surveys.
    Validate {
        /// Number of systems.
        #[arg(long, default_value_t = 2000)]
        count: u64,
        #[arg(long, default_value_t = 1)]
        first_seed: u64,
        /// Write the report (Markdown) here instead of standard output.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Exit with an error if any metric is outside its tolerance.
        #[arg(long)]
        strict: bool,
        /// Read model parameters from a JSON file (fields may be omitted),
        /// to try the population with other settings. The star, tier and
        /// seed are still set by this command.
        #[arg(long)]
        params: Option<PathBuf>,
    },
    /// Print the fingerprint of a run; equal on every platform.
    Hash {
        #[command(flatten)]
        run: RunArgs,
        #[arg(long, default_value_t = 1)]
        seed: u64,
    },
}

#[derive(clap::Args)]
struct RunArgs {
    /// Fidelity tier.
    #[arg(long, value_enum, default_value_t = FidelityArg::Classic)]
    fidelity: FidelityArg,
    /// Stellar mass in solar masses, for a single star.
    #[arg(long, default_value_t = 1.0)]
    mass: f64,
    /// Use a preset instead of --mass.
    #[arg(long, value_enum)]
    preset: Option<PresetArg>,
    /// Fix the system age in Gyr (default: sampled).
    #[arg(long)]
    age_gyr: Option<f64>,
    /// Fix the metallicity [Fe/H] in dex (Standard tier; default: sampled).
    #[arg(long, allow_hyphen_values = true)]
    feh: Option<f64>,
    /// Read parameters from a JSON file (fields may be omitted). The
    /// command-line seed still applies; --mass, --age-gyr and --fidelity are
    /// ignored.
    #[arg(long)]
    params: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
enum PresetArg {
    SunLike,
    RedDwarf,
    HotJupiter,
    MetalPoor,
    MassiveAStar,
    Circumbinary,
    STypeBinary,
    HierarchicalTriple,
    Random,
}

#[derive(Clone, Copy, ValueEnum)]
enum FidelityArg {
    Classic,
    Standard,
    High,
}

impl RunArgs {
    fn params(&self, seed: u64) -> Result<SimulationParams> {
        if let Some(path) = &self.params {
            let text =
                fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
            let mut params: SimulationParams = serde_json::from_str(&text)
                .with_context(|| format!("parsing {}", path.display()))?;
            params.seed = seed;
            return Ok(params);
        }
        let fidelity = match self.fidelity {
            FidelityArg::Classic => Fidelity::Classic,
            FidelityArg::Standard => Fidelity::Standard,
            FidelityArg::High => Fidelity::High,
        };
        if let Some(preset) = self.preset {
            let id = match preset {
                PresetArg::SunLike => PresetId::SunLike,
                PresetArg::RedDwarf => PresetId::RedDwarf,
                PresetArg::HotJupiter => PresetId::HotJupiter,
                PresetArg::MetalPoor => PresetId::MetalPoor,
                PresetArg::MassiveAStar => PresetId::MassiveAStar,
                PresetArg::Circumbinary => PresetId::Circumbinary,
                PresetArg::STypeBinary => PresetId::STypeBinary,
                PresetArg::HierarchicalTriple => PresetId::HierarchicalTriple,
                PresetArg::Random => PresetId::Random,
            };
            let mut params = sg_sim::preset_params(id, seed, fidelity);
            params.age_gyr = self.age_gyr.or(params.age_gyr);
            params.metallicity_feh = self.feh.or(params.metallicity_feh);
            return Ok(params);
        }
        Ok(SimulationParams {
            seed,
            fidelity,
            system: SystemSpec::Auto {
                primary_mass_msun: Some(self.mass),
                multiplicity: Some(Multiplicity::Single),
            },
            age_gyr: self.age_gyr,
            metallicity_feh: self.feh,
            ..SimulationParams::default()
        })
    }
}

fn class_name(class: PlanetClass) -> &'static str {
    match class {
        PlanetClass::Asteroids => "Asteroids",
        PlanetClass::Rock => "Rock",
        PlanetClass::Venusian => "Venusian",
        PlanetClass::Terrestrial => "Terrestrial",
        PlanetClass::Martian => "Martian",
        PlanetClass::Water => "Water",
        PlanetClass::Ice => "Ice",
        PlanetClass::OneFace => "One-face",
        PlanetClass::GasDwarf => "Gas dwarf",
        PlanetClass::SubJovian => "Sub-Jovian",
        PlanetClass::Jovian => "Jovian",
        PlanetClass::Unknown => "Unknown",
        PlanetClass::Rocky => "Rocky",
        PlanetClass::SuperEarth => "Super-Earth",
        PlanetClass::LavaWorld => "Lava world",
        PlanetClass::WaterWorld => "Water world",
        PlanetClass::IceWorld => "Ice world",
        PlanetClass::SubNeptune => "Sub-Neptune",
        PlanetClass::IceGiant => "Ice giant",
        PlanetClass::GasGiant => "Gas giant",
        PlanetClass::HotJupiter => "Hot Jupiter",
    }
}

fn result_json(result: &SimulationResult) -> serde_json::Value {
    let d = &result.diagnostics;
    serde_json::json!({
        "snapshot": result.snapshot,
        "timeline": result.timeline,
        "diagnostics": {
            "fidelity": d.fidelity,
            "bit_deterministic": d.bit_deterministic,
            "rng_draws": d.rng_draws,
            "rejected_samples": d.rejected_samples,
            "warnings": d.warnings,
            "result_hash": sg_sim::result_hash(result),
        },
    })
}

fn body_row(index: usize, body: &Body) -> String {
    let temp = body
        .climate
        .as_ref()
        .map_or(body.equilibrium_temp_k, |c| c.surface_temp_k);
    let pressure = body
        .atmosphere
        .as_ref()
        .and_then(|a| a.surface_pressure_mbar)
        .map_or_else(|| "-".to_string(), |p| format!("{:.3}", p / 1013.25));
    let hz = if body.habitability.in_conservative_hz {
        "HZ"
    } else if body.habitability.in_optimistic_hz {
        "hz"
    } else {
        ""
    };
    let day = body
        .rotation
        .period_hours
        .map_or_else(|| "-".to_string(), |hours| format!("{hours:.1}"));
    format!(
        "{:>3} {:<11} {:>8.3} {:>6.3} {:>10.3} {:>9.0} {:>8} {:>9} {:>7.0} {:<2}",
        index + 1,
        class_name(body.class),
        body.orbit.a_au,
        body.orbit.e,
        body.mass_mearth,
        body.radius_km,
        day,
        pressure,
        temp,
        hz
    )
}

/// Lists the stars, indented by their place in the hierarchy.
fn stars(node: &HierarchyNode, depth: usize, out: &mut String) {
    use std::fmt::Write;
    let indent = "  ".repeat(depth);
    match node {
        HierarchyNode::Star(star) => {
            let _ = writeln!(
                out,
                "{indent}Star {}: {} {:.2} Msun, {:.3} Lsun, {:.2} Rsun, {:.0} K, age {:.2} Gyr",
                star.name,
                star.spectral_class,
                star.mass_msun,
                star.luminosity_lsun,
                star.radius_rsun,
                star.teff_k,
                star.age_gyr
            );
        }
        HierarchyNode::Pair { a, b, orbit, .. } => {
            let _ = writeln!(
                out,
                "{indent}Pair: {:.3} AU apart, e {:.2}, period {:.4} years",
                orbit.a_au,
                orbit.e,
                orbit.period_days / 365.25
            );
            stars(a, depth + 1, out);
            stars(b, depth + 1, out);
        }
    }
}

/// Names of the stars under a node, e.g. `Vorus A` or `Vorus A + Vorus B`.
fn star_names(node: &HierarchyNode) -> Vec<&str> {
    match node {
        HierarchyNode::Star(star) => vec![star.name.as_str()],
        HierarchyNode::Pair { a, b, .. } => {
            let mut names = star_names(a);
            names.extend(star_names(b));
            names
        }
    }
}

fn find_host(node: &HierarchyNode, host: HostRef) -> Option<&HierarchyNode> {
    match (node, host) {
        (HierarchyNode::Star(star), HostRef::Star(id)) if star.id == id => Some(node),
        (HierarchyNode::Pair { id, .. }, HostRef::Barycenter(wanted)) if *id == wanted => {
            Some(node)
        }
        (HierarchyNode::Pair { a, b, .. }, _) => find_host(a, host).or_else(|| find_host(b, host)),
        _ => None,
    }
}

fn host_name(snap: &SystemSnapshot, host: HostRef) -> String {
    find_host(&snap.hierarchy, host).map_or_else(|| "?".into(), |node| star_names(node).join(" + "))
}

fn summary(result: &SimulationResult) -> String {
    use std::fmt::Write;
    let snap = &result.snapshot;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Stardust model {MODEL_VERSION}, {:?} tier, hash {}",
        result.diagnostics.fidelity,
        sg_sim::result_hash(result)
    );
    stars(&snap.hierarchy, 0, &mut out);
    for zone in &snap.planet_zones {
        let _ = write!(
            out,
            "\nPlanets of {}: {:.2}-{:.2} AU",
            host_name(snap, zone.host),
            zone.inner_au,
            zone.outer_au
        );
        if let Some(hz) = snap.habitable_zones.iter().find(|hz| hz.host == zone.host) {
            let _ = write!(
                out,
                "; habitable zone {:.2}-{:.2} AU (optimistic {:.2}-{:.2} AU)",
                hz.conservative_inner_au,
                hz.conservative_outer_au,
                hz.optimistic_inner_au,
                hz.optimistic_outer_au
            );
        }
        let _ = writeln!(
            out,
            "\n{:>3} {:<11} {:>8} {:>6} {:>10} {:>9} {:>8} {:>9} {:>7} {:<2}",
            "#", "class", "a (AU)", "e", "mass (ME)", "R (km)", "day (h)", "P (atm)", "T (K)", ""
        );
        let hosted = snap.bodies.iter().filter(|b| b.host == zone.host);
        for (index, body) in hosted.enumerate() {
            let _ = writeln!(out, "{}", body_row(index, body));
        }
    }
    for planet in &snap.bodies {
        if let Some(rings) = &planet.rings {
            let _ = writeln!(
                out,
                "\nRings of {}: {:.0}-{:.0} km ({:.2}-{:.2} planetary radii), optical depth {:.1}",
                planet.name,
                rings.inner_radius_km,
                rings.outer_radius_km,
                rings.inner_radius_km / planet.radius_km,
                rings.outer_radius_km / planet.radius_km,
                rings.optical_depth
            );
        }
        let moons: Vec<&Body> = snap
            .bodies
            .iter()
            .filter(|b| b.host == HostRef::Body(planet.id))
            .collect();
        if moons.is_empty() {
            continue;
        }
        let _ = writeln!(
            out,
            "\nMoons of {}:\n{:>5} {:<10} {:>12} {:>10} {:>10} {:>9}",
            planet.name, "#", "class", "a (km)", "a (radii)", "mass (ME)", "R (km)"
        );
        for (index, moon) in moons.iter().enumerate() {
            let a_km = moon.orbit.a_au * 1.495_978_707e8;
            let _ = writeln!(
                out,
                "{:>5} {:<10} {:>12.0} {:>10.1} {:>10.5} {:>9.0}",
                index + 1,
                class_name(moon.class),
                a_km,
                a_km / planet.radius_km,
                moon.mass_mearth,
                moon.radius_km
            );
        }
    }
    for belt in &snap.belts {
        let _ = writeln!(
            out,
            "\nBelt around {}: {:.2}-{:.2} AU, {:.4} Earth masses",
            host_name(snap, belt.host),
            belt.inner_au,
            belt.outer_au,
            belt.mass_mearth
        );
    }
    let _ = writeln!(
        out,
        "\n{} planets, {} moons, {} formation events, {} keyframes",
        snap.bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Planet)
            .count(),
        snap.bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Moon)
            .count(),
        result.timeline.events.len(),
        result.timeline.keyframes.len()
    );
    for standing in sg_sim::long_term_stability(snap) {
        let _ = writeln!(
            out,
            "long term, planets of {}: {}",
            host_name(snap, standing.host),
            standing.describe()
        );
    }
    for warning in &result.diagnostics.warnings {
        let _ = writeln!(out, "warning: {warning}");
    }
    out
}

fn write_output(out: Option<&PathBuf>, text: &str) -> Result<()> {
    match out {
        Some(path) => fs::write(path, text).with_context(|| format!("writing {}", path.display())),
        None => {
            print!("{text}");
            Ok(())
        }
    }
}

/// The value below which `fraction` of the sorted values lie (nearest
/// rank).
fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    let rank = (fraction * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

/// Times each simulation. The clock is the machine's, so the numbers are
/// not reproducible; the systems are.
fn bench(run: &RunArgs, first_seed: u64, count: u64) -> Result<()> {
    anyhow::ensure!(count > 0, "nothing to time: --count is 0");
    let mut times_ms = Vec::with_capacity(count as usize);
    for seed in first_seed..first_seed + count {
        let params = run.params(seed)?;
        let started = std::time::Instant::now();
        sg_sim::simulate(&params).with_context(|| format!("simulating seed {seed}"))?;
        times_ms.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    times_ms.sort_by(f64::total_cmp);
    let mean = times_ms.iter().sum::<f64>() / times_ms.len() as f64;
    println!(
        "{count} systems on {}/{}: mean {mean:.1} ms, median {:.1} ms, 95th percentile {:.1} ms, \
         slowest {:.1} ms",
        std::env::consts::OS,
        std::env::consts::ARCH,
        percentile(&times_ms, 0.5),
        percentile(&times_ms, 0.95),
        times_ms[times_ms.len() - 1],
    );
    Ok(())
}

fn batch(run: &RunArgs, first_seed: u64, count: u64, out: &Path) -> Result<()> {
    let file = fs::File::create(out).with_context(|| format!("creating {}", out.display()))?;
    let mut writer = BufWriter::new(file);
    let mut planets = 0usize;
    let mut classes: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut in_hz_with_water = 0usize;

    for seed in first_seed..first_seed + count {
        let result = sg_sim::simulate(&run.params(seed)?)
            .with_context(|| format!("simulating seed {seed}"))?;
        serde_json::to_writer(&mut writer, &result.snapshot)?;
        writer.write_all(b"\n")?;
        let is_planet = |b: &&Body| b.kind == BodyKind::Planet;
        planets += result.snapshot.bodies.iter().filter(is_planet).count();
        for body in result.snapshot.bodies.iter().filter(is_planet) {
            *classes.entry(class_name(body.class)).or_insert(0) += 1;
            if body.habitability.in_conservative_hz && body.habitability.surface_liquid_water {
                in_hz_with_water += 1;
            }
        }
    }
    writer.flush()?;

    eprintln!(
        "{count} systems, {planets} planets ({:.1} per system) -> {}",
        planets as f64 / count as f64,
        out.display()
    );
    for (class, n) in &classes {
        eprintln!(
            "  {class:<11} {n:>7} ({:.1}%)",
            100.0 * *n as f64 / planets as f64
        );
    }
    eprintln!("  in the habitable zone with surface water: {in_hz_with_water}");
    Ok(())
}

fn compare_golden(golden_dir: &Path, tolerance: f64, statistical_only: bool) -> Result<()> {
    let reports = golden::compare(golden_dir, tolerance)?;
    print!("{}", golden::render(&reports, tolerance));

    let statistical = reports.iter().all(golden::SetReport::statistical_pass);
    let per_seed = reports.iter().all(golden::SetReport::per_seed_pass);
    println!(
        "\nstatistical agreement: {}\nper-seed agreement:    {}",
        if statistical { "PASS" } else { "FAIL" },
        if per_seed {
            "PASS"
        } else if statistical_only {
            "FAIL (not required: --statistical-only)"
        } else {
            "FAIL"
        }
    );
    if !statistical || !(per_seed || statistical_only) {
        bail!("the Classic tier does not match the golden reference");
    }
    Ok(())
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Simulate {
            run,
            seed,
            json,
            out,
        } => {
            let result = sg_sim::simulate(&run.params(seed)?)?;
            let text = if json {
                let mut text = serde_json::to_string_pretty(&result_json(&result))?;
                text.push('\n');
                text
            } else {
                summary(&result)
            };
            write_output(out.as_ref(), &text)
        }
        Command::Batch {
            run,
            first_seed,
            count,
            out,
        } => batch(&run, first_seed, count, &out),
        Command::Bench {
            run,
            first_seed,
            count,
        } => bench(&run, first_seed, count),
        Command::CompareGolden {
            golden,
            tolerance,
            statistical_only,
        } => compare_golden(&golden, tolerance, statistical_only),
        Command::Validate {
            count,
            first_seed,
            out,
            strict,
            params,
        } => {
            let base: Option<SimulationParams> = match &params {
                Some(path) => {
                    let text = fs::read_to_string(path)
                        .with_context(|| format!("reading {}", path.display()))?;
                    Some(
                        serde_json::from_str(&text)
                            .with_context(|| format!("parsing {}", path.display()))?,
                    )
                }
                None => None,
            };
            let mut systems = Vec::with_capacity(count as usize);
            let (mut total_ms, mut max_ms) = (0.0f64, 0.0f64);
            for seed in first_seed..first_seed + count {
                let mut params = validate::population_params(seed);
                if let Some(base) = &base {
                    params.disk = base.disk.clone();
                    params.formation = base.formation.clone();
                    params.toggles = base.toggles.clone();
                    params.metallicity_feh = base.metallicity_feh;
                }
                let started = std::time::Instant::now();
                let result =
                    sg_sim::simulate(&params).with_context(|| format!("simulating seed {seed}"))?;
                let ms = started.elapsed().as_secs_f64() * 1000.0;
                total_ms += ms;
                max_ms = max_ms.max(ms);
                // The metallicity drawn for the run is recorded by the disk.
                let feh = result
                    .snapshot
                    .disks_at_dispersal
                    .first()
                    .map_or(0.0, |disk| (disk.dust_to_gas / 0.0149).log10());
                systems.push(validate::SystemStats::from_result(&result, feh));
            }
            let text = validate::report(
                &systems,
                first_seed,
                MODEL_VERSION,
                total_ms / count.max(1) as f64,
                max_ms,
            );
            write_output(out.as_ref(), &text)?;
            let failed = validate::metrics(&systems)
                .iter()
                .filter(|m| !m.passes())
                .count();
            eprintln!("{failed} metric(s) outside tolerance");
            if strict && failed > 0 {
                bail!("validation failed");
            }
            Ok(())
        }
        Command::Hash { run, seed } => {
            let result = sg_sim::simulate(&run.params(seed)?)?;
            println!("{}", sg_sim::result_hash(&result));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::percentile;

    #[test]
    fn percentiles_use_the_nearest_rank() {
        let values = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        assert_eq!(percentile(&values, 0.5), 5.0);
        assert_eq!(percentile(&values, 0.95), 10.0);
        assert_eq!(percentile(&values, 0.0), 1.0);
        assert_eq!(percentile(&values, 1.0), 10.0);
        assert_eq!(percentile(&[42.0], 0.5), 42.0);
    }
}
