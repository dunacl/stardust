// A star's visible surface. See `star.rs`.
//
// Limb darkening by the linear law, with its dependence on colour, and for
// stars with convective envelopes a pattern of granules. The granules are
// drawn tens of thousands of times larger than real ones (the Sun's are
// about a thousand kilometres across, far below a pixel at any distance
// from which the disc is seen whole): texture, not measurement. So are the
// sunspots: stars with convective envelopes have them, in belts either side
// of the equator as the Sun does, but their places are noise.

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

    var mottle = vec3<f32>(1.0);
    let contrast = star.detail.x;
    // The surface turns about the pole, by the clock. Slower than the
    // planets, as a drawing choice; the Sun takes about 25 days.
    let turn = star.detail.y * 0.45;
    let turned = vec3<f32>(cos(turn) * n.x + sin(turn) * n.z, n.y, cos(turn) * n.z - sin(turn) * n.x);
    let pixels = star.detail.z;
    if contrast > 0.0 && pixels > 12.0 {
        let p = turned + vec3<f32>(star.detail.w);
        let t = star.detail.y;
        // Bright cells with dark lanes between, slowly boiling, at sizes
        // that appear as the star fills the view; a coarser layer stands
        // for the larger convective pattern.
        var cells = 0.0;
        var weight = 0.5;
        var scale = 26.0;
        for (var i = 0; i < 4; i = i + 1) {
            let show = smoothstep(scale * 0.5, scale * 2.0, pixels * 0.25);
            let fine = value_noise(p * scale + vec3<f32>(f32(i) * 7.0, t, 0.0));
            cells = cells + weight * (select(show, 1.0, i == 0)) * (1.0 - abs(2.0 * fine - 1.0) - 0.5);
            weight = weight * 0.7;
            scale = scale * 2.7;
        }
        let coarse = value_noise(p * 7.0 + vec3<f32>(t * 0.4, 0.0, 9.0));
        let fade = smoothstep(12.0, 60.0, pixels);
        // Close up the lanes need more contrast to survive the bright disc.
        let lanes = mix(1.0, 3.2, smoothstep(150.0, 700.0, pixels));
        let surface = 1.0 + contrast * fade * (lanes * cells + 0.35 * (coarse - 0.5));
        mottle = vec3<f32>(surface);

        // Spots, in belts between about 5 and 35 degrees of latitude.
        let latitude = abs(n.y);
        let belt = smoothstep(0.08, 0.2, latitude) * (1.0 - smoothstep(0.5, 0.62, latitude));
        let wobble = value_noise(p * 40.0) * 0.05;
        let spot = value_noise(p * 13.0 + vec3<f32>(0.0, 3.0, t * 0.05)) * 0.7
            + value_noise(p * 31.0 + vec3<f32>(5.0)) * 0.3 + wobble;
        let penumbra = smoothstep(0.74, 0.77, spot) * belt * fade * contrast;
        let umbra = smoothstep(0.79, 0.82, spot) * belt * fade * contrast;
        // Cooler, so darker and redder, as well as darker still at the core.
        mottle = mottle * mix(vec3<f32>(1.0), vec3<f32>(0.5, 0.36, 0.26), penumbra);
        mottle = mottle * mix(vec3<f32>(1.0), vec3<f32>(0.22, 0.13, 0.09), umbra);

        // Bright faculae, which show best toward the limb.
        let facula = smoothstep(0.55, 0.68, value_noise(p * 18.0 + vec3<f32>(2.0, 0.0, t * 0.1)));
        mottle = mottle * (1.0 + 0.35 * contrast * fade * belt * facula * (1.0 - mu));
    }
    return vec4<f32>(star.color.rgb * star.color.a * darkening * mottle, 1.0);
}
