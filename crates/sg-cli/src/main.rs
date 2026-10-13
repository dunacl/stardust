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

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use sg_schema::{
    Body, Fidelity, HierarchyNode, HostRef, Multiplicity, PlanetClass, PresetId, SimulationParams,
    SystemSnapshot, SystemSpec,
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
                PresetArg::MassiveAStar => PresetId::MassiveAStar,
                PresetArg::Circumbinary => PresetId::Circumbinary,
                PresetArg::STypeBinary => PresetId::STypeBinary,
                PresetArg::HierarchicalTriple => PresetId::HierarchicalTriple,
                PresetArg::Random => PresetId::Random,
            };
            let mut params = sg_sim::preset_params(id, seed, fidelity);
            params.age_gyr = self.age_gyr.or(params.age_gyr);
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
    format!(
        "{:>3} {:<11} {:>8.3} {:>6.3} {:>10.3} {:>9.0} {:>8.1} {:>9} {:>7.0} {:<2}",
        index + 1,
        class_name(body.class),
        body.orbit.a_au,
        body.orbit.e,
        body.mass_mearth,
        body.radius_km,
        body.rotation.period_hours,
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
    let _ = writeln!(
        out,
        "\n{} bodies, {} formation events, {} keyframes",
        snap.bodies.len(),
        result.timeline.events.len(),
        result.timeline.keyframes.len()
    );
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
        planets += result.snapshot.bodies.len();
        for body in &result.snapshot.bodies {
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
        Command::CompareGolden {
            golden,
            tolerance,
            statistical_only,
        } => compare_golden(&golden, tolerance, statistical_only),
        Command::Hash { run, seed } => {
            let result = sg_sim::simulate(&run.params(seed)?)?;
            println!("{}", sg_sim::result_hash(&result));
            Ok(())
        }
    }
}
