//! N-body integration of planets around one central mass (PLAN.md §5.7).
//!
//! Units: AU, solar masses, years, so `G = 4π²`. State is heliocentric
//! (positions and velocities relative to the central body).
//!
//! Two integrators share the work:
//!
//! - **A symplectic map** for the regular motion: the democratic
//!   heliocentric splitting of Duncan, Levison & Lee (1998, AJ 116, 2067),
//!   a second-order Wisdom–Holman scheme (Wisdom & Holman 1991, AJ 102,
//!   1528). Each planet follows its Kepler orbit exactly between kicks
//!   from the others, so the energy error is bounded and does not grow
//!   with time, at about twenty steps per shortest orbit.
//! - **A fourth-order Hermite scheme** with an adaptive step (Makino &
//!   Aarseth 1992, PASJ 44, 141) for close encounters, which the map
//!   cannot follow: its kicks assume the planets' pull on each other is
//!   small beside the star's.
//!
//! The switch is global and made at step boundaries: an interval in which
//! any two planets come within a few mutual Hill radii, or any planet
//! passes too close to the star for the map's step, is integrated by
//! Hermite, every other interval by the map. This is simpler than the
//! per-pair changeover of Chambers (1999, MNRAS 304, 793), which splits
//! the Hamiltonian smoothly so that the whole remains symplectic; here
//! symplecticity is given up during encounters, and the energy error then
//! is set by the Hermite scheme's accuracy. For the chaotic, collisional
//! evolution this is used for, that is the regime where individual
//! trajectories are not predictable anyway, and energy conservation is
//! what is checked.
//!
//! Collisions are perfect mergers (mass and momentum conserved, no
//! fragments), the usual first approximation (as in Chambers 2001). A
//! planet is lost to the star inside the star's radius and counted as
//! ejected beyond a given distance on an unbound orbit.
//!
//! Valid for point masses under Newtonian gravity, with planets much
//! lighter than the central body. Not included: general relativity,
//! tides, gas.

use crate::kepler::Elements;
use sg_core::math::{acos, atan2, cbrt, cos, sin, sqrt};
use sg_core::units::Au;

/// The gravitational constant in AU³ M☉⁻¹ yr⁻².
pub const G: f64 = 4.0 * core::f64::consts::PI * core::f64::consts::PI;

type V3 = [f64; 3];

