//! Seeded random streams.
//!
//! Every simulation module draws from its own ChaCha8 stream, derived from
//! the run seed and a module id. Adding a draw in one module therefore never
//! shifts the numbers another module sees (PLAN.md §6.4).
//!
//! Sampling helpers are implemented here rather than taken from the `rand`
//! crate, whose distribution algorithms are allowed to change between
//! releases.

use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

/// Identifies the module that owns a stream. Values are part of the model:
/// changing one changes outputs and requires a `model_version` bump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleId(pub u64);

impl ModuleId {
    pub const STELLAR: ModuleId = ModuleId(1);
    pub const DISK: ModuleId = ModuleId(2);
    pub const FORMATION: ModuleId = ModuleId(3);
    pub const DYNAMICS: ModuleId = ModuleId(4);
    pub const PLANET: ModuleId = ModuleId(5);
    /// Orbital angles the Classic tier does not model (PLAN.md §7.3).
    pub const ORBIT_ANGLES: ModuleId = ModuleId(6);
    pub const VISUAL: ModuleId = ModuleId(7);
    /// Names of stars and planets.
    pub const NAMES: ModuleId = ModuleId(8);
    /// Moons and rings.
    pub const MOONS: ModuleId = ModuleId(9);
    /// Nuclei of the accretion model (the Classic tier).
    pub const ACCRETION: ModuleId = ModuleId(10);
}

/// One SplitMix64 step (Steele, Lea & Flood 2014). Used only to spread the
/// 64-bit run seed over the 256-bit ChaCha key.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A deterministic random stream owned by one module.
#[derive(Debug, Clone)]
pub struct RngStream {
    inner: ChaCha8Rng,
}

impl RngStream {
    /// Derives the stream for `module` from the run `seed`.
    pub fn new(seed: u64, module: ModuleId) -> Self {
        let mut state = seed;
        let mut key = [0u8; 32];
        for chunk in key.chunks_exact_mut(8) {
            chunk.copy_from_slice(&splitmix64(&mut state).to_le_bytes());
        }
        let mut inner = ChaCha8Rng::from_seed(key);
        inner.set_stream(module.0);
        Self { inner }
    }

    /// Next raw 64 bits.
    pub fn next_u64(&mut self) -> u64 {
        self.inner.next_u64()
    }

    /// Uniform in `[0, 1)` with 53 random bits.
    pub fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Standard normal variate, by the Box–Muller transform (Box & Muller
    /// 1958): `sqrt(-2 ln u₁)·cos(2π u₂)`. Consumes two uniforms.
    pub fn standard_normal(&mut self) -> f64 {
        // 1 − u is in (0, 1], so the logarithm is finite.
        let u1 = 1.0 - self.uniform();
        let u2 = self.uniform();
        crate::math::sqrt(-2.0 * crate::math::ln(u1))
            * crate::math::cos(2.0 * core::f64::consts::PI * u2)
    }

    /// Uniform in `[lo, hi)`.
    pub fn uniform_range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.uniform()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn same_seed_and_module_repeat() {
        let mut a = RngStream::new(42, ModuleId::STELLAR);
        let mut b = RngStream::new(42, ModuleId::STELLAR);
        for _ in 0..64 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn modules_and_seeds_are_independent() {
        let first = |seed, module| RngStream::new(seed, module).next_u64();
        assert_ne!(first(42, ModuleId::STELLAR), first(42, ModuleId::DISK));
        assert_ne!(first(42, ModuleId::STELLAR), first(43, ModuleId::STELLAR));
    }

    /// Pins the stream so an accidental change (dependency upgrade, key
    /// derivation edit) fails loudly on every platform.
    #[test]
    fn stream_is_pinned() {
        let mut rng = RngStream::new(42, ModuleId::STELLAR);
        let got = [rng.next_u64(), rng.next_u64(), rng.next_u64()];
        assert_eq!(got, PINNED_SEED_42_STELLAR);
    }

    const PINNED_SEED_42_STELLAR: [u64; 3] = [
        14_589_515_635_352_196_654,
        2_877_973_030_336_034_368,
        15_230_479_811_515_263_985,
    ];

    #[test]
    fn uniform_mean_is_half() {
        let mut rng = RngStream::new(7, ModuleId::PLANET);
        let n = 100_000;
        let mean = (0..n).map(|_| rng.uniform()).sum::<f64>() / n as f64;
        // Standard error is 1/sqrt(12 n) ≈ 9.1e-4; allow 5 sigma.
        assert!((mean - 0.5).abs() < 5e-3, "mean = {mean}");
    }

    #[test]
    fn normal_has_zero_mean_and_unit_variance() {
        let mut rng = RngStream::new(9, ModuleId::STELLAR);
        let n = 200_000;
        let xs: Vec<f64> = (0..n).map(|_| rng.standard_normal()).collect();
        let mean = xs.iter().sum::<f64>() / n as f64;
        let var = xs.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n as f64;
        assert!(mean.abs() < 0.01, "mean = {mean}");
        assert!((var - 1.0).abs() < 0.02, "variance = {var}");
        assert!(xs.iter().all(|x| x.is_finite()));
        // 68.27% of draws fall within one standard deviation.
        let within = xs.iter().filter(|x| x.abs() < 1.0).count() as f64 / n as f64;
        assert!((within - 0.6827).abs() < 0.005, "{within}");
    }

    proptest! {
        #[test]
        fn uniform_stays_in_unit_interval(seed: u64, module: u64) {
            let mut rng = RngStream::new(seed, ModuleId(module));
            for _ in 0..32 {
                let u = rng.uniform();
                prop_assert!((0.0..1.0).contains(&u));
            }
        }

        #[test]
        fn uniform_range_stays_in_range(seed: u64, lo in -1e6f64..1e6, width in 1e-6f64..1e6) {
            let mut rng = RngStream::new(seed, ModuleId::DISK);
            let x = rng.uniform_range(lo, lo + width);
            prop_assert!(x >= lo && x <= lo + width);
        }
    }
}
