//! A run: inject nuclei, grow them, coalesce, until no dust remains.

use crate::bands::Cloud;
use crate::model::{
    critical_mass_msun, dust_density_msun_au3, gas_density_msun_au3, AccretionParams, Reach,
};
use crate::AccretionError;
use sg_core::math;
use sg_core::rng::RngStream;

/// The star and the limits of a run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunSpec {
    pub star_mass_msun: f64,
    pub luminosity_lsun: f64,
    /// Between these bounds all dust must go before the run ends (D p.17).
    pub inner_bound_au: f64,
    pub outer_bound_au: f64,
    /// How far the cloud's material reaches. A nucleus near a bound sweeps
    /// beyond it (Dole's nucleus 2 goes out to 61.6 AU, D p.25).
    pub material_inner_au: f64,
    pub material_outer_au: f64,
}

impl RunSpec {
    /// Dole's bounds of 0.3 and 50 AU (D p.17) in a cloud with no outer edge
    /// of its own: 1000 AU, where his density is negligible, stands for one.
    pub fn single_star(star_mass_msun: f64, luminosity_lsun: f64) -> Self {
        Self {
            star_mass_msun,
            luminosity_lsun,
            inner_bound_au: 0.3,
            outer_bound_au: 50.0,
            material_inner_au: 0.0,
            material_outer_au: 1000.0,
        }
    }

    fn validate(&self) -> Result<(), AccretionError> {
        let star_ok = self.star_mass_msun > 0.0
            && self.star_mass_msun.is_finite()
            && self.luminosity_lsun > 0.0
            && self.luminosity_lsun.is_finite();
        if !star_ok {
            return Err(AccretionError::BadStar);
        }
        let bounds_ok = self.inner_bound_au > 0.0
            && self.inner_bound_au < self.outer_bound_au
            && self.outer_bound_au.is_finite()
            && self.material_inner_au <= self.inner_bound_au
            && self.outer_bound_au <= self.material_outer_au;
        if !bounds_ok {
            return Err(AccretionError::BadBounds);
        }
        Ok(())
    }
}

/// A planet. Mass is kept as dust and gas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Planet {
    /// The number of the nucleus it began as, counted from 1.
    pub id: u32,
    /// Where that nucleus was injected.
    pub origin_a_au: f64,
    /// Planets it has absorbed.
    pub mergers: u32,
    pub a_au: f64,
    pub e: f64,
    pub dust_msun: f64,
    pub gas_msun: f64,
    /// `m_c` on this orbit; a planet above it gathered gas.
    pub critical_mass_msun: f64,
}

impl Planet {
    /// Total mass, M☉.
    pub fn mass_msun(&self) -> f64 {
        self.dust_msun + self.gas_msun
    }

    /// Whether the planet passed its critical mass (D p.25).
    pub fn is_giant(&self) -> bool {
        self.mass_msun() > self.critical_mass_msun
    }
}

/// What happened during a run, in order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    /// Nucleus `id` was injected and grew.
    Grew {
        id: u32,
        a_au: f64,
        e: f64,
        mass_msun: f64,
        /// The band it swept.
        inner_au: f64,
        outer_au: f64,
        /// Whether it passed its critical mass.
        captured_gas: bool,
    },
    /// `absorbed` joined `survivor`, the heavier of the two.
    Coalesced {
        survivor: u32,
        absorbed: u32,
        a_au: f64,
        e: f64,
        mass_msun: f64,
    },
}

/// The outcome of a run.
#[derive(Debug, Clone, PartialEq)]
pub struct Accretion {
    /// The final planets, ordered outward.
    pub planets: Vec<Planet>,
    pub events: Vec<Event>,
    /// Nuclei injected, duds included.
    pub nuclei: u32,
    /// True if `max_nuclei` ran out with dust left between the bounds.
    pub dust_left: bool,
}

/// Draws an orbit: `a = bound·Y₁`, redrawn inside the inner bound, and
/// `ε = 1 − (1 − Y₂)^Q` (D p.15; gap 1).
fn draw_orbit(rng: &mut RngStream, p: &AccretionParams, spec: &RunSpec) -> (f64, f64) {
    let a_au = loop {
        let a_au = spec.outer_bound_au * rng.uniform();
        if a_au >= spec.inner_bound_au {
            break a_au;
        }
    };
    let e = 1.0 - math::pow(1.0 - rng.uniform(), p.eccentricity_exponent);
    (a_au, e)
}

