// A planet's rings. See `ring.rs`.
//
// A flat annulus in the planet's equatorial plane. From the model: where
// the rings begin and end and how opaque they are overall. Drawn: the
// pattern of ringlets and gaps (noise in radius), and the colour. The
// planet's shadow falls across the rings; the rings' shadow on the planet
// is not drawn.

import bevy_pbr::render::forward_io::VertexOutput;

struct Ring {
    // xyz: the planet's centre; w: its radius as drawn.
    planet: vec4<f32>,
    // xyz: unit direction to the brightest star; w: unused.
    star_dir: vec4<f32>,
    // rgb: the star's colour times the ring's own; a: exposure.
    color: vec4<f32>,
    // xyz: camera position.
    eye: vec4<f32>,
    // xyz: unit normal of the ring plane.
    normal: vec4<f32>,
    // inner radius, outer radius (as drawn), optical depth, seed.
    shape: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> ring: Ring;

fn hash1(x: f32) -> f32 {
    return fract(sin(x * 127.1) * 43758.5453);
}

fn noise1(x: f32) -> f32 {
    let i = floor(x);
    let f = fract(x);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(hash1(i), hash1(i + 1.0), u);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let from_centre = in.world_position.xyz - ring.planet.xyz;
    let r = length(from_centre);
    let inner = ring.shape.x;
    let outer = ring.shape.y;
    if r < inner || r > outer {
        discard;
    }
    let u = (r - inner) / max(outer - inner, 1e-9);
    let seed = ring.shape.w;

    // Ringlets at three scales, and one or two wide gaps.
    let broad = noise1(u * 5.0 + seed);
    let medium = noise1(u * 23.0 + seed * 3.1);
    let fine = noise1(u * 97.0 + seed * 7.7);
    var density = 0.25 + 0.75 * (0.55 * broad + 0.3 * medium + 0.15 * fine);
    let gap = noise1(u * 3.0 + seed * 1.7 + 40.0);
    density = density * smoothstep(0.12, 0.2, abs(gap - 0.5) + 0.08);
    // Denser in the middle than at either edge, as Saturn's B ring is.
    density = density * (0.45 + 0.55 * sin(3.14159 * clamp(u, 0.0, 1.0)));
    let edge = smoothstep(0.0, 0.02, u) * (1.0 - smoothstep(0.98, 1.0, u));

    // Seen at a slant, the rings are thicker along the line of sight.
    let v = normalize(ring.eye.xyz - in.world_position.xyz);
    let slant = max(abs(dot(v, ring.normal.xyz)), 0.08);
    let depth = ring.shape.z * density * edge;
    let alpha = 1.0 - exp(-depth / slant);

    // The planet's shadow: does the ray toward the star hit the planet?
    let l = ring.star_dir.xyz;
    let along = dot(-from_centre, l);
    let closest_sq = dot(from_centre, from_centre) - along * along;
    let radius = ring.planet.w;
    var lit = 1.0;
    if along > 0.0 {
        lit = smoothstep(radius * radius * 0.96, radius * radius * 1.04, closest_sq);
    }
    // Sunlight on the ring's face, a little even when the star is nearly
    // in the ring's plane.
    let face = 0.35 + 0.65 * abs(dot(l, ring.normal.xyz));
    let brightness = (0.75 + 0.25 * fine) * face * (0.04 + 0.96 * lit);
    let color = ring.color.rgb * brightness * ring.color.a;
    return vec4<f32>(color * alpha, alpha);
}
