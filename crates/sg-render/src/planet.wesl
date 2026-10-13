// Planet surfaces and atmospheres. See `planet.rs` for the uniform and
// `surface.rs` for where its numbers come from.
//
// One sphere per body, and a second, slightly larger one for its air
// (`flags.y = 1`). Everything is procedural: no textures. Lighting is done
// here, from up to three stars whose directions and colours arrive in the
// uniform, so that close pairs of suns light a planet from both sides.
//
// The atmosphere is an analytic single-scattering approximation, not the
// precomputed tables of Bruneton & Neyret (2008): limb brightening from
// the path length through a uniform shell, and reddening of starlight near
// the terminator by Rayleigh's law.

import bevy_pbr::render::forward_io::VertexOutput;

struct Planet {
    // xyz: camera position.
    eye: vec4<f32>,
    // Planet frame in world space: x toward longitude 0, y the north pole.
    frame_x: vec4<f32>,
    frame_y: vec4<f32>,
    frame_z: vec4<f32>,
    // xyz: unit direction to each star; w: its share of the light, 0 to 1.
    star_dir: array<vec4<f32>, 3>,
    // rgb: each star's colour.
    star_color: array<vec4<f32>, 3>,
    color_a: vec4<f32>,
    color_b: vec4<f32>,
    color_c: vec4<f32>,
    ocean_color: vec4<f32>,
    // rgb: scattered colour; a: density 0..1.
    air: vec4<f32>,
    // rgb: thermal glow; a: strength 0..1.
    glow: vec4<f32>,
    // sea level and cloud level in noise units (y: sine of the ice-cap
    // latitude; w: seed). Below 0 or above 1 means none or all.
    cover: vec4<f32>,
    // kind, band count, band contrast, storms.
    shape: vec4<f32>,
    // locked, is the air shell, apparent radius in pixels, shell height.
    flags: vec4<f32>,
    // cloud drift angle, exposure, unused, unused.
    extra: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> planet: Planet;

const RAYLEIGH: vec3<f32> = vec3<f32>(0.175, 0.410, 1.0);

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

// Fractal noise in 0..1, with as many octaves as the body's size on screen
// can show: detail finer than a pixel only shimmers.
fn fbm(p: vec3<f32>, octaves: i32) -> f32 {
    var sum = 0.0;
    var norm = 0.0;
    var amp = 0.5;
    var q = p;
    for (var i = 0; i < 7; i = i + 1) {
        if i >= octaves {
            break;
        }
        sum = sum + amp * value_noise(q);
        norm = norm + amp;
        amp = amp * 0.5;
        q = q * 2.03 + vec3<f32>(17.1, 3.7, 9.2);
    }
    return sum / max(norm, 1e-5);
}

// Sharp-crested noise in 0..1: cracks and ridges.
fn ridged(p: vec3<f32>, octaves: i32) -> f32 {
    var sum = 0.0;
    var norm = 0.0;
    var amp = 0.5;
    var q = p;
    for (var i = 0; i < 6; i = i + 1) {
        if i >= octaves {
            break;
        }
        let n = 1.0 - abs(2.0 * value_noise(q) - 1.0);
        sum = sum + amp * n * n;
        norm = norm + amp;
        amp = amp * 0.5;
        q = q * 2.07 + vec3<f32>(5.3, 11.9, 2.4);
    }
    return sum / max(norm, 1e-5);
}

fn octaves_for(pixels: f32) -> i32 {
    return clamp(i32(log2(max(pixels, 1.0))) - 1, 1, 7);
}

// Starlight reaching a point whose normal is `n`, with the reddening of a
// long path through air of the given density. `wrap` lets light bleed past
// the terminator, as it does through an atmosphere.
fn starlight(n: vec3<f32>, density: f32, wrap: f32) -> vec3<f32> {
    var light = vec3<f32>(0.0);
    for (var i = 0; i < 3; i = i + 1) {
        let weight = planet.star_dir[i].w;
        if weight <= 0.0 {
            continue;
        }
        let mu = dot(n, planet.star_dir[i].xyz);
        let lit = clamp((mu + wrap) / (1.0 + wrap), 0.0, 1.0);
        // Air mass grows as 1/μ toward the terminator.
        let path = min(1.0 / max(mu + 0.08, 0.04), 14.0) - 0.9;
        let through = exp(-RAYLEIGH * density * 0.22 * max(path, 0.0));
        light = light + planet.star_color[i].rgb * weight * lit * through;
    }
    return light;
}

// How strongly the first star stands overhead, for locked worlds.
fn substellar(n: vec3<f32>) -> f32 {
    return max(dot(n, planet.star_dir[0].xyz), 0.0);
}

struct Surface {
    albedo: vec3<f32>,
    emission: vec3<f32>,
    // 0 matte, 1 mirror-like water.
    gloss: f32,
}

fn rock(p: vec3<f32>, octaves: i32) -> Surface {
    let h = fbm(p * 2.2, octaves);
    let fine = fbm(p * 9.0 + 40.0, octaves);
    var albedo = mix(planet.color_a.rgb, planet.color_b.rgb, smoothstep(0.3, 0.7, h));
    albedo = mix(albedo, planet.color_c.rgb, smoothstep(0.55, 0.8, fine) * 0.6);
    return Surface(albedo, vec3<f32>(0.0), 0.0);
}

fn land_and_sea(p: vec3<f32>, latitude_sin: f32, octaves: i32) -> Surface {
    let warp = vec3<f32>(fbm(p * 1.7 + 11.0, 3), fbm(p * 1.7 + 23.0, 3), fbm(p * 1.7 + 37.0, 3));
    let h = fbm(p * 1.6 + 1.5 * warp, octaves);
    // The sea level is the height below which the model's fraction of the
    // surface lies (`planet.rs`: `noise_quantile`).
    let sea_level = planet.cover.x;
    let edge = 0.006;
    let water = 1.0 - smoothstep(sea_level - edge, sea_level + edge, h);
    let height = clamp((h - sea_level) / max(1.0 - sea_level, 0.05), 0.0, 1.0);
    var land = mix(planet.color_a.rgb, planet.color_b.rgb, smoothstep(0.05, 0.45, height));
    land = mix(land, planet.color_c.rgb, smoothstep(0.5, 0.9, height));
    let depth = clamp((sea_level - h) * 6.0, 0.0, 1.0);
    let sea = mix(planet.ocean_color.rgb * 1.8, planet.ocean_color.rgb * 0.7, depth);
    var albedo = mix(land, sea, water);
    var gloss = water;
    // Polar ice, with a ragged edge.
    let cap = planet.cover.y;
    if cap < 0.999 {
        let ragged = (fbm(p * 5.0 + 60.0, octaves) - 0.5) * 0.12;
        let ice = smoothstep(cap - 0.02, cap + 0.02, abs(latitude_sin) + ragged);
        albedo = mix(albedo, vec3<f32>(0.90, 0.93, 0.96), ice);
        gloss = gloss * (1.0 - ice);
    }
    return Surface(albedo, vec3<f32>(0.0), gloss);
}

fn ice(p: vec3<f32>, octaves: i32) -> Surface {
    let h = fbm(p * 2.0, octaves);
    let cracks = ridged(p * 3.5 + 9.0, min(octaves, 5));
    var albedo = mix(planet.color_a.rgb, planet.color_b.rgb, smoothstep(0.35, 0.7, h));
    albedo = mix(albedo, planet.color_c.rgb, smoothstep(0.80, 0.94, cracks) * 0.7);
    return Surface(albedo, vec3<f32>(0.0), 0.25);
}

fn lava(p: vec3<f32>, n: vec3<f32>, octaves: i32) -> Surface {
    let h = fbm(p * 2.4, octaves);
    let cracks = ridged(p * 3.0 + 4.0, min(octaves, 5));
    let albedo = mix(planet.color_a.rgb, planet.color_b.rgb, smoothstep(0.3, 0.7, h));
    // Molten rock shows in the cracks, and everywhere under the star of a
    // locked world.
    var molten = smoothstep(0.78, 0.93, cracks);
    if planet.flags.x > 0.5 {
        let noon = substellar(n);
        molten = max(molten, smoothstep(0.35, 0.9, noon + 0.35 * (h - 0.5)));
    }
    let emission = planet.glow.rgb * planet.glow.a * molten * 3.0;
    return Surface(mix(albedo, planet.color_c.rgb, molten), emission, 0.0);
}

fn bands(p: vec3<f32>, latitude_sin: f32, octaves: i32, hazy: bool) -> Surface {
    let count = planet.shape.y;
    let contrast = planet.shape.z;
    let latitude = asin(clamp(latitude_sin, -1.0, 1.0));
    // Stretch the turbulence along the bands.
    let q = vec3<f32>(p.x, p.y * 4.0, p.z);
    let swirl = fbm(q * 3.0 + 7.0, octaves) - 0.5;
    // Waves along each band's edge, and slow meanders of the whole pattern.
    let meander = fbm(p * 1.3 + 19.0, 3) - 0.5;
    let phase = latitude * count + select(5.0, 1.5, hazy) * swirl + select(2.5, 1.0, hazy) * meander;
    let wave = 0.5 + 0.5 * sin(phase);
    // Belts are narrower and sharper-edged than zones.
    let belt = smoothstep(0.35, 0.65, wave);
    let fine = fbm(q * 9.0 + 3.0, octaves) - 0.5;
    var albedo = mix(planet.color_a.rgb, planet.color_b.rgb, clamp(belt + 0.5 * fine, 0.0, 1.0) * contrast);
    // A few large ovals.
    let storms = planet.shape.w;
    if storms > 0.0 && !hazy {
        let cell = fbm(vec3<f32>(p.x * 2.0, p.y * 6.0, p.z * 2.0) + 51.0, min(octaves, 3));
        let oval = smoothstep(0.80 - 0.08 * storms, 0.86, cell);
        albedo = mix(albedo, planet.color_c.rgb, oval * contrast);
    }
    // The poles darken a little, as on Jupiter and Saturn.
    albedo = albedo * (1.0 - 0.25 * smoothstep(0.75, 1.0, abs(latitude_sin)));
    return Surface(albedo, vec3<f32>(0.0), 0.0);
}

// Local position on the unit sphere, rotated by `angle` about the pole.
fn turned(p: vec3<f32>, angle: f32) -> vec3<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return vec3<f32>(c * p.x + s * p.z, p.y, c * p.z - s * p.x);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let v = normalize(planet.eye.xyz - in.world_position.xyz);
    let exposure = planet.extra.y;
    let density = planet.air.a;