#[inline]
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
#[inline]
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
fn scale(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
#[inline]
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
#[inline]
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
#[inline]
fn norm(a: V3) -> f64 {
    sqrt(dot(a, a))
}

/// The Stumpff functions `c₀…c₃` of `x` (Danby 1988, *Fundamentals of
/// Celestial Mechanics*, §6.9): by series for small `|x|`, reached by
/// quartering `x` and undone with the duplication formulas. Arithmetic
/// only, so the result is the same on every platform.
fn stumpff(mut x: f64) -> [f64; 4] {
    let mut halvings = 0;
    while x.abs() > 0.1 {
        x *= 0.25;
        halvings += 1;
    }
    let mut c2 = (1.0
        - x * (1.0
            - x * (1.0 - x * (1.0 - x * (1.0 - x * (1.0 - x / 182.0) / 132.0) / 90.0) / 56.0)
                / 30.0)
            / 12.0)
        / 2.0;
    let mut c3 = (1.0
        - x * (1.0
            - x * (1.0 - x * (1.0 - x * (1.0 - x * (1.0 - x / 210.0) / 156.0) / 110.0) / 72.0)
                / 42.0)
            / 20.0)
        / 6.0;
    let mut c1 = 1.0 - x * c3;
    let mut c0 = 1.0 - x * c2;
    for _ in 0..halvings {
        c3 = (c2 + c0 * c3) * 0.25;
        c2 = c1 * c1 * 0.5;
        c1 *= c0;
        c0 = 2.0 * c0 * c0 - 1.0;
    }
    [c0, c1, c2, c3]
}

/// Advances a two-body orbit by `dt`: position and velocity relative to a
/// centre of gravitational parameter `mu = G·M`.
///
/// Universal variables (Danby 1988, §6.9): Kepler's equation in the
/// universal anomaly `s`, `r₀·G₁ + (r₀·v₀)·G₂ + μ·G₃ = dt` with
/// `Gₙ = sⁿ·cₙ(β s²)` and `β = 2μ/r₀ − v₀²`, solved by Newton's method and
/// then the Gauss `f` and `g` functions. One formula covers ellipses,
/// parabolas and hyperbolas. If Newton's method does not converge (a step
/// that is long beside a close pericentre passage) the step is halved and
/// done in two parts.
pub fn kepler_step(pos: V3, vel: V3, mu: f64, dt: f64) -> (V3, V3) {
    kepler_step_at_depth(pos, vel, mu, dt, 0)
}

fn kepler_step_at_depth(pos: V3, vel: V3, mu: f64, dt: f64, depth: u32) -> (V3, V3) {
    let r0 = norm(pos);
    let v0_sq = dot(vel, vel);
    let u = dot(pos, vel);
    let beta = 2.0 * mu / r0 - v0_sq;

    // First guess: for a bound orbit, the step as a fraction of the period
    // in the universal anomaly, plus the leading correction for the radial
    // motion; otherwise the straight-line value.
    let mut s = dt / r0 - 0.5 * u * dt * dt / (r0 * r0 * r0);
    let mut converged = false;
    let mut g = [0.0; 4];
    let mut r = r0;
    for _ in 0..16 {
        let c = stumpff(beta * s * s);
        g = [c[0], s * c[1], s * s * c[2], s * s * s * c[3]];
        let f = r0 * g[1] + u * g[2] + mu * g[3] - dt;
        r = r0 * g[0] + u * g[1] + mu * g[2];
        let ds = f / r;
        s -= ds;
        if ds.abs() <= 1.0e-14 * s.abs().max(1.0e-300) {
            converged = true;
            // Evaluate at the converged anomaly.
            let c = stumpff(beta * s * s);
            g = [c[0], s * c[1], s * s * c[2], s * s * s * c[3]];
            r = r0 * g[0] + u * g[1] + mu * g[2];
            break;
        }
    }
    if !(converged && r > 0.0 && r.is_finite()) && depth < 24 {
        let (p, v) = kepler_step_at_depth(pos, vel, mu, 0.5 * dt, depth + 1);
        return kepler_step_at_depth(p, v, mu, 0.5 * dt, depth + 1);
    }
    let f = 1.0 - mu * g[2] / r0;
    let big_g = dt - mu * g[3];
    let f_dot = -mu * g[1] / (r * r0);
    let g_dot = 1.0 - mu * g[2] / r;
    (
        add(scale(pos, f), scale(vel, big_g)),
        add(scale(pos, f_dot), scale(vel, g_dot)),
    )
}

/// Position and velocity relative to the centre for orbital elements
/// (two-body relations; Murray & Dermott 1999, §2.8), with `mu = G·(M+m)`.
/// Bound orbits only, `e < 1`. Same frame as [`crate::kepler::position`].
pub fn state_from_elements(el: Elements, mu: f64) -> (V3, V3) {
    let e = el.e.clamp(0.0, 1.0 - 1.0e-12);
    let a = el.a.0;
    let big_e = crate::kepler::eccentric_anomaly(el.mean_anomaly, e);
    let (se, ce) = (sin(big_e), cos(big_e));
    let root = sqrt(1.0 - e * e);
    let n = sqrt(mu / (a * a * a));
    let r_over_a = 1.0 - e * ce;
    // In the orbital plane.
    let (x, y) = (a * (ce - e), a * root * se);
    let (vx, vy) = (-a * n * se / r_over_a, a * n * root * ce / r_over_a);
    let (so, co) = (sin(el.arg_periapsis), cos(el.arg_periapsis));
    let (si, ci) = (sin(el.i), cos(el.i));
    let (sn, cn) = (sin(el.long_asc_node), cos(el.long_asc_node));
    let rotate = |x: f64, y: f64| {
        let x1 = x * co - y * so;
        let y1 = x * so + y * co;
        [x1 * cn - y1 * ci * sn, x1 * sn + y1 * ci * cn, y1 * si]
    };
    (rotate(x, y), rotate(vx, vy))
}

/// Orbital elements of a bound orbit from position and velocity relative
/// to the centre (Murray & Dermott 1999, §2.8), with `mu = G·(M+m)`.
/// `None` for an unbound or degenerate orbit. For a circular or
/// equatorial orbit, where the node or the pericentre is undefined, the
/// corresponding angle is 0 and the others absorb it.
pub fn elements_from_state(pos: V3, vel: V3, mu: f64) -> Option<Elements> {
    let r = norm(pos);
    let v_sq = dot(vel, vel);
    let energy = 0.5 * v_sq - mu / r;
    // Written so that a NaN counts as "not bound".
    let bound = energy < 0.0 && r > 0.0;
    if !bound {
        return None;
    }
    let a = -0.5 * mu / energy;
    let h = cross(pos, vel);
    let h_len = norm(h);
    let turning = h_len > 0.0;
    if !turning {
        return None;
    }
    // Eccentricity vector: (v × h)/μ − r̂.
    let e_vec = sub(scale(cross(vel, h), 1.0 / mu), scale(pos, 1.0 / r));
    let e = norm(e_vec);
    let i = acos(h[2] / h_len);
    // Node line: ẑ × h.
    let node = [-h[1], h[0], 0.0];
    let node_len = norm(node);
    let long_asc_node = if node_len > 1.0e-12 * h_len {
        atan2(node[1], node[0])
    } else {
        0.0
    };
    // Unit vectors in the orbital plane: toward the node, and 90° on.
    let p = if node_len > 1.0e-12 * h_len {
        scale(node, 1.0 / node_len)
    } else {
        [1.0, 0.0, 0.0]
    };
    let q = scale(cross(h, p), 1.0 / h_len);
    let arg_periapsis = if e > 1.0e-12 {
        atan2(dot(e_vec, q), dot(e_vec, p))
    } else {
        0.0
    };
    // True longitude from the node, then true → eccentric → mean anomaly.
    let theta = atan2(dot(pos, q), dot(pos, p));
    let true_anomaly = theta - arg_periapsis;
    let (sf, cf) = (sin(true_anomaly), cos(true_anomaly));
    let big_e = atan2(sqrt((1.0 - e * e).max(0.0)) * sf, e + cf);
    let mean_anomaly = big_e - e * sin(big_e);
    Some(Elements {
        a: Au(a),
        e,
        i,
        long_asc_node: crate::kepler::wrap_angle(long_asc_node),
        arg_periapsis: crate::kepler::wrap_angle(arg_periapsis),
        mean_anomaly: crate::kepler::wrap_angle(mean_anomaly),
    })
}

/// A planet in the integration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Body {
    /// The caller's identifier; a merged body keeps the heavier one's.
    pub id: u32,
    pub mass_msun: f64,
    /// Physical radius, AU: two bodies merge when they touch.
    pub radius_au: f64,
    /// Relative to the central body, AU.
    pub pos: V3,
    /// Relative to the central body, AU/yr.
    pub vel: V3,
}

/// Something that removed a body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    /// Two bodies collided and merged.
    Merged {
        t_yr: f64,
        survivor: u32,
        absorbed: u32,
        /// Distance from the central body, AU.
        r_au: f64,
    },
    /// A body fell into the central body.
    HitStar { t_yr: f64, id: u32 },
    /// A body left on an unbound orbit.
    Ejected { t_yr: f64, id: u32 },
}

/// How the integration is run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// Steps of the symplectic map per shortest orbit. Twenty keeps its
    /// energy error near `10⁻⁶·(m/M)`-ish levels for well-separated
    /// planets (Wisdom & Holman 1991 used about ten times fewer for the
    /// outer planets; close-in chains need more).
    pub steps_per_orbit: f64,
    /// Planets closer than this many mutual Hill radii are in a close
    /// encounter, handled by the Hermite scheme. Three is where the
    /// map's error grows quickly (Chambers 1999 changes over between
    /// about 1 and 3).
    pub encounter_hill_radii: f64,
    /// Accuracy parameter of the Hermite step, `dt = η·min(|a|/|ȧ|)`.
    pub hermite_eta: f64,
    /// A body farther than this from the centre on an unbound orbit is
    /// ejected, AU.
    pub ejection_distance_au: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            steps_per_orbit: 20.0,
            encounter_hill_radii: 3.0,
            hermite_eta: 0.01,
            ejection_distance_au: 1000.0,
        }
    }
}