/// Grows a body in place until the mass changes by less than the tolerance
/// (D pp.20–21), then clears what it took from the cloud.
///
/// `m_{i+1}` is what was there before plus what the mass `m_i` reaches, so a
/// merged planet keeps its mass while regrowing. In an untouched cloud this is
/// Dole's closed form with the band width of `Reach` (see the tests).
fn grow(body: &mut Planet, cloud: &mut Cloud, p: &AccretionParams, spec: &RunSpec) -> (f64, f64) {
    let (a_au, e, w) = (body.a_au, body.e, p.cloud_eccentricity);
    let star = spec.star_mass_msun;
    let rho1 = dust_density_msun_au3(p, star, a_au);
    let crit = critical_mass_msun(p, a_au * (1.0 - e), spec.luminosity_lsun);
    body.critical_mass_msun = crit;
    let (start_dust, start_gas) = (body.dust_msun, body.gas_msun);
    let mut mass = body.mass_msun();
    let (lo, hi, gas_rho) = loop {
        let reach = Reach::new(a_au, e, mass, star);
        let (lo, hi) = (reach.swept_inner_au(w), reach.swept_outer_au(w));
        let held = cloud.contents(lo, hi);
        let area = reach.area_au2(a_au);
        let gas_rho = gas_density_msun_au3(rho1, p.gas_dust_ratio, mass, crit);
        let dust = area * held.dust_width_au * rho1;
        // Gas comes along with the dust and not without it (D pp.21, 23, 25).
        let gas = area * held.dust_width_au * gas_rho;
        let next = start_dust + start_gas + dust + gas;
        let change = math::abs(next - mass);
        mass = next;
        body.dust_msun = start_dust + dust;
        body.gas_msun = start_gas + gas;
        if change < p.growth_tolerance * mass {
            break (lo, hi, gas_rho);
        }
    };
    // Of the gas `(K − 1)ρ₁` originally there, the body took `gas_rho` (gap 3).
    let original_gas = (p.gas_dust_ratio - 1.0) * rho1;
    let kept = (1.0 - gas_rho / original_gas).clamp(0.0, 1.0);
    cloud.sweep(lo, hi, kept);
    (lo, hi)
}

/// Whether two planets are close enough to coalesce: one's `x` limits overlap
/// the other's `ε` limits (D pp.22, 25, Event 6; gap 4).
fn close_enough(a: &Planet, b: &Planet, star_mass_msun: f64) -> bool {
    let reach_a = Reach::new(a.a_au, a.e, a.mass_msun(), star_mass_msun);
    let reach_b = Reach::new(b.a_au, b.e, b.mass_msun(), star_mass_msun);
    let x_over_e = |x: &Reach, r: &Reach| {
        x.perihelion_au - x.x_perihelion_au <= r.aphelion_au
            && r.perihelion_au <= x.aphelion_au + x.x_aphelion_au
    };
    x_over_e(&reach_a, &reach_b) || x_over_e(&reach_b, &reach_a)
}

/// Coalesces two planets with no loss of mass, at Dole's `a₃(max)` from energy
/// and with `ε₃` from angular momentum (D p.23).
pub fn coalesce(one: &Planet, two: &Planet) -> Planet {
    let (m1, m2) = (one.mass_msun(), two.mass_msun());
    let total = m1 + m2;
    let a_au = total / (m1 / one.a_au + m2 / two.a_au);
    let l = |m: f64, a: f64, e: f64| m * math::sqrt(a) * math::sqrt(1.0 - e * e);
    let ratio = (l(m1, one.a_au, one.e) + l(m2, two.a_au, two.e)) / (total * math::sqrt(a_au));
    // Rounding can put a circular result a hair above 1.
    let e = math::sqrt((1.0 - ratio * ratio).max(0.0));
    let heavier = if m2 > m1 { two } else { one };
    Planet {
        id: heavier.id,
        origin_a_au: heavier.origin_a_au,
        mergers: one.mergers + two.mergers + 1,
        a_au,
        e,
        dust_msun: one.dust_msun + two.dust_msun,
        gas_msun: one.gas_msun + two.gas_msun,
        critical_mass_msun: 0.0, // set by the regrowth that follows
    }
}

