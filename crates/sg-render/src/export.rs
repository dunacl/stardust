//! Saves the view as a binary glTF 2.0 file (`.glb`), which Blender and most
//! 3D tools open.
//!
//! It holds what is drawn at this moment: the same positions and drawn sizes,
//! in the chosen scale, with 1 unit for 1 scene unit (an AU in the True and
//! Enlarged scales). Surfaces cannot travel with their shaders, so they are
//! baked into vertex colours, from the same appearance and the same noise.
//! The bake is coarser than the screen: no craters, clouds, relief or
//! atmosphere, and nothing that depends on the camera. Belts, comet tails
//! and the sky are not exported. Nodes carry the body's name, and `extras`
//! its class and mass, so they can be found in the file.

use crate::planet::{self, fbm};
use crate::surface::{Appearance, SurfaceKind};
use bevy::math::{Quat, Vec3};
use std::fmt::Write;

/// Segments of an exported sphere around its equator; half from pole to pole.
const SPHERE_AROUND: usize = 96;
const SPHERE_DOWN: usize = 48;
/// Segments of an exported ring.
const RING_AROUND: usize = 128;

/// What an object is painted with.
#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    /// A body's baked surface.
    Surface(Box<Appearance>),
    /// Shines by itself: a star, as linear RGB.
    Light([f32; 3]),
    /// One colour, linear RGB.
    Plain([f32; 3]),
}

/// A ring around an object, in its equatorial plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ring {
    /// As multiples of the object's radius.
    pub inner: f32,
    pub outer: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    pub name: String,
    /// What it is, for `extras`.
    pub kind: String,
    pub mass_mearth: Option<f64>,
    pub position: Vec3,
    pub radius: f32,
    pub tilt: Quat,
    pub paint: Paint,
    pub ring: Option<Ring>,
}