/// The map's step is reconsidered every this many intervals.
const STEP_REVIEW_INTERVAL: u64 = 32;

/// Counts of the work done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Work {
    /// Steps of the symplectic map.
    pub map_steps: u64,
    /// Steps of the Hermite scheme.
    pub hermite_steps: u64,
    /// Intervals handed to the Hermite scheme.
    pub intervals: u64,
}

/// Planets around a central mass, being integrated.
#[derive(Debug, Clone, PartialEq)]
pub struct Integration {
    pub star_mass_msun: f64,
    pub star_radius_au: f64,
    pub bodies: Vec<Body>,
    pub t_yr: f64,
    pub events: Vec<Event>,
    pub settings: Settings,
    pub work: Work,
    /// Orbital energy removed by mergers, falls into the star and
    /// ejections so far: with it added back, [`Self::energy`] is constant
    /// to the integrators' accuracy.
    pub energy_removed: f64,
    /// Step of the symplectic map, yr.
    step_yr: f64,
    /// For each pair `(i, j)`, `((mᵢ+mⱼ)/(3M))^(1/3)`: the mass factor of
    /// their mutual Hill radius, which changes only when a body does.
    hill_factor: Vec<f64>,
}

/// Gravitational acceleration of every body and its rate of change
/// ("jerk"), in heliocentric coordinates: the star's direct pull, the
/// planets' pull on each other, and the indirect term from the star's own
/// acceleration toward the planets.
fn accelerations(star_mass: f64, bodies: &[Body]) -> (Vec<V3>, Vec<V3>) {
    let n = bodies.len();
    let mut acc = vec![[0.0; 3]; n];
    let mut jerk = vec![[0.0; 3]; n];
    // The star's acceleration and jerk toward the planets.
    let mut star_acc = [0.0; 3];
    let mut star_jerk = [0.0; 3];
    for (i, b) in bodies.iter().enumerate() {
        let r_sq = dot(b.pos, b.pos);
        let r = sqrt(r_sq);
        let inv_r3 = 1.0 / (r_sq * r);
        let rv = dot(b.pos, b.vel) / r_sq;
        // d/dt (r/|r|³) = v/|r|³ − 3 (r·v) r/|r|⁵.
        let term_a = scale(b.pos, inv_r3);
        let term_j = scale(sub(b.vel, scale(b.pos, 3.0 * rv)), inv_r3);
        acc[i] = scale(term_a, -G * star_mass);
        jerk[i] = scale(term_j, -G * star_mass);
        star_acc = add(star_acc, scale(term_a, G * b.mass_msun));
        star_jerk = add(star_jerk, scale(term_j, G * b.mass_msun));
    }
    for i in 0..n {
        for j in (i + 1)..n {
            let d = sub(bodies[j].pos, bodies[i].pos);
            let dv = sub(bodies[j].vel, bodies[i].vel);
            let d_sq = dot(d, d);
            let dist = sqrt(d_sq);
            let inv_d3 = 1.0 / (d_sq * dist);
            let dd = dot(d, dv) / d_sq;
            let term_a = scale(d, inv_d3);
            let term_j = scale(sub(dv, scale(d, 3.0 * dd)), inv_d3);
            acc[i] = add(acc[i], scale(term_a, G * bodies[j].mass_msun));
            jerk[i] = add(jerk[i], scale(term_j, G * bodies[j].mass_msun));
            acc[j] = sub(acc[j], scale(term_a, G * bodies[i].mass_msun));
            jerk[j] = sub(jerk[j], scale(term_j, G * bodies[i].mass_msun));
        }
    }
    for i in 0..n {
        acc[i] = sub(acc[i], star_acc);
        jerk[i] = sub(jerk[i], star_jerk);
    }
    (acc, jerk)
}

impl Integration {
    /// Sets up an integration from heliocentric positions and velocities.
    pub fn new(
        star_mass_msun: f64,
        star_radius_au: f64,
        bodies: Vec<Body>,
        settings: Settings,
    ) -> Self {
        let mut integration = Self {
            star_mass_msun,
            star_radius_au,
            bodies,
            t_yr: 0.0,
            events: Vec::new(),
            settings,
            work: Work::default(),
            energy_removed: 0.0,
            step_yr: 0.0,
            hill_factor: Vec::new(),
        };
        integration.choose_step();
        integration
    }

    /// The step the map should have for the orbits as they are now: a
    /// fixed fraction of the shortest time any planet takes to round its
    /// pericentre, `2π·√(q³/μ)`, the period of a circular orbit there. For
    /// a circular orbit that is its period; for an elongated one it is
    /// what the kicks must resolve (Wisdom & Holman's map loses accuracy
    /// when a step spans much of a pericentre passage; Rauch & Holman
    /// 1999, AJ 117, 1087). A body on an unbound orbit does not set it.
    fn wanted_step(&self) -> f64 {
        let shortest = self
            .bodies
            .iter()
            .filter_map(|b| {
                let mu = G * (self.star_mass_msun + b.mass_msun);
                let el = elements_from_state(b.pos, b.vel, mu)?;
                let q = el.a.0 * (1.0 - el.e.min(0.999));
                Some(core::f64::consts::TAU * sqrt(q * q * q / mu))
            })
            .fold(f64::INFINITY, f64::min);
        if shortest.is_finite() {
            shortest / self.settings.steps_per_orbit
        } else {
            1.0
        }
    }

    /// Sets the map's step and the pair factors for the bodies as they
    /// are; called at the start and whenever a body is merged or lost.
    fn choose_step(&mut self) {
        // The mutual Hill radius of a pair is this factor times their mean
        // distance from the star: `((m₁+m₂)/(3M))^(1/3)·(r₁+r₂)/2`
        // (Chambers, Wetherill & Boss 1996, Icarus 119, 261, with
        // distances for semi-major axes).
        let n = self.bodies.len();
        self.hill_factor.clear();
        self.hill_factor.reserve(n * n);
        for a in &self.bodies {
            for b in &self.bodies {
                self.hill_factor.push(cbrt(
                    (a.mass_msun + b.mass_msun) / (3.0 * self.star_mass_msun),
                ));
            }
        }
        self.step_yr = self.wanted_step();
    }

