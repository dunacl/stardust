//! Colours for bodies and overlays.
//!
//! Each planet is drawn as a sphere in one colour chosen by its class. The
//! class comes from the simulation; only the swatch is chosen here. Surface
//! palettes derived from composition and temperature arrive with M5.

use bevy::color::{Color, LinearRgba};
use sg_schema::PlanetClass;

/// sRGB base colour for a planet class.
pub fn class_color(class: PlanetClass) -> Color {
    let (r, g, b) = match class {
        PlanetClass::Asteroids => (0.42, 0.40, 0.38),
        PlanetClass::Rock => (0.55, 0.50, 0.46),
        PlanetClass::Venusian => (0.89, 0.72, 0.38),
        PlanetClass::Terrestrial => (0.22, 0.47, 0.78),
        PlanetClass::Martian => (0.76, 0.40, 0.26),
        PlanetClass::Water => (0.10, 0.33, 0.72),
        PlanetClass::Ice => (0.80, 0.89, 0.95),
        PlanetClass::OneFace => (0.60, 0.42, 0.50),
        PlanetClass::GasDwarf => (0.55, 0.74, 0.76),
        PlanetClass::SubJovian => (0.38, 0.60, 0.86),
        PlanetClass::Jovian => (0.85, 0.68, 0.50),
        PlanetClass::Unknown => (0.60, 0.60, 0.60),
        PlanetClass::Rocky => (0.58, 0.52, 0.47),
        PlanetClass::SuperEarth => (0.66, 0.55, 0.44),
        PlanetClass::LavaWorld => (0.86, 0.32, 0.16),
        PlanetClass::WaterWorld => (0.14, 0.38, 0.74),
        PlanetClass::IceWorld => (0.82, 0.90, 0.96),
        PlanetClass::SubNeptune => (0.52, 0.76, 0.80),
        PlanetClass::IceGiant => (0.36, 0.62, 0.88),
        PlanetClass::GasGiant => (0.84, 0.70, 0.52),
        PlanetClass::HotJupiter => (0.90, 0.56, 0.34),
    };
    Color::srgb(r, g, b)
}

/// Rocks of a belt: lighter than the class swatch, since they are drawn
/// only a pixel or two across.
pub const BELT_ROCK_COLOR: Color = Color::srgb(0.66, 0.62, 0.57);

/// Colour of a body that is still forming.
pub const EMBRYO_COLOR: Color = Color::srgb(0.62, 0.50, 0.40);

pub const ORBIT_COLOR: Color = Color::srgba(0.55, 0.62, 0.75, 0.35);
pub const ORBIT_SELECTED_COLOR: Color = Color::srgba(1.0, 0.92, 0.55, 0.95);
pub const HZ_COLOR: Color = Color::srgba(0.30, 0.85, 0.45, 0.55);
pub const HZ_OPTIMISTIC_COLOR: Color = Color::srgba(0.30, 0.85, 0.45, 0.22);
/// Paths of the stars of a pair around their centre of mass.
pub const STAR_ORBIT_COLOR: Color = Color::srgba(0.95, 0.80, 0.45, 0.45);
/// Edge of the zone open to planets, where a companion star sets it.
pub const STABILITY_LIMIT_COLOR: Color = Color::srgba(0.90, 0.35, 0.30, 0.40);
pub const SWEEP_COLOR: Color = Color::srgb(0.95, 0.70, 0.30);
pub const COLLISION_COLOR: Color = Color::srgb(1.0, 1.0, 1.0);

/// A star's colour as Bevy linear RGB, from the snapshot's blackbody colour.
pub fn star_color(rgb: [f64; 3]) -> LinearRgba {
    LinearRgba::rgb(rgb[0] as f32, rgb[1] as f32, rgb[2] as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::color::ColorToComponents;

    #[test]
    fn every_class_has_a_distinct_colour() {
        let classes = [
            PlanetClass::Asteroids,
            PlanetClass::Rock,
            PlanetClass::Venusian,
            PlanetClass::Terrestrial,
            PlanetClass::Martian,
            PlanetClass::Water,
            PlanetClass::Ice,
            PlanetClass::OneFace,
            PlanetClass::GasDwarf,
            PlanetClass::SubJovian,
            PlanetClass::Jovian,
            PlanetClass::Unknown,
            PlanetClass::Rocky,
            PlanetClass::SuperEarth,
            PlanetClass::LavaWorld,
            PlanetClass::WaterWorld,
            PlanetClass::IceWorld,
            PlanetClass::SubNeptune,
            PlanetClass::IceGiant,
            PlanetClass::GasGiant,
            PlanetClass::HotJupiter,
        ];
        let colors: Vec<[f32; 4]> = classes
            .iter()
            .map(|c| class_color(*c).to_srgba().to_f32_array())
            .collect();
        for (i, a) in colors.iter().enumerate() {
            for b in &colors[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
}
