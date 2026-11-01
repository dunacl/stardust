//! What a world's surface is probably made of: an estimate, not chemistry.
//!
//! The model gives bulk fractions and a temperature, and follows no mineral.
//! This turns them into a likely surface mix by a few rules that hold in the
//! Solar System. Cards label the result as an estimate, and shares are
//! rough. The thresholds are round values:
//!
//! - Above about 1400 K silicate rock melts (basalt's solidus is near this
//!   at low pressure).
//! - Below about 170 K water ice lasts at the surface (Hayashi 1981, Prog.
//!   Theor. Phys. Suppl. 70, 35, gives this for the snow line).
//! - Below about 90 K ices of nitrogen and methane also last, as on Triton
//!   and Pluto.
//! - Water oxidises iron, so a wet or once-wet world shows iron oxides, as
//!   Mars does; with liquid water at Earth-like temperatures carbonates form
//!   too (Walker, Hays & Kasting 1981, JGR 86, 9776).
//! - Silicates make up what is left, as they do on every rocky body known.

use sg_schema::{Mineral, MineralShare};

const MELT_K: f64 = 1400.0;
const WATER_ICE_K: f64 = 170.0;
const VOLATILE_ICE_K: f64 = 90.0;
/// Temperatures between which liquid water can stand and weather rock.
const LIQUID_WATER_K: (f64, f64) = (240.0, 400.0);
/// Water fraction above which a world counts as wet or once wet.
const WET: f64 = 1.0e-4;
/// Hydrogen and helium above which a body has no solid surface to speak of.
const GAS_ENVELOPE: f64 = 0.05;

/// The likely surface mix of a body, by decreasing share, adding to 1.
/// Empty for a body with a deep envelope, which has no surface.
///
/// Fractions are by mass of the whole body; `temp_k` is the equilibrium
/// temperature. Valid for rocky and icy bodies from about 30 to 3000 K. An
/// estimate from the rules in the module notes, for the card and the drawn
/// colour only.
pub fn surface_mix(
    metal_frac: f64,
    rock_frac: f64,
    water_frac: f64,
    h_he_frac: f64,
    temp_k: f64,
) -> Vec<MineralShare> {
    if h_he_frac > GAS_ENVELOPE {
        return Vec::new();
    }
    let mut parts: Vec<(Mineral, f64)> = Vec::new();
    if temp_k >= MELT_K {
        parts.push((Mineral::MoltenRock, 0.7));
    }
    if temp_k < WATER_ICE_K && water_frac > 0.01 {
        // A little ice covers a lot: it rises to the surface.
        parts.push((Mineral::WaterIce, (3.0 * water_frac).clamp(0.2, 0.85)));
        if temp_k < VOLATILE_ICE_K {
            parts.push((Mineral::VolatileIces, 0.1));
        }
    }
    let wet = water_frac > WET;
    let iron_share = metal_frac / (metal_frac + rock_frac).max(1e-9);
    if wet && temp_k < MELT_K {
        parts.push((Mineral::IronOxides, 0.4 * iron_share));
    }
    if wet && temp_k >= LIQUID_WATER_K.0 && temp_k <= LIQUID_WATER_K.1 {
        parts.push((Mineral::Carbonates, 0.08));
    }
    if temp_k < MELT_K {
        parts.push((Mineral::Sulfides, 0.03));
    }
    let taken: f64 = parts.iter().map(|(_, share)| share).sum();
    parts.push((Mineral::Silicates, f64::max(1.0 - taken, 0.05)));
    let total: f64 = parts.iter().map(|(_, share)| share).sum();
    let mut mix: Vec<MineralShare> = parts
        .into_iter()
        .map(|(mineral, share)| MineralShare {
            mineral,
            share: share / total,
        })
        .collect();
    mix.sort_by(|a, b| b.share.total_cmp(&a.share));
    mix
}

#[cfg(test)]
mod tests {
    use super::*;

    fn share(mix: &[MineralShare], mineral: Mineral) -> f64 {
        mix.iter()
            .find(|m| m.mineral == mineral)
            .map_or(0.0, |m| m.share)
    }

    #[test]
    fn the_shares_add_up_and_run_downward() {
        for (metal, rock, water, gas, temp) in [
            (0.33, 0.67, 0.0, 0.0, 700.0),
            (0.1, 0.4, 0.5, 0.0, 100.0),
            (0.33, 0.67, 0.001, 0.0, 255.0),
            (0.33, 0.67, 0.0, 0.0, 2000.0),
        ] {
            let mix = surface_mix(metal, rock, water, gas, temp);
            let total: f64 = mix.iter().map(|m| m.share).sum();
            assert!((total - 1.0).abs() < 1e-12);
            assert!(mix.windows(2).all(|p| p[0].share >= p[1].share));
        }
    }

    /// Earth's crust: silicates, with carbonates and iron oxides.
    #[test]
    fn an_earth_is_mostly_silicate_and_weathered() {
        let mix = surface_mix(0.33, 0.67, 0.001, 0.0, 255.0);
        assert_eq!(mix[0].mineral, Mineral::Silicates);
        assert!(share(&mix, Mineral::Carbonates) > 0.0);
        assert!(share(&mix, Mineral::IronOxides) > 0.0);
    }

    /// Mercury and the Moon are dry: no oxides or carbonates.
    #[test]
    fn a_dry_world_does_not_rust() {
        let mix = surface_mix(0.33, 0.67, 0.0, 0.0, 440.0);
        assert_eq!(share(&mix, Mineral::IronOxides), 0.0);
        assert_eq!(share(&mix, Mineral::Carbonates), 0.0);
    }

    /// Europa at about 100 K, Pluto at about 44 K.
    #[test]
    fn cold_wet_worlds_are_iced_over() {
        let europa = surface_mix(0.1, 0.8, 0.1, 0.0, 100.0);
        assert!(share(&europa, Mineral::WaterIce) > 0.2);
        assert_eq!(share(&europa, Mineral::VolatileIces), 0.0);
        let pluto = surface_mix(0.05, 0.35, 0.6, 0.0, 44.0);
        assert_eq!(pluto[0].mineral, Mineral::WaterIce);
        assert!(share(&pluto, Mineral::VolatileIces) > 0.0);
    }

    #[test]
    fn hot_worlds_are_molten_and_giants_have_no_surface() {
        let lava = surface_mix(0.33, 0.67, 0.0, 0.0, 2200.0);
        assert_eq!(lava[0].mineral, Mineral::MoltenRock);
        assert!(surface_mix(0.05, 0.05, 0.0, 0.9, 120.0).is_empty());
    }
}