    // The shell of air around the body.
    if planet.flags.y > 0.5 {
        let h = planet.flags.w;
        let outer = 1.0 + h;
        let mu = clamp(dot(n, v), 0.0, 1.0);
        // Closest approach of the line of sight to the centre, in radii.
        let b = outer * sqrt(1.0 - mu * mu);
        var path = 0.0;
        if b < 1.0 {
            path = sqrt(outer * outer - b * b) - sqrt(1.0 - b * b);
        } else {
            // Past the limb: the whole chord, thinning with height.
            path = 2.0 * sqrt(max(outer * outer - b * b, 0.0)) * exp(-(b - 1.0) / (0.3 * h));
        }
        let depth = density * path / h;
        let opacity = 1.0 - exp(-0.55 * depth);
        let light = starlight(n, density, 0.12);
        let scattered = planet.air.rgb * light * opacity * exposure;
        // Premultiplied, mostly additive: air glows, it does not hide.
        return vec4<f32>(scattered, opacity * 0.35 * clamp(dot(light, vec3<f32>(0.4)), 0.0, 1.0));
    }

    let local = vec3<f32>(
        dot(n, planet.frame_x.xyz),
        dot(n, planet.frame_y.xyz),
        dot(n, planet.frame_z.xyz),
    );
    let seed = planet.cover.w;
    let p = local + vec3<f32>(seed * 0.37, seed * 0.11, seed * 0.23);
    let octaves = octaves_for(planet.flags.z);
    let kind = i32(planet.shape.x + 0.5);

