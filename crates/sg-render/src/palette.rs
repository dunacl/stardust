//! Colours for bodies and overlays.
//!
//! M2 draws each planet as a sphere in one colour chosen by its class. The
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
    };
    Color::srgb(r, g, b)
}

/// Colour of a body that is still forming.
pub const EMBRYO_COLOR: Color = Color::srgb(0.62, 0.50, 0.40);

pub const ORBIT_COLOR: Color = Color::srgba(0.55, 0.62, 0.75, 0.35);
pub const ORBIT_SELECTED_COLOR: Color = Color::srgba(1.0, 0.92, 0.55, 0.95);
pub const HZ_COLOR: Color = Color::srgba(0.30, 0.85, 0.45, 0.55);
pub const HZ_OPTIMISTIC_COLOR: Color = Color::srgba(0.30, 0.85, 0.45, 0.22);
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
