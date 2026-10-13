// A star's visible surface. See `star.rs`.
//
// Limb darkening by the linear law, with its dependence on colour, and for
// stars with convective envelopes a pattern of granules. The granules are
// drawn tens of thousands of times larger than real ones (the Sun's are
// about a thousand kilometres across, far below a pixel at any distance
// from which the disc is seen whole): texture, not measurement.

import bevy_pbr::render::forward_io::VertexOutput;

struct Star {
    // xyz: camera position.
    eye: vec4<f32>,
    // rgb: blackbody colour; a: brightness at the centre of the disc.
    color: vec4<f32>,
    // Linear limb-darkening coefficient for red, green and blue.
    limb: vec4<f32>,
    // granulation contrast, clock, apparent radius in pixels, seed.
    detail: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> star: Star;

fn hash3(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.1031);
    q = q + dot(q, q.zyx + 31.32);
    return fract((q.x + q.y) * q.z);
}

fn value_noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(hash3(i), hash3(i + vec3<f32>(1.0, 0.0, 0.0)), u.x);
    let b = mix(hash3(i + vec3<f32>(0.0, 1.0, 0.0)), hash3(i + vec3<f32>(1.0, 1.0, 0.0)), u.x);
    let c = mix(hash3(i + vec3<f32>(0.0, 0.0, 1.0)), hash3(i + vec3<f32>(1.0, 0.0, 1.0)), u.x);
    let d = mix(hash3(i + vec3<f32>(0.0, 1.0, 1.0)), hash3(i + vec3<f32>(1.0, 1.0, 1.0)), u.x);
    return mix(mix(a, b, u.y), mix(c, d, u.y), u.z);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let v = normalize(star.eye.xyz - in.world_position.xyz);
    let mu = clamp(dot(n, v), 0.0, 1.0);
    // I(μ)/I(1) = 1 − u(1 − μ), per colour.
    let darkening = vec3<f32>(1.0) - star.limb.rgb * (1.0 - mu);

    var mottle = 1.0;
    let contrast = star.detail.x;
    let pixels = star.detail.z;
    if contrast > 0.0 && pixels > 12.0 {
        let p = n + vec3<f32>(star.detail.w);
        let t = star.detail.y;
        // Bright cells with dark lanes between, slowly boiling; a coarser
        // layer stands for the larger convective pattern.
        let fine = value_noise(p * 26.0 + vec3<f32>(0.0, t, 0.0));
        let cells = 1.0 - abs(2.0 * fine - 1.0);
        let coarse = value_noise(p * 7.0 + vec3<f32>(t * 0.4, 0.0, 9.0));
        let fade = smoothstep(12.0, 60.0, pixels);
        mottle = 1.0 + contrast * fade * (0.55 * (cells - 0.5) + 0.45 * (coarse - 0.5));
    }
    return vec4<f32>(star.color.rgb * star.color.a * darkening * mottle, 1.0);
}