    var surface: Surface;
    if kind == 1 {
        surface = land_and_sea(p, local.y, octaves);
    } else if kind == 2 {
        surface = ice(p, octaves);
    } else if kind == 3 {
        surface = lava(p, n, octaves);
    } else if kind == 4 {
        surface = bands(p, local.y, octaves, false);
    } else if kind == 5 {
        surface = bands(p, local.y, octaves, true);
    } else {
        surface = rock(p, octaves);
    }

    // Clouds drift over the surface.
    let cloud_level = planet.cover.z;
    var cloud = 0.0;
    if cloud_level < 1.5 && kind != 4 && kind != 5 {
        let cp = turned(local, planet.extra.x) + vec3<f32>(seed * 0.19 + 80.0);
        let cw = vec3<f32>(fbm(cp * 2.0 + 5.0, 3), fbm(cp * 2.0 + 15.0, 3), fbm(cp * 2.0 + 25.0, 3));
        let c = fbm(vec3<f32>(cp.x, cp.y * 1.6, cp.z) * 2.6 + 1.2 * cw, octaves);
        cloud = smoothstep(cloud_level - 0.025, cloud_level + 0.025, c);
    }
    let albedo = mix(surface.albedo, vec3<f32>(0.92), cloud);
    let gloss = surface.gloss * (1.0 - cloud);

    let wrap = 0.12 * density;
    let light = starlight(n, density, wrap);
    var color = albedo * (light + vec3<f32>(0.006));

    // The glint of each star on open water.
    if gloss > 0.01 {
        for (var i = 0; i < 3; i = i + 1) {
            let weight = planet.star_dir[i].w;
            if weight <= 0.0 {
                continue;
            }
            let l = planet.star_dir[i].xyz;
            let half_way = normalize(l + v);
            let shine = pow(max(dot(n, half_way), 0.0), 90.0) * step(0.0, dot(n, l));
            color = color + planet.star_color[i].rgb * weight * shine * gloss * 2.5;
        }
    }

    // Air between the eye and the ground: more of it toward the limb.
    if density > 0.0 {
        let rim = pow(1.0 - clamp(dot(n, v), 0.0, 1.0), 2.5);
        let haze = clamp(density * (0.06 + 0.75 * rim), 0.0, 0.85);
        color = mix(color, planet.air.rgb * starlight(n, density, 0.12), haze);
    }

    // The body's own heat: surface emission, and for a hot giant the glow
    // of the whole disc, seen best on the night side.
    var emission = surface.emission;
    if kind == 4 || kind == 5 {
        emission = planet.glow.rgb * planet.glow.a * (0.25 + 0.5 * surface.albedo);
    }
    color = color + emission * (1.0 - cloud);

    return vec4<f32>(color * exposure, 1.0);
}