/// Linear RGB of a surface at a unit direction in its own frame. A coarser
/// version of `planet.wesl`: it follows that shader's choices, and shares
/// its noise through [`planet::fbm`].
pub fn bake(look: &Appearance, direction: Vec3) -> [f32; 3] {
    let colors = look.colors.map(Vec3::from);
    let p = direction + Vec3::new(look.seed * 0.37, look.seed * 0.11, look.seed * 0.23);
    let smooth = |low: f32, high: f32, x: f32| {
        let t = ((x - low) / (high - low)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let color = match look.kind {
        SurfaceKind::Rock => {
            let h = fbm(p * 2.2, 5);
            let fine = fbm(p * 9.0 + Vec3::splat(40.0), 5);
            colors[0]
                .lerp(colors[1], smooth(0.3, 0.7, h))
                .lerp(colors[2], smooth(0.55, 0.8, fine) * 0.6)
        }
        SurfaceKind::LandAndSea => {
            let h = fbm(p * 1.6, 5);
            let sea_level = planet::noise_quantile(look.ocean_frac);
            let land = colors[0].lerp(colors[1], smooth(sea_level, sea_level + 0.25, h));
            let cap = crate::surface::ice_cap_sin_latitude(look.ice_frac);
            if direction.y.abs() > cap {
                Vec3::new(0.90, 0.93, 0.96)
            } else if h < sea_level {
                Vec3::from(look.ocean_color)
            } else {
                land
            }
        }
        SurfaceKind::Ice => colors[0].lerp(colors[1], smooth(0.35, 0.7, fbm(p * 2.0, 5))),
        SurfaceKind::Lava => {
            let h = fbm(p * 2.4, 5);
            let molten = smooth(0.5, 0.62, fbm(p * 6.0, 5));
            colors[0]
                .lerp(colors[1], smooth(0.3, 0.7, h))
                .lerp(Vec3::from(look.glow_color), molten)
        }
        SurfaceKind::Bands | SurfaceKind::Haze => {
            let latitude = direction.y.clamp(-1.0, 1.0).asin();
            let swirl = fbm(Vec3::new(p.x, p.y * 4.0, p.z) * 2.2, 5) - 0.5;
            let wave = 0.5 + 0.5 * (latitude * look.band_count + 5.0 * swirl).sin();
            colors[0].lerp(colors[1], smooth(0.35, 0.65, wave) * look.band_contrast)
        }
    };
    color.to_array()
}

/// A growing binary buffer and what the JSON says about it.
#[derive(Default)]
struct Writer {
    bin: Vec<u8>,
    views: Vec<String>,
    accessors: Vec<String>,
}

/// glTF's `componentType` and `target` values.
const FLOAT: u32 = 5126;
const UNSIGNED_INT: u32 = 5125;
const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;

impl Writer {
    fn view(&mut self, bytes: &[u8], target: u32) -> usize {
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
        self.views.push(format!(
            r#"{{"buffer":0,"byteOffset":{},"byteLength":{},"target":{target}}}"#,
            self.bin.len(),
            bytes.len()
        ));
        self.bin.extend_from_slice(bytes);
        self.views.len() - 1
    }

    fn vec3s(&mut self, values: &[[f32; 3]], bounds: bool) -> usize {
        let bytes: Vec<u8> = values
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let view = self.view(&bytes, ARRAY_BUFFER);
        let mut extent = String::new();
        if bounds {
            let fold = |pick: fn(f32, f32) -> f32, start: f32| {
                [0, 1, 2].map(|axis| values.iter().map(|v| v[axis]).fold(start, pick))
            };
            let (low, high) = (fold(f32::min, f32::MAX), fold(f32::max, f32::MIN));
            write!(extent, r#","min":{low:?},"max":{high:?}"#).expect("a String takes writes");
        }
        self.accessors.push(format!(
            r#"{{"bufferView":{view},"componentType":{FLOAT},"count":{},"type":"VEC3"{extent}}}"#,
            values.len()
        ));
        self.accessors.len() - 1
    }

    fn indices(&mut self, values: &[u32]) -> usize {
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        let view = self.view(&bytes, ELEMENT_ARRAY_BUFFER);
        self.accessors.push(format!(
            r#"{{"bufferView":{view},"componentType":{UNSIGNED_INT},"count":{},"type":"SCALAR"}}"#,
            values.len()
        ));
        self.accessors.len() - 1
    }
}

/// The objects drawn at this moment, in the positions and sizes they are
/// drawn at, centred on the first star. Bodies drawn as belts are left out.
pub fn objects_in_view(
    snapshot: &sg_schema::SystemSnapshot,
    placements: &crate::BodyPlacements,
) -> Vec<Object> {
    use crate::scene::Target;
    let stars = crate::layout::stars(&snapshot.hierarchy);
    let centre = stars
        .first()
        .and_then(|star| placements.get(Target::Star(star.id)))
        .map_or(Vec3::ZERO, |placement| placement.position);
    let mut objects = Vec::new();
    for star in stars {
        if let Some(placement) = placements.get(Target::Star(star.id)) {
            objects.push(Object {
                name: star.name.clone(),
                kind: format!("star {}", star.spectral_class),
                mass_mearth: None,
                position: placement.position - centre,
                radius: placement.body_radius,
                tilt: Quat::IDENTITY,
                paint: Paint::Light(star.color_linear_rgb.map(|c| c as f32)),
                ring: None,
            });
        }
    }
    for body in &snapshot.bodies {
        if crate::belt::is_belt(body) {
            continue;
        }
        let Some(placement) = placements.get(Target::Body(body.id)) else {
            continue;
        };
        let look = crate::surface::appearance(body);
        objects.push(Object {
            name: body.name.clone(),
            kind: format!("{:?}", body.class),
            mass_mearth: Some(body.mass_mearth),
            position: placement.position - centre,
            radius: placement.body_radius,
            tilt: planet::axis_tilt(&look),
            paint: Paint::Surface(Box::new(look)),
            ring: crate::ring::extent_in_radii(body).map(|(inner, outer)| Ring { inner, outer }),
        });
    }
    for comet in &snapshot.comets {
        if let Some(placement) = placements.get(Target::Comet(comet.id)) {
            objects.push(Object {
                name: comet.name.clone(),
                kind: "comet".to_string(),
                mass_mearth: None,
                position: placement.position - centre,
                radius: placement.body_radius,
                tilt: Quat::IDENTITY,
                paint: Paint::Plain([0.05, 0.05, 0.05]),
                ring: None,
            });
        }
    }
    objects
}

/// Unit directions of a sphere's vertices, pole to pole, and its triangles,
/// wound to face outward.
fn sphere() -> (Vec<[f32; 3]>, Vec<u32>) {
    let mut points = Vec::with_capacity((SPHERE_AROUND + 1) * (SPHERE_DOWN + 1));
    for down in 0..=SPHERE_DOWN {
        let polar = std::f32::consts::PI * down as f32 / SPHERE_DOWN as f32;
        for around in 0..=SPHERE_AROUND {
            let angle = std::f32::consts::TAU * around as f32 / SPHERE_AROUND as f32;
            points.push([
                polar.sin() * angle.cos(),
                polar.cos(),
                polar.sin() * angle.sin(),
            ]);
        }
    }
    let row = SPHERE_AROUND as u32 + 1;
    let mut triangles = Vec::with_capacity(6 * SPHERE_AROUND * SPHERE_DOWN);
    for down in 0..SPHERE_DOWN as u32 {
        for around in 0..SPHERE_AROUND as u32 {
            let (a, b) = (down * row + around, down * row + around + 1);
            let (c, d) = (a + row, b + row);
            triangles.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
    (points, triangles)
}

/// A flat ring in the XZ plane between two radii; its material is two-sided.
fn annulus(ring: Ring) -> (Vec<[f32; 3]>, Vec<u32>) {
    let mut points = Vec::with_capacity(2 * (RING_AROUND + 1));
    for around in 0..=RING_AROUND {
        let angle = std::f32::consts::TAU * around as f32 / RING_AROUND as f32;
        for radius in [ring.inner, ring.outer] {
            points.push([radius * angle.cos(), 0.0, radius * angle.sin()]);
        }
    }
    let mut triangles = Vec::with_capacity(6 * RING_AROUND);
    for around in 0..RING_AROUND as u32 {
        let (a, b, c, d) = (2 * around, 2 * around + 1, 2 * around + 2, 2 * around + 3);
        triangles.extend_from_slice(&[a, c, b, b, c, d]);
    }
    (points, triangles)
}

/// Escapes text for a JSON string.
fn json_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// The objects as a `.glb` file.
pub fn glb(objects: &[Object]) -> Vec<u8> {
    let mut writer = Writer::default();
    let (points, triangles) = sphere();
    let shared_position = writer.vec3s(&points, true);
    let shared_indices = writer.indices(&triangles);
    let mut materials: Vec<String> = Vec::new();
    let mut meshes: Vec<String> = Vec::new();
    let mut nodes: Vec<String> = Vec::new();
    let mut roots: Vec<usize> = Vec::new();

    for object in objects {
        let (color, base, emissive) = match &object.paint {
            Paint::Surface(look) => {
                let colors: Vec<[f32; 3]> =
                    points.iter().map(|p| bake(look, Vec3::from(*p))).collect();
                (
                    Some(writer.vec3s(&colors, false)),
                    [1.0, 1.0, 1.0],
                    [0.0; 3],
                )
            }
            Paint::Light(rgb) => (None, [0.0, 0.0, 0.0], *rgb),
            Paint::Plain(rgb) => (None, *rgb, [0.0; 3]),
        };
        materials.push(format!(
            r#"{{"pbrMetallicRoughness":{{"baseColorFactor":[{},{},{},1],"metallicFactor":0,"roughnessFactor":1}},"emissiveFactor":{emissive:?}}}"#,
            base[0], base[1], base[2]
        ));
        let colored = color.map_or(String::new(), |c| format!(r#","COLOR_0":{c}"#));
        meshes.push(format!(
            r#"{{"name":"{}","primitives":[{{"attributes":{{"POSITION":{shared_position},"NORMAL":{shared_position}{colored}}},"indices":{shared_indices},"material":{}}}]}}"#,
            json_text(&object.name),
            materials.len() - 1
        ));
        let mass = object
            .mass_mearth
            .map_or(String::new(), |m| format!(r#","mass_mearth":{m}"#));
        let [x, y, z] = object.position.to_array();
        let [qx, qy, qz, qw] = object.tilt.to_array();
        let r = object.radius;
        let mut children = String::new();
        if let Some(ring) = object.ring {
            let (flat, flat_triangles) = annulus(ring);
            let up = vec![[0.0, 1.0, 0.0]; flat.len()];
            let position = writer.vec3s(&flat, true);
            let normal = writer.vec3s(&up, false);
            let indices = writer.indices(&flat_triangles);
            materials.push(
                r#"{"pbrMetallicRoughness":{"baseColorFactor":[0.8,0.74,0.62,0.6],"metallicFactor":0,"roughnessFactor":1},"alphaMode":"BLEND","doubleSided":true}"#
                    .to_string(),
            );
            meshes.push(format!(
                r#"{{"name":"{} rings","primitives":[{{"attributes":{{"POSITION":{position},"NORMAL":{normal}}},"indices":{indices},"material":{}}}]}}"#,
                json_text(&object.name),
                materials.len() - 1
            ));
            nodes.push(format!(
                r#"{{"name":"{} rings","mesh":{}}}"#,
                json_text(&object.name),
                meshes.len() - 1
            ));
            children = format!(r#","children":[{}]"#, nodes.len() - 1);
        }
        let mesh = meshes.len() - 1 - usize::from(object.ring.is_some());
        nodes.push(format!(
            r#"{{"name":"{}","mesh":{mesh}{children},"translation":[{x},{y},{z}],"rotation":[{qx},{qy},{qz},{qw}],"scale":[{r},{r},{r}],"extras":{{"kind":"{}"{mass}}}}}"#,
            json_text(&object.name),
            json_text(&object.kind),
        ));
        roots.push(nodes.len() - 1);
    }

    while !writer.bin.len().is_multiple_of(4) {
        writer.bin.push(0);
    }
    let list = |items: &[String]| items.join(",");
    let scene = roots
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let mut json = format!(
        r#"{{"asset":{{"version":"2.0","generator":"Stardust"}},"scene":0,"scenes":[{{"name":"Stardust","nodes":[{scene}]}}],"nodes":[{}],"meshes":[{}],"materials":[{}],"accessors":[{}],"bufferViews":[{}],"buffers":[{{"byteLength":{}}}]}}"#,
        list(&nodes),
        list(&meshes),
        list(&materials),
        list(&writer.accessors),
        list(&writer.views),
        writer.bin.len()
    )
    .into_bytes();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }

    let total = 12 + 8 + json.len() + 8 + writer.bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&json);
    out.extend_from_slice(&(writer.bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&writer.bin);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(bytes: &[u8], at: usize) -> usize {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
    }

    fn sample() -> Vec<Object> {
        vec![
            Object {
                name: "Vel\"ora".into(),
                kind: "star".into(),
                mass_mearth: None,
                position: Vec3::ZERO,
                radius: 0.1,
                tilt: Quat::IDENTITY,
                paint: Paint::Light([1.0, 0.9, 0.8]),
                ring: None,
            },
            Object {
                name: "Kesura".into(),
                kind: "gas_giant".into(),
                mass_mearth: Some(95.0),
                position: Vec3::new(3.0, 0.0, 1.0),
                radius: 0.05,
                tilt: Quat::from_rotation_z(0.4),
                paint: Paint::Plain([0.5, 0.4, 0.3]),
                ring: Some(Ring {
                    inner: 1.2,
                    outer: 2.2,
                }),
            },
        ]
    }

    #[test]
    fn the_file_has_the_layout_gltf_asks_for() {
        let file = glb(&sample());
        assert_eq!(&file[0..4], b"glTF");
        assert_eq!(word(&file, 4), 2);
        assert_eq!(word(&file, 8), file.len());
        let json_length = word(&file, 12);
        assert_eq!(&file[16..20], b"JSON");
        assert_eq!(json_length % 4, 0);
        let bin_at = 20 + json_length;
        let bin_length = word(&file, bin_at);
        assert_eq!(&file[bin_at + 4..bin_at + 8], b"BIN\0");
        assert_eq!(bin_length % 4, 0);
        assert_eq!(bin_at + 8 + bin_length, file.len());
    }

    #[test]
    fn names_are_escaped_and_the_json_parses() {
        let file = glb(&sample());
        let json = std::str::from_utf8(&file[20..20 + word(&file, 12)]).unwrap();
        assert!(json.contains(r#"Vel\"ora"#));
        assert!(json.contains(r#""mass_mearth":95"#));
        assert_eq!(json.matches('{').count(), json.matches('}').count());
    }

    #[test]
    fn the_same_objects_give_the_same_file() {
        assert_eq!(glb(&sample()), glb(&sample()));
    }

    #[test]
    fn a_sphere_faces_outward() {
        let (points, triangles) = sphere();
        for corner in triangles.chunks(3).skip(SPHERE_AROUND) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(points[corner[i] as usize]));
            let normal = (b - a).cross(c - a);
            if normal.length() > 1e-6 {
                assert!(normal.dot(a + b + c) > 0.0, "{corner:?}");
            }
        }
    }

    #[test]
    fn a_bake_gives_colours_in_range() {
        let result = sg_sim::simulate(&sg_schema::SimulationParams {
            seed: 3,
            ..sg_schema::SimulationParams::default()
        })
        .unwrap();
        for body in &result.snapshot.bodies {
            let look = crate::surface::appearance(body);
            for direction in [Vec3::X, Vec3::Y, Vec3::new(0.6, 0.0, 0.8)] {
                for channel in bake(&look, direction) {
                    assert!(channel.is_finite() && (0.0..=2.0).contains(&channel));
                }
            }
        }
    }
}