    /// Changes the map's step if the orbits have changed enough to need
    /// it. Checked now and then, not every step: a symplectic map keeps
    /// its good behaviour only while its step is constant, so the step is
    /// changed rarely and by a clear margin.
    fn revise_step(&mut self) {
        let wanted = self.wanted_step();
        if wanted < 0.7 * self.step_yr || wanted > 2.0 * self.step_yr {
            self.step_yr = wanted;
        }
    }

    /// The step of the symplectic map, yr.
    pub fn step_yr(&self) -> f64 {
        self.step_yr
    }

    /// Velocity of the star in the frame of the centre of mass, given the
    /// planets' heliocentric velocities.
    fn star_velocity(&self) -> V3 {
        let total: f64 = self.star_mass_msun + self.bodies.iter().map(|b| b.mass_msun).sum::<f64>();
        let momentum = self
            .bodies
            .iter()
            .fold([0.0; 3], |sum, b| add(sum, scale(b.vel, b.mass_msun)));
        scale(momentum, -1.0 / total)
    }

    /// Total energy in the centre-of-mass frame, M☉ AU² yr⁻²: kinetic
    /// energy of the planets and the star, and every pair's potential.
    pub fn energy(&self) -> f64 {
        let star_vel = self.star_velocity();
        let mut energy = 0.5 * self.star_mass_msun * dot(star_vel, star_vel);
        for (i, b) in self.bodies.iter().enumerate() {
            let v = add(b.vel, star_vel);
            energy += 0.5 * b.mass_msun * dot(v, v);
            energy -= G * self.star_mass_msun * b.mass_msun / norm(b.pos);
            for other in &self.bodies[i + 1..] {
                energy -= G * b.mass_msun * other.mass_msun / norm(sub(b.pos, other.pos));
            }
        }
        energy
    }

    /// Total angular momentum about the centre of mass, M☉ AU² yr⁻¹.
    pub fn angular_momentum(&self) -> V3 {
        let total: f64 = self.star_mass_msun + self.bodies.iter().map(|b| b.mass_msun).sum::<f64>();
        let star_vel = self.star_velocity();
        // Position of the star relative to the centre of mass.
        let star_pos = scale(
            self.bodies
                .iter()
                .fold([0.0; 3], |sum, b| add(sum, scale(b.pos, b.mass_msun))),
            -1.0 / total,
        );
        let mut l = scale(cross(star_pos, star_vel), self.star_mass_msun);
        for b in &self.bodies {
            l = add(
                l,
                scale(
                    cross(add(b.pos, star_pos), add(b.vel, star_vel)),
                    b.mass_msun,
                ),
            );
        }
        l
    }

    /// True if the next map step needs the Hermite scheme instead: two
    /// planets within the encounter distance (allowing for their approach
    /// during the step), or a planet whose pericentre the step would not
    /// resolve.
    fn needs_hermite(&self) -> bool {
        let dt = self.step_yr;
        let n = self.bodies.len();
        // The map's Kepler drift is exact, but its kicks are not: a planet
        // much closer to the star than where the step was set sees them
        // poorly resolved. A local period under a quarter of the steps per
        // orbit means r³ below this.
        let quarter = 0.25 * self.settings.steps_per_orbit * dt / core::f64::consts::TAU;
        let r_cubed_min = quarter * quarter * G * self.star_mass_msun;
        let mut distances = Vec::with_capacity(n);
        for b in &self.bodies {
            let r = norm(b.pos);
            if r * r * r < r_cubed_min {
                return true;
            }
            distances.push(r);
        }
        let k = 0.5 * self.settings.encounter_hill_radii;
        for i in 0..n {
            let a = &self.bodies[i];
            for j in (i + 1)..n {
                let b = &self.bodies[j];
                let d = sub(b.pos, a.pos);
                let dv = sub(b.vel, a.vel);
                let limit = k * self.hill_factor[i * n + j] * (distances[i] + distances[j])
                    + a.radius_au
                    + b.radius_au;
                // Within the limit, or able to close to it in one step.
                let d_sq = dot(d, d);
                if d_sq < limit * limit || sqrt(d_sq) < limit + sqrt(dot(dv, dv)) * dt {
                    return true;
                }
            }
        }
        false
    }

    /// One step of the democratic heliocentric map.
    fn map_step(&mut self) {
        let dt = self.step_yr;
        let star = self.star_mass_msun;
        // To barycentric velocities.
        let star_vel = self.star_velocity();
        for b in &mut self.bodies {
            b.vel = add(b.vel, star_vel);
        }
        let sun_drift = |bodies: &mut [Body], h: f64| {
            let momentum = bodies
                .iter()
                .fold([0.0; 3], |sum, b| add(sum, scale(b.vel, b.mass_msun)));
            let shift = scale(momentum, h / star);
            for b in bodies.iter_mut() {
                b.pos = add(b.pos, shift);
            }
        };
        let kick = |bodies: &mut [Body], h: f64| {
            let n = bodies.len();
            for i in 0..n {
                for j in (i + 1)..n {
                    let d = sub(bodies[j].pos, bodies[i].pos);
                    let d_sq = dot(d, d);
                    let w = G * h / (d_sq * sqrt(d_sq));
                    let (mi, mj) = (bodies[i].mass_msun, bodies[j].mass_msun);
                    bodies[i].vel = add(bodies[i].vel, scale(d, w * mj));
                    bodies[j].vel = sub(bodies[j].vel, scale(d, w * mi));
                }
            }
        };
        sun_drift(&mut self.bodies, 0.5 * dt);
        kick(&mut self.bodies, 0.5 * dt);
        for b in &mut self.bodies {
            let (pos, vel) = kepler_step(b.pos, b.vel, G * star, dt);
            b.pos = pos;
            b.vel = vel;
        }
        kick(&mut self.bodies, 0.5 * dt);
        sun_drift(&mut self.bodies, 0.5 * dt);
        // Back to heliocentric velocities: v_h = v_b − v_star, with the
        // star's barycentric velocity −Σ m v_b / M.
        let momentum = self
            .bodies
            .iter()
            .fold([0.0; 3], |sum, b| add(sum, scale(b.vel, b.mass_msun)));
        let star_vel = scale(momentum, -1.0 / star);
        for b in &mut self.bodies {
            b.vel = sub(b.vel, star_vel);
        }
        self.t_yr += dt;
        self.work.map_steps += 1;
    }