/// Runs the model. The same seed stream gives the same result on every
/// platform; `max_nuclei`, never the clock, bounds the run.
pub fn accrete(
    rng: &mut RngStream,
    p: &AccretionParams,
    spec: &RunSpec,
) -> Result<Accretion, AccretionError> {
    spec.validate()?;
    let mut cloud = Cloud::new(spec.material_inner_au, spec.material_outer_au);
    let mut planets: Vec<Planet> = Vec::new();
    let mut events = Vec::new();
    let mut nuclei = 0;
    while cloud.has_dust(spec.inner_bound_au, spec.outer_bound_au) && nuclei < p.max_nuclei {
        nuclei += 1;
        let (a_au, e) = draw_orbit(rng, p, spec);
        let reach = Reach::new(a_au, e, p.nucleus_mass_msun, spec.star_mass_msun);
        let w = p.cloud_eccentricity;
        if !cloud.has_dust(reach.swept_inner_au(w), reach.swept_outer_au(w)) {
            continue; // a dud (D p.23)
        }
        let mut body = Planet {
            id: nuclei,
            origin_a_au: a_au,
            mergers: 0,
            a_au,
            e,
            dust_msun: p.nucleus_mass_msun,
            gas_msun: 0.0,
            critical_mass_msun: 0.0,
        };
        let (inner_au, outer_au) = grow(&mut body, &mut cloud, p, spec);
        events.push(Event::Grew {
            id: nuclei,
            a_au,
            e,
            mass_msun: body.mass_msun(),
            inner_au,
            outer_au,
            captured_gas: body.is_giant(),
        });
        // Growth completes before coalescence, and a merged body may grow on
        // (D p.22). Oldest planets are tried first (gap 4).
        while let Some(i) = planets
            .iter()
            .position(|other| close_enough(&body, other, spec.star_mass_msun))
        {
            let other = planets.remove(i);
            let mut merged = coalesce(&body, &other);
            grow(&mut merged, &mut cloud, p, spec);
            events.push(Event::Coalesced {
                survivor: merged.id,
                absorbed: if merged.id == body.id {
                    other.id
                } else {
                    body.id
                },
                a_au: merged.a_au,
                e: merged.e,
                mass_msun: merged.mass_msun(),
            });
            body = merged;
        }
        planets.push(body);
    }
    planets.sort_by(|a, b| a.a_au.total_cmp(&b.a_au));
    Ok(Accretion {
        planets,
        events,
        nuclei,
        dust_left: cloud.has_dust(spec.inner_bound_au, spec.outer_bound_au),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sg_core::rng::ModuleId;

    fn nucleus(a_au: f64, e: f64, p: &AccretionParams) -> Planet {
        Planet {
            id: 1,
            origin_a_au: a_au,
            mergers: 0,
            a_au,
            e,
            dust_msun: p.nucleus_mass_msun,
            gas_msun: 0.0,
            critical_mass_msun: 0.0,
        }
    }

    /// The first nucleus of Dole's run 41 meets an untouched cloud, and the
    /// others near-untouched parts: swept limits to about 1% (D pp.25–26).
    #[test]
    fn doles_nuclei_sweep_his_limits() {
        let p = AccretionParams::set3();
        let spec = RunSpec::single_star(1.0, 1.0);
        for (a, e, lo, hi) in [
            (22.8, 0.149, 14.4, 36.5),
            (9.29, 0.007, 6.45, 13.6),
            (0.421, 0.269, 0.248, 0.690),
        ] {
            let mut cloud = Cloud::new(0.0, 1000.0);
            let mut body = nucleus(a, e, &p);
            grow(&mut body, &mut cloud, &p, &spec);
            let reach = Reach::new(a, e, body.mass_msun(), 1.0);
            let (l, h) = (reach.swept_inner_au(0.2), reach.swept_outer_au(0.2));
            assert!((l / lo - 1.0).abs() < 0.01, "{a}: {l} vs {lo}");
            assert!((h / hi - 1.0).abs() < 0.01, "{a}: {h} vs {hi}");
        }
    }

    /// Dole calls the first a giant, the last terrestrial (D pp.25–26).
    #[test]
    fn doles_nuclei_are_the_right_kind() {
        let p = AccretionParams::set3();
        let spec = RunSpec::single_star(1.0, 1.0);
        let kind = |a, e| {
            let mut cloud = Cloud::new(0.0, 1000.0);
            let mut body = nucleus(a, e, &p);
            grow(&mut body, &mut cloud, &p, &spec);
            body.is_giant()
        };
        assert!(kind(22.8, 0.149));
        assert!(!kind(0.421, 0.269));
    }

    /// The band-by-band sum equals Dole's closed form in an untouched cloud.
    #[test]
    fn growth_step_is_doles_closed_form() {
        let p = AccretionParams::set3();
        let (a, e, w) = (5.0, 0.1, p.cloud_eccentricity);
        let m = 1e-6;
        let rho = dust_density_msun_au3(&p, 1.0, a);
        let q = math::pow(m / (1.0 + m), 0.25);
        let closed = 8.0 * core::f64::consts::PI * a * a * a * rho * q / (1.0 - w * w)
            * (e + q + w + w * e * q);
        let reach = Reach::new(a, e, m, 1.0);
        let width = reach.swept_outer_au(w) - reach.swept_inner_au(w);
        let summed = reach.area_au2(a) * width * rho;
        assert!(
            (summed / closed - 1.0).abs() < 1e-12,
            "{summed} vs {closed}"
        );
    }

    /// A heavy planet barely moves on taking a light one (D p.26, Event 9).
    #[test]
    fn coalescence_keeps_mass_and_favours_the_heavy() {
        let p = AccretionParams::set3();
        let heavy = Planet {
            dust_msun: 1e-4,
            gas_msun: 5e-4,
            ..nucleus(9.29, 0.007, &p)
        };
        let light = Planet {
            dust_msun: 1e-7,
            ..nucleus(6.43, 0.229, &p)
        };
        let m = coalesce(&heavy, &light);
        assert_eq!(m.mass_msun(), heavy.mass_msun() + light.mass_msun());
        assert!((m.a_au - 9.28).abs() < 0.01, "{}", m.a_au);
        assert!(m.e < 0.02, "{}", m.e);
    }

    /// Two equal bodies on one circular orbit stay there.
    #[test]
    fn coalescence_of_twins_is_the_identity() {
        let p = AccretionParams::set3();
        let one = Planet {
            dust_msun: 1e-6,
            ..nucleus(2.0, 0.0, &p)
        };
        let m = coalesce(&one, &one);
        assert!((m.a_au - 2.0).abs() < 1e-12 && m.e < 1e-6);
    }

    /// Median of `1 − (1 − Y)^Q` is `1 − 0.5^0.077` = 0.0520 (D p.15).
    #[test]
    fn eccentricity_draw_has_doles_median() {
        let p = AccretionParams::set3();
        let spec = RunSpec::single_star(1.0, 1.0);
        let mut rng = RngStream::new(7, ModuleId::ACCRETION);
        let mut es: Vec<f64> = (0..20_001)
            .map(|_| draw_orbit(&mut rng, &p, &spec).1)
            .collect();
        es.sort_by(f64::total_cmp);
        assert!((es[10_000] - 0.0520).abs() < 0.002, "{}", es[10_000]);
    }

    #[test]
    fn every_orbit_is_inside_the_bounds() {
        let p = AccretionParams::set3();
        let spec = RunSpec::single_star(1.0, 1.0);
        let mut rng = RngStream::new(3, ModuleId::ACCRETION);
        for _ in 0..10_000 {
            let (a, _) = draw_orbit(&mut rng, &p, &spec);
            assert!((0.3..50.0).contains(&a));
        }
    }

    #[test]
    fn a_run_is_repeatable_and_clears_the_dust() {
        let spec = RunSpec::single_star(1.0, 1.0);
        let p = AccretionParams::default();
        let run = |seed| {
            let mut rng = RngStream::new(seed, ModuleId::ACCRETION);
            accrete(&mut rng, &p, &spec).unwrap()
        };
        let a = run(11);
        assert_eq!(a, run(11));
        assert!(!a.dust_left);
        assert!(a.planets.windows(2).all(|w| w[0].a_au <= w[1].a_au));
    }

    #[test]
    fn bad_input_is_an_error() {
        let p = AccretionParams::default();
        let mut rng = RngStream::new(1, ModuleId::ACCRETION);
        let mut spec = RunSpec::single_star(0.0, 1.0);
        assert_eq!(accrete(&mut rng, &p, &spec), Err(AccretionError::BadStar));
        spec = RunSpec::single_star(1.0, 1.0);
        spec.inner_bound_au = 60.0;
        assert_eq!(accrete(&mut rng, &p, &spec), Err(AccretionError::BadBounds));
    }
}
