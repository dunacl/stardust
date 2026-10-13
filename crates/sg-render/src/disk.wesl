// The disk of gas and dust during formation playback. See `disk.rs`.
//
// A flat ring around the host, drawn procedurally: how opaque it is at
// each distance comes from the model (the gas profile, the gaps giants
// open, the lanes swept clear); the streaks are noise sheared as orbits
// shear, so that the disk reads as turning. The streaks are texture, not
// modelled structure.

import bevy_pbr::render::forward_io::VertexOutput;

struct DiskUniform {
    // xyz: the host's position.
    centre: vec4<f32>,
    // inner and outer scene radius drawn; clock; overall opacity.
    extent: vec4<f32>,
    // rgb: colour of the lit dust.
    color: vec4<f32>,
    // Opacity at 64 evenly spaced scene radii from inner to outer.
    samples: array<vec4<f32>, 16>,
    // Gaps: x scene radius, y half-width, z depth 0..1.
    gaps: array<vec4<f32>, 8>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> disk: DiskUniform;

fn hash2(p: vec2<f32>) -> f32 {
    var q = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    q = q + dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn noise2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(hash2(i), hash2(i + vec2<f32>(1.0, 0.0)), u.x),
        mix(hash2(i + vec2<f32>(0.0, 1.0)), hash2(i + vec2<f32>(1.0, 1.0)), u.x),
        u.y,
    );
}

fn sample_at(index: i32) -> f32 {
    let k = clamp(index, 0, 63);
    return disk.samples[k / 4][k % 4];
}

// Streaks at radius fraction `u` and angle `phi`, wound by `turns` of
// differential rotation.
fn streaks(u: f32, phi: f32, turns: f32) -> f32 {
    // Inner orbits run ahead of outer ones.
    let wound = phi + turns * 3.0 / (0.35 + u);
    let a = vec2<f32>(cos(wound), sin(wound)) * (2.0 + 5.0 * u);
    let coarse = noise2(a * 1.3 + vec2<f32>(u * 9.0, 0.0));
    let fine = noise2(a * 4.1 + vec2<f32>(u * 41.0, 7.0));
    return 0.6 * coarse + 0.4 * fine;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let d = in.world_position.xz - disk.centre.xz;
    let rho = length(d);
    let inner = disk.extent.x;
    let outer = disk.extent.y;
    if rho < inner || rho > outer {
        discard;
    }
    let u = (rho - inner) / max(outer - inner, 1e-9);
    let x = u * 63.0;
    let i = i32(floor(x));
    var opacity = mix(sample_at(i), sample_at(i + 1), fract(x));

    for (var g = 0; g < 8; g = g + 1) {
        let gap = disk.gaps[g];
        if gap.z <= 0.0 {
            continue;
        }
        let across = abs(rho - gap.x) / max(gap.y, 1e-9);
        opacity = opacity * (1.0 - gap.z * (1.0 - smoothstep(0.6, 1.0, across)));
    }

    // Two copies of the streak pattern, half a cycle apart, cross-faded so
    // that the winding never runs away.
    let phi = atan2(d.y, d.x);
    let cycle = disk.extent.z;
    let t1 = fract(cycle);
    let t2 = fract(cycle + 0.5);
    let w1 = 1.0 - abs(2.0 * t1 - 1.0);
    let pattern = w1 * streaks(u, phi, t1 - 0.5) + (1.0 - w1) * streaks(u, phi, t2 - 0.5);
    let texture = 0.55 + 0.9 * (pattern - 0.5);

    // Soft edges.
    let edge = smoothstep(0.0, 0.02, u) * (1.0 - smoothstep(0.93, 1.0, u));
    let alpha = clamp(opacity * texture * edge * disk.extent.w, 0.0, 0.9);
    if alpha < 0.003 {
        discard;
    }
    return vec4<f32>(disk.color.rgb * alpha, alpha * 0.6);
}