    /// Integrates one map step's worth of time with the Hermite scheme,
    /// merging bodies that touch on the way.
    fn hermite_interval(&mut self) {
        let end = self.t_yr + self.step_yr;
        let mut guard = 0u32;
        while self.t_yr < end && !self.bodies.is_empty() {
            let (acc, jerk) = accelerations(self.star_mass_msun, &self.bodies);
            let mut dt = f64::INFINITY;
            for (a, j) in acc.iter().zip(&jerk) {
                let j_len = norm(*j);
                if j_len > 0.0 {
                    dt = dt.min(self.settings.hermite_eta * norm(*a) / j_len);
                }
            }
            let remaining = end - self.t_yr;
            let last = dt >= remaining;
            let dt = if last { remaining } else { dt };

            // Predict.
            let old = self.bodies.clone();
            for (b, (a, j)) in self.bodies.iter_mut().zip(acc.iter().zip(&jerk)) {
                b.pos = add(
                    add(b.pos, scale(b.vel, dt)),
                    add(scale(*a, 0.5 * dt * dt), scale(*j, dt * dt * dt / 6.0)),
                );
                b.vel = add(add(b.vel, scale(*a, dt)), scale(*j, 0.5 * dt * dt));
            }
            // Evaluate at the prediction and correct.
            let (acc1, jerk1) = accelerations(self.star_mass_msun, &self.bodies);
            for (k, b) in self.bodies.iter_mut().enumerate() {
                let vel = add(
                    add(old[k].vel, scale(add(acc[k], acc1[k]), 0.5 * dt)),
                    scale(sub(jerk[k], jerk1[k]), dt * dt / 12.0),
                );
                b.pos = add(
                    add(old[k].pos, scale(add(old[k].vel, vel), 0.5 * dt)),
                    scale(sub(acc[k], acc1[k]), dt * dt / 12.0),
                );
                b.vel = vel;
            }
            self.t_yr = if last { end } else { self.t_yr + dt };
            self.work.hermite_steps += 1;
            self.resolve_collisions();
            guard += 1;
            if guard > 2_000_000 {
                // A pathological interval: stop refining rather than hang.
                self.t_yr = end;
            }
        }
    }

