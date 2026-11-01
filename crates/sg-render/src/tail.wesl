// A comet's tails and coma. See `comet.rs`.
//
// Each is a flat quad turned to the camera. The nucleus, the directions and
// the length come from `comet.rs`; the falloff here is a drawing choice.

import bevy_pbr::render::forward_io::VertexOutput;

struct Tail {
    // xyz: the nucleus; w: the length of the tail (radius for a coma).
    origin: vec4<f32>,
    // xyz: unit direction of the tail; w: 1 for a coma.
    axis: vec4<f32>,
    // xyz: unit direction across it; w: half its width at the far end.
    side: vec4<f32>,
    // rgb: colour; a: brightness.
    color: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> tail: Tail;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let from_nucleus = in.world_position.xyz - tail.origin.xyz;
    let length = tail.origin.w;
    var glow = 0.0;
    if tail.axis.w > 0.5 {
        // A coma: bright at the head, fading as one over distance does.
        let r = sqrt(dot(from_nucleus, from_nucleus)) / length;
        glow = max(1.0 - r, 0.0) * max(1.0 - r, 0.0) / (0.06 + 3.0 * r);
    } else {
        let along = dot(from_nucleus, tail.axis.xyz) / length;
        let across = dot(from_nucleus, tail.side.xyz) / tail.side.w;
        // Widens from the head, and thins as it goes.
        let width = max(0.05 + 0.95 * sqrt(max(along, 0.0)), 0.01);
        let edge = exp(-3.5 * (across / width) * (across / width));
        let fade = smoothstep(0.0, 0.02, along) * (1.0 - smoothstep(0.1, 1.0, along));
        glow = edge * fade;
    }
    let light = tail.color.rgb * tail.color.a * glow;
    // Premultiplied and additive: a tail shines, it does not hide.
    return vec4<f32>(light, 0.0);
}