    /// Merges every pair of bodies that overlap, and removes bodies inside
    /// the star.
    fn resolve_collisions(&mut self) {
        loop {
            let mut pair = None;
            'search: for i in 0..self.bodies.len() {
                for j in (i + 1)..self.bodies.len() {
                    let (a, b) = (&self.bodies[i], &self.bodies[j]);
                    if norm(sub(a.pos, b.pos)) < a.radius_au + b.radius_au {
                        pair = Some((i, j));
                        break 'search;
                    }
                }
            }
            let Some((i, j)) = pair else {
                break;
            };
            let before = self.energy();
            let (a, b) = (self.bodies[i], self.bodies[j]);
            let mass = a.mass_msun + b.mass_msun;
            let (heavy, light) = if a.mass_msun >= b.mass_msun {
                (a, b)
            } else {
                (b, a)
            };
            self.bodies[i] = Body {
                id: heavy.id,
                mass_msun: mass,
                // Volumes add: the merged body keeps the mean density.
                radius_au: cbrt(
                    a.radius_au * a.radius_au * a.radius_au
                        + b.radius_au * b.radius_au * b.radius_au,
                ),
                pos: scale(
                    add(scale(a.pos, a.mass_msun), scale(b.pos, b.mass_msun)),
                    1.0 / mass,
                ),
                vel: scale(
                    add(scale(a.vel, a.mass_msun), scale(b.vel, b.mass_msun)),
                    1.0 / mass,
                ),
            };
            self.bodies.remove(j);
            self.events.push(Event::Merged {
                t_yr: self.t_yr,
                survivor: heavy.id,
                absorbed: light.id,
                r_au: norm(self.bodies[i].pos),
            });
            self.energy_removed += before - self.energy();
            self.choose_step();
        }
        self.remove_lost();
    }

    /// Removes bodies that have fallen into the star or escaped.
    fn remove_lost(&mut self) {
        let mut k = 0;
        while k < self.bodies.len() {
            let b = self.bodies[k];
            let r = norm(b.pos);
            let mu = G * (self.star_mass_msun + b.mass_msun);
            let unbound = 0.5 * dot(b.vel, b.vel) - mu / r >= 0.0;
            let event = if r < self.star_radius_au + b.radius_au {
                Some(Event::HitStar {
                    t_yr: self.t_yr,
                    id: b.id,
                })
            } else if unbound && r > self.settings.ejection_distance_au {
                Some(Event::Ejected {
                    t_yr: self.t_yr,
                    id: b.id,
                })
            } else {
                None
            };
            match event {
                Some(event) => {
                    let before = self.energy();
                    self.bodies.remove(k);
                    self.events.push(event);
                    self.energy_removed += before - self.energy();
                    self.choose_step();
                }
                None => k += 1,
            }
        }
    }

    /// Integrates until `until_yr` or until `max_steps` more steps (of
    /// either kind) have been taken, whichever comes first. Returns true
    /// if `until_yr` was reached.
    pub fn advance(&mut self, until_yr: f64, max_steps: u64) -> bool {
        let limit = (self.work.map_steps + self.work.hermite_steps).saturating_add(max_steps);
        while self.t_yr < until_yr && !self.bodies.is_empty() {
            if self.work.map_steps + self.work.hermite_steps >= limit {
                return false;
            }
            if (self.work.map_steps + self.work.intervals) % STEP_REVIEW_INTERVAL == 0 {
                self.revise_step();
            }
            if self.needs_hermite() {
                self.hermite_interval();
                self.work.intervals += 1;
            } else {
                self.map_step();
                self.remove_lost();
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::f64::consts::{PI, TAU};

    const MSUN_PER_MEARTH: f64 = 3.003e-6;
    const EARTH_RADIUS_AU: f64 = 4.2635e-5;

    fn elements(a: f64, e: f64, mean_anomaly: f64) -> Elements {
        Elements {
            a: Au(a),
            e,
            i: 0.0,
            long_asc_node: 0.0,
            arg_periapsis: 0.0,
            mean_anomaly,
        }
    }

    fn planet(id: u32, mass_mearth: f64, a: f64, e: f64, mean_anomaly: f64) -> Body {
        let mass = mass_mearth * MSUN_PER_MEARTH;
        let (pos, vel) = state_from_elements(elements(a, e, mean_anomaly), G * (1.0 + mass));
        Body {
            id,
            mass_msun: mass,
            radius_au: EARTH_RADIUS_AU * mass_mearth.cbrt(),
            pos,
            vel,
        }
    }

    #[test]
    fn stumpff_functions_match_their_closed_forms() {
        // c₀(x) = cos √x, c₁ = sin √x/√x for x > 0; cosh, sinh for x < 0.
        for x in [1.0e-6, 0.05, 0.7, 3.0, 25.0, 140.0] {
            let c = stumpff(x);
            let root = f64::sqrt(x);
            assert!((c[0] - root.cos()).abs() < 1e-13, "c0({x})");
            assert!((c[1] - root.sin() / root).abs() < 1e-13, "c1({x})");
            // (The closed form of c₂ loses its digits for small x.)
            if x >= 0.05 {
                assert!((c[2] - (1.0 - root.cos()) / x).abs() < 1e-12, "c2({x})");
            }
            let c = stumpff(-x);
            assert!((c[0] / root.cosh() - 1.0).abs() < 1e-12, "c0(-{x})");
            assert!(
                (c[1] / (root.sinh() / root) - 1.0).abs() < 1e-12,
                "c1(-{x})"
            );
        }
        assert_eq!(stumpff(0.0), [1.0, 1.0, 0.5, 1.0 / 6.0]);
    }

    #[test]
    fn a_circular_orbit_turns_a_quarter_in_a_quarter_period() {
        // 1 AU around 1 M☉: one year, speed 2π AU/yr.
        let (pos, vel) = kepler_step([1.0, 0.0, 0.0], [0.0, TAU, 0.0], G, 0.25);
        assert!((pos[0]).abs() < 1e-13 && (pos[1] - 1.0).abs() < 1e-13);
        assert!((vel[0] + TAU).abs() < 1e-12 && vel[1].abs() < 1e-12);
    }

    #[test]
    fn eccentric_orbits_close_after_one_period() {
        for e in [0.1, 0.6, 0.9, 0.99] {
            let (pos, vel) = state_from_elements(elements(2.0, e, 0.3), G);
            let period = TAU * f64::sqrt(8.0 / G);
            // In one step, and in 37 unequal pieces.
            let (p1, v1) = kepler_step(pos, vel, G, period);
            assert!(norm(sub(p1, pos)) < 1e-9, "e = {e}: {}", norm(sub(p1, pos)));
            assert!(norm(sub(v1, vel)) < 1e-9 * norm(vel).max(1.0) * 100.0);
            let (mut p, mut v) = (pos, vel);
            let mut left = period;
            for k in 0..37 {
                let dt = if k == 36 {
                    left
                } else {
                    period * (k as f64 + 1.0) / 800.0
                };
                (p, v) = kepler_step(p, v, G, dt);
                left -= dt;
            }
            assert!(norm(sub(p, pos)) < 1e-8, "e = {e} in pieces");
        }
    }

    #[test]
    fn the_propagator_agrees_with_keplers_equation() {
        // Advance by a third of a period and compare with the elements'
        // own position at the later mean anomaly.
        let el = elements(1.5, 0.4, 0.2);
        let mu = G * 1.3;
        let period = TAU * f64::sqrt(1.5f64.powi(3) / mu);
        let (pos, vel) = state_from_elements(el, mu);
        let (later, _) = kepler_step(pos, vel, mu, period / 3.0);
        let expected = crate::kepler::position(Elements {
            mean_anomaly: 0.2 + TAU / 3.0,
            ..el
        });
        assert!(norm(sub(later, expected)) < 1e-12);
    }

    #[test]
    fn hyperbolic_flybys_conserve_energy_and_angular_momentum() {
        let pos = [-50.0, 3.0, 0.0];
        let vel = [8.0, 0.0, 0.2];
        let energy = |p: V3, v: V3| 0.5 * dot(v, v) - G / norm(p);
        let (p, v) = kepler_step(pos, vel, G, 15.0);
        assert!(energy(pos, vel) > 0.0);
        assert!((energy(p, v) / energy(pos, vel) - 1.0).abs() < 1e-12);
        assert!(norm(sub(cross(p, v), cross(pos, vel))) < 1e-11);
        // It has passed the star and is leaving.
        assert!(p[0] > 0.0 && dot(p, v) > 0.0);
    }

    #[test]
    fn elements_round_trip_through_state_vectors() {
        let el = Elements {
            a: Au(3.2),
            e: 0.37,
            i: 0.4,
            long_asc_node: 1.1,
            arg_periapsis: 2.5,
            mean_anomaly: 4.0,
        };
        let mu = G * 1.001;
        let (pos, vel) = state_from_elements(el, mu);
        // The state matches the position the renderer computes.
        assert!(norm(sub(pos, crate::kepler::position(el))) < 1e-13);
        // Vis-viva: v² = μ(2/r − 1/a).
        assert!((dot(vel, vel) - mu * (2.0 / norm(pos) - 1.0 / 3.2)).abs() < 1e-12);
        let back = elements_from_state(pos, vel, mu).unwrap();
        assert!((back.a.0 - 3.2).abs() < 1e-12 && (back.e - 0.37).abs() < 1e-13);
        assert!((back.i - 0.4).abs() < 1e-13);
        assert!((back.long_asc_node - 1.1).abs() < 1e-12);
        assert!((back.arg_periapsis - 2.5).abs() < 1e-12);
        assert!((back.mean_anomaly - 4.0).abs() < 1e-11);
        // Unbound: no elements.
        assert!(elements_from_state([1.0, 0.0, 0.0], [0.0, 10.0, 0.0], G).is_none());
        // Circular and in the plane: angles default to zero, phase kept.
        let (pos, vel) = state_from_elements(elements(1.0, 0.0, 1.0), G);
        let flat = elements_from_state(pos, vel, G).unwrap();
        assert!(flat.e < 1e-12 && flat.i < 1e-12);
        assert!(
            (crate::kepler::wrap_angle(flat.arg_periapsis + flat.mean_anomaly) - 1.0).abs() < 1e-9
        );
    }

    fn jupiter_and_saturn() -> Integration {
        Integration::new(
            1.0,
            0.00465,
            vec![
                planet(5, 317.8, 5.203, 0.048, 0.3),
                planet(6, 95.2, 9.537, 0.054, 4.0),
            ],
            Settings::default(),
        )
    }

    #[test]
    fn the_map_keeps_jupiter_and_saturn_for_ten_thousand_years() {
        let mut system = jupiter_and_saturn();
        let e0 = system.energy();
        let l0 = system.angular_momentum();
        let mut worst = [0.0f64; 2];
        for k in 1..=200 {
            assert!(system.advance(50.0 * f64::from(k), u64::MAX));
            let half = usize::from(k > 100);
            worst[half] = worst[half].max((system.energy() / e0 - 1.0).abs());
        }
        // The map's error scales as (m/M)·(n·dt)², here 10⁻³ × 0.3² ≈ 10⁻⁴
        // at the very worst, and in practice far less...
        assert!(worst[0] < 1.0e-5, "energy error {worst:?}");
        // ...and it is bounded, not growing: a symplectic map's signature.
        // The second five thousand years are no worse than the first.
        assert!(worst[1] < 1.5 * worst[0], "energy error grew: {worst:?}");
        let l = system.angular_momentum();
        assert!(norm(sub(l, l0)) / norm(l0) < 1e-12);
        // All of it by the map; nothing lost; orbits where they were.
        assert_eq!(system.work.hermite_steps, 0);
        assert!(system.events.is_empty() && system.bodies.len() == 2);
        let jupiter = elements_from_state(
            system.bodies[0].pos,
            system.bodies[0].vel,
            G * (1.0 + system.bodies[0].mass_msun),
        )
        .unwrap();
        assert!((jupiter.a.0 - 5.203).abs() < 0.02 && jupiter.e < 0.08);
    }

    #[test]
    fn hermite_alone_follows_an_eccentric_orbit() {
        // Force the Hermite scheme by calling it directly.
        let mut system = Integration::new(
            1.0,
            0.00465,
            vec![planet(1, 1.0, 1.0, 0.6, 0.0)],
            Settings::default(),
        );
        let e0 = system.energy();
        // Ten orbits, an interval at a time. (An interval is the map's
        // step: a twentieth of the pericentre time, 0.4^1.5 yr.)
        assert!((system.step_yr() - 0.4f64.powf(1.5) / 20.0).abs() < 1e-6);
        while system.t_yr < 10.0 {
            system.hermite_interval();
        }
        assert!((system.t_yr - 10.0).abs() < 0.02);
        assert!((system.energy() / e0 - 1.0).abs() < 1e-7);
        assert!(system.work.hermite_steps > 2000);
    }

    #[test]
    fn two_planets_on_the_same_orbit_collide_and_merge() {
        // One planet a thousandth of an AU outside the other, falling
        // toward it at 0.05 AU/yr (a quarter of a kilometre a second).
        let a = planet(1, 1.0, 1.0, 0.0, 0.0);
        let mut b = planet(2, 2.0, 1.0, 0.0, 0.0);
        b.pos = scale(a.pos, 1.001);
        b.vel = add(a.vel, scale(a.pos, -0.05));
        let mut system = Integration::new(1.0, 0.00465, vec![a, b], Settings::default());
        let e0 = system.energy();
        let l0 = system.angular_momentum();
        let mass0 = a.mass_msun + b.mass_msun;
        system.advance(1.0, 5_000_000);
        assert_eq!(system.bodies.len(), 1, "events: {:?}", system.events);
        let merged = system.bodies[0];
        // The heavier one's identity; mass and angular momentum kept.
        assert_eq!(merged.id, 2);
        assert!((merged.mass_msun / mass0 - 1.0).abs() < 1e-15);
        // (The pair's small angular momentum about its own centre goes
        // into the merged body's spin, which is not followed.)
        let dl = norm(sub(system.angular_momentum(), l0)) / norm(l0);
        assert!(dl < 1e-6, "angular momentum changed by {dl}");
        assert!(matches!(
            system.events[0],
            Event::Merged {
                survivor: 2,
                absorbed: 1,
                ..
            }
        ));
        // Volumes add.
        let expected = EARTH_RADIUS_AU * 3.0f64.cbrt();
        assert!((merged.radius_au / expected - 1.0).abs() < 1e-12);
        // Energy is accounted for: what the merger removed is recorded.
        let de = ((system.energy() + system.energy_removed) / e0 - 1.0).abs();
        assert!(de < 1e-7, "energy unaccounted for: {de}");
        // What is removed is the pair's energy of relative motion less
        // their mutual binding, either of which can be the larger; here
        // they met well below their mutual escape speed.
        assert!(system.energy_removed < 0.0);
    }

    #[test]
    fn a_close_pair_scatters_with_energy_conserved() {
        // Two Neptunes 1.5 mutual Hill radii apart: Hill-unstable (the
        // limit is 2√3 ≈ 3.46). They must interact strongly at once.
        let hill = (2.0 * 17.0 * MSUN_PER_MEARTH / 3.0).cbrt();
        let mut system = Integration::new(
            1.0,
            0.00465,
            vec![
                planet(1, 17.0, 1.0, 0.0, 0.0),
                planet(2, 17.0, 1.0 + 1.5 * hill, 0.0, 0.05),
            ],
            Settings::default(),
        );
        let e0 = system.energy();
        system.advance(200.0, 20_000_000);
        // Close encounters were handed to the Hermite scheme.
        assert!(system.work.hermite_steps > 0);
        assert!(((system.energy() + system.energy_removed) / e0 - 1.0).abs() < 1e-5);
        // They did not stay on their circles.
        let changed = system.bodies.len() < 2
            || system.bodies.iter().any(|b| {
                elements_from_state(b.pos, b.vel, G * (1.0 + b.mass_msun))
                    .map_or(true, |el| el.e > 0.01)
            });
        assert!(changed);
    }

    #[test]
    fn venus_and_earth_stay_apart() {
        // 26 mutual Hill radii: stable. The map does all the work.
        let mut system = Integration::new(
            1.0,
            0.00465,
            vec![
                planet(2, 0.815, 0.723, 0.007, 1.0),
                planet(3, 1.0, 1.0, 0.017, 2.0),
            ],
            Settings::default(),
        );
        let e0 = system.energy();
        assert!(system.advance(2000.0, u64::MAX));
        assert_eq!(system.bodies.len(), 2);
        assert_eq!(system.work.hermite_steps, 0);
        let de = (system.energy() / e0 - 1.0).abs();
        // (m/M)·(n·dt)² ≈ 3×10⁻⁶ × 0.3² ≈ 3×10⁻⁷ at worst.
        assert!(de < 1e-7, "energy error {de}");
        for (body, a) in system.bodies.iter().zip([0.723, 1.0]) {
            let el = elements_from_state(body.pos, body.vel, G * (1.0 + body.mass_msun)).unwrap();
            assert!((el.a.0 - a).abs() < 1e-3 && el.e < 0.03);
        }
    }

    #[test]
    fn bodies_can_fall_into_the_star_or_escape() {
        // One aimed at the star, one faster than escape speed.
        let falling = Body {
            id: 1,
            mass_msun: MSUN_PER_MEARTH,
            radius_au: EARTH_RADIUS_AU,
            pos: [0.3, 0.0, 0.0],
            vel: [-1.0, 0.05, 0.0],
        };
        let leaving = Body {
            id: 2,
            mass_msun: MSUN_PER_MEARTH,
            radius_au: EARTH_RADIUS_AU,
            pos: [0.0, 5.0, 0.0],
            vel: [0.0, 6.0, 1.0],
        };
        let settings = Settings {
            ejection_distance_au: 100.0,
            ..Settings::default()
        };
        let mut system = Integration::new(1.0, 0.00465, vec![falling, leaving], settings);
        let e0 = system.energy();
        system.advance(40.0, 50_000_000);
        assert!(system.bodies.is_empty(), "left: {:?}", system.bodies);
        assert!(system
            .events
            .iter()
            .any(|e| matches!(e, Event::HitStar { id: 1, .. })));
        assert!(system
            .events
            .iter()
            .any(|e| matches!(e, Event::Ejected { id: 2, .. })));
        // Everything the bodies carried is in the removed-energy account.
        assert!((system.energy_removed / e0 - 1.0).abs() < 1e-3);
    }

    #[test]
    fn the_step_follows_the_shortest_orbit_and_the_budget_is_respected() {
        let mut system = Integration::new(
            1.0,
            0.00465,
            vec![planet(1, 1.0, 0.1, 0.0, 0.0), planet(2, 1.0, 1.0, 0.0, PI)],
            Settings::default(),
        );
        // 0.1 AU: a period of 0.1^1.5 yr, in twenty steps.
        assert!((system.step_yr() - 0.1f64.powf(1.5) / 20.0).abs() < 1e-8);
        assert!(!system.advance(1000.0, 100));
        assert_eq!(system.work.map_steps + system.work.hermite_steps, 100);
        assert!(system.t_yr < 1.0);
    }

    #[test]
    fn integrations_repeat_exactly() {
        let run = || {
            let mut system = jupiter_and_saturn();
            system.advance(300.0, u64::MAX);
            system
        };
        assert_eq!(run(), run());
    }
}

#[cfg(test)]
mod debug_tests {
    use super::*;

    #[test]
    #[ignore = "diagnostic"]
    fn three_jupiters_energy_trace() {
        let m = 318.0 * 3.003e-6;
        let hill = (2.0 * m / 3.0f64).cbrt();
        let ratio = (1.0 + 1.6 * hill) / (1.0 - 1.6 * hill);
        let mut bodies = Vec::new();
        for (k, phase) in [0.3, 2.1, 4.4].iter().enumerate() {
            let a = 5.0 * ratio.powi(k as i32);
            let el = Elements {
                a: Au(a),
                e: 0.01,
                i: 0.005,
                long_asc_node: 1.0 + k as f64,
                arg_periapsis: 0.5 * k as f64,
                mean_anomaly: *phase,
            };
            let (pos, vel) = state_from_elements(el, G * (1.0 + m));
            bodies.push(Body {
                id: k as u32,
                mass_msun: m,
                radius_au: 4.7e-4,
                pos,
                vel,
            });
        }
        let mut sys = Integration::new(1.0, 0.00465, bodies, Settings::default());
        let e0 = sys.energy();
        let mut last = (0u64, 0u64, 0usize);
        let mut prev = 0.0f64;
        for k in 1..=40000 {
            sys.advance(25.0 * f64::from(k), u64::MAX);
            let err = (sys.energy() + sys.energy_removed) / e0 - 1.0;
            let now = (sys.work.map_steps, sys.work.hermite_steps, sys.events.len());
            let jump = (err - prev).abs() > 2e-4;
            prev = err;
            if jump || now.2 != last.2 {
                println!(
                    "t {:>8.0} err {:>10.2e} map +{} hermite +{} bodies {} step {:.4} events {:?}",
                    sys.t_yr,
                    err,
                    now.0 - last.0,
                    now.1 - last.1,
                    sys.bodies.len(),
                    sys.step_yr(),
                    &sys.events[last.2..]
                );
                for b in &sys.bodies {
                    let el = elements_from_state(b.pos, b.vel, G * (1.0 + b.mass_msun));
                    println!(
                        "     id {} r {:.3} {:?}",
                        b.id,
                        norm(b.pos),
                        el.map(|e| (e.a.0, e.e))
                    );
                }
            }
            last = now;
            if err.abs() > 5e-2 || sys.bodies.len() < 3 && k > 30000 {
                break;
            }
        }
    }
}
