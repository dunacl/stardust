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
    for (var i = 0; i < 9; i = i + 1) {
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
    return clamp(i32(log2(max(pixels, 1.0))) - 1, 1, 9);
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
    // Slope of the ground in the planet's frame, added to the normal for
    // lighting. Zero on anything that has no relief drawn.
    tilt: vec3<f32>,
}

// What the craters do to a point.
struct Craters {
    // Added to the albedo as a factor: below 0 darkens, above 0 brightens.
    shade: f32,
    tilt: vec3<f32>,
}

// Slope of a crater `x` radii from its centre, positive where the ground
// rises outward: a flat floor, a steep inner wall up to a sharp rim, then a
// gentle outer flank.
fn crater_slope(x: f32) -> f32 {
    return 1.6 * smoothstep(0.55, 0.9, x) * (1.0 - smoothstep(0.93, 1.0, x))
        - 0.35 * smoothstep(1.0, 1.04, x) * (1.0 - smoothstep(1.04, 1.3, x));
}

// Height of rough ground, for relief too fine to draw as craters.
fn rubble(p: vec3<f32>) -> f32 {
    return fbm(p * 34.0, 4) + 0.35 * ridged(p * 90.0 + 13.0, 3);
}

// Slope of that ground, by differences; `step_size` is in its own units.
fn rubble_tilt(p: vec3<f32>, step_size: f32) -> vec3<f32> {
    let h = rubble(p);
    return (h - vec3<f32>(
        rubble(p + vec3<f32>(step_size, 0.0, 0.0)),
        rubble(p + vec3<f32>(0.0, step_size, 0.0)),
        rubble(p + vec3<f32>(0.0, 0.0, step_size)),
    )) / step_size;
}

// One size of crater: a jittered lattice around the unit sphere `local`, of
// which `presence` of the cells near the surface hold one. Distance is
// measured along the surface, so a crater is a full circle wherever its
// centre lies in the shell. The eight cells around the point see every
// crater that can reach it: a centre moves at most 0.25 of a cell, and a
// crater reaches 0.22 along the surface and 0.12 through the shell, 0.25 in
// all.
fn crater_layer(local: vec3<f32>, scale: f32, salt: f32, presence: f32, strength: f32) -> Craters {
    var out = Craters(0.0, vec3<f32>(0.0));
    let q = local * scale;
    let base = floor(q - 0.5);
    for (var i = 0; i < 8; i = i + 1) {
        let cell = base + vec3<f32>(f32(i & 1), f32((i >> 1) & 1), f32((i >> 2) & 1));
        let key = cell + salt;
        if hash3(key) > presence {
            continue;
        }
        let jitter = vec3<f32>(hash3(key + 11.0), hash3(key + 23.0), hash3(key + 37.0)) - 0.5;
        let d = q - (cell + 0.5 + 0.5 * jitter);
        let radial = dot(d, local);
        if abs(radial) > 0.12 {
            continue;
        }
        let along = d - local * radial;
        let span = length(along);
        let age = hash3(key + 53.0);
        let radius = mix(0.08, 0.17, age * age);
        if span >= radius * 1.45 {
            continue;
        }
        let dir = along / max(span, 1e-5);
        // No rim is a true circle: slumps and younger hits break it up.
        let rough = fbm(dir * 3.0 + key, 3) - 0.5;
        let x = (span / radius) * (1.0 + 0.3 * rough);
        // Old craters are softened; the freshest keep bright walls.
        let fresh = smoothstep(0.75, 1.0, hash3(key + 71.0));
        let depth = strength * mix(0.55, 1.0, fresh);
        out.tilt = out.tilt - dir * crater_slope(x) * depth;
        out.shade = out.shade - 0.12 * (1.0 - smoothstep(0.6, 0.95, x)) * (1.0 - fresh)
            + fresh * 0.35 * (1.0 - smoothstep(0.8, 1.3, x));
    }
    return out;
}

// Craters at the sizes the body's size on screen can show, from basins
// down. Their sizes and places are a drawing choice, as all terrain is
// (`surface.rs`).
fn craters(local: vec3<f32>, seed: f32, octaves: i32) -> Craters {
    var out = Craters(0.0, vec3<f32>(0.0));
    for (var level = 0; level < 6; level = level + 1) {
        if octaves < level + 1 {
            break;
        }
        let f = f32(level);
        let layer = crater_layer(local, 1.4 * pow(2.0, f), 100.0 + seed + 17.0 * f, 1.0, 0.9 - 0.1 * f);
        out.shade = out.shade + layer.shade;
        out.tilt = out.tilt + layer.tilt;
    }
    return out;
}

fn rock(p: vec3<f32>, local: vec3<f32>, octaves: i32) -> Surface {
    let h = fbm(p * 2.2, octaves);
    let fine = fbm(p * 9.0 + 40.0, octaves);
    var albedo = mix(planet.color_a.rgb, planet.color_b.rgb, smoothstep(0.3, 0.7, h));
    albedo = mix(albedo, planet.color_c.rgb, smoothstep(0.55, 0.8, fine) * 0.6);
    // Dark plains, flooded long ago: few craters survive on them.
    let plains = smoothstep(0.52, 0.60, fbm(p * 1.1 + 70.0, min(octaves, 4)));
    albedo = albedo * (1.0 - 0.32 * plains);
    let marks = craters(local, planet.cover.w, octaves);
    let kept = 1.0 - 0.75 * plains;
    albedo = albedo * clamp(1.0 + marks.shade * kept, 0.4, 1.6);
    var tilt = marks.tilt * kept;
    // Close up, the ground itself is rough, on plains less than in the
    // highlands.
    if octaves >= 6 {
        tilt = tilt + 0.006 * (1.0 - 0.5 * plains) * rubble_tilt(p, 0.0015);
        albedo = albedo * (0.86 + 0.28 * fbm(p * 60.0, 4));
    }
    return Surface(albedo, vec3<f32>(0.0), 0.0, tilt);
}

fn land_and_sea(p: vec3<f32>, latitude_sin: f32, octaves: i32) -> Surface {
    let warp = vec3<f32>(fbm(p * 1.7 + 11.0, 3), fbm(p * 1.7 + 23.0, 3), fbm(p * 1.7 + 37.0, 3));
    var h = fbm(p * 1.6 + 1.5 * warp, octaves);
    // Close up, coasts break into bays and islands. This hardly moves the
    // sea level, which the model's fraction sets.
    if octaves >= 5 {
        h = h + 0.035 * (fbm(p * 26.0, 5) - 0.5);
    }
    // The sea level is the height below which the model's fraction of the
    // surface lies (`planet.rs`: `noise_quantile`).
    let sea_level = planet.cover.x;
    let edge = 0.004;
    let water = 1.0 - smoothstep(sea_level - edge, sea_level + edge, h);
    let height = clamp((h - sea_level) / max(1.0 - sea_level, 0.05), 0.0, 1.0);
    var land = mix(planet.color_a.rgb, planet.color_b.rgb, smoothstep(0.05, 0.45, height));
    land = mix(land, planet.color_c.rgb, smoothstep(0.5, 0.9, height));
    // Dry belts near 30 degrees, as Hadley circulation leaves them. The
    // tint and where it falls are a drawing choice: the model has no rain.
    let belt = 1.0 - smoothstep(0.08, 0.26, abs(abs(latitude_sin) - 0.48 + 0.3 * (fbm(p * 3.1 + 88.0, 4) - 0.5)));
    land = mix(land, vec3<f32>(0.46, 0.34, 0.19), 0.7 * belt * (1.0 - smoothstep(0.4, 0.7, height)));
    land = land * (0.8 + 0.4 * fbm(p * 14.0 + 3.0, min(octaves, 6)));
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
    var tilt = vec3<f32>(0.0);
    if octaves >= 5 {
        tilt = 0.008 * (1.0 - water) * rubble_tilt(p * 0.5, 0.002);
    }
    return Surface(albedo, vec3<f32>(0.0), gloss, tilt);
}

fn ice(p: vec3<f32>, octaves: i32) -> Surface {
    let h = fbm(p * 2.0, octaves);
    var albedo = mix(planet.color_a.rgb, planet.color_b.rgb, smoothstep(0.35, 0.7, h));
    // Long lines where the crust has split, at two sizes, stained by what
    // came up through them.
    let wide = ridged(p * 3.5 + 9.0, min(octaves, 6));
    let thin = ridged(p * 9.0 + 31.0, min(octaves, 6));
    let lines = max(smoothstep(0.84, 0.93, wide), 0.65 * smoothstep(0.85, 0.94, thin));
    albedo = mix(albedo, planet.color_c.rgb, lines * 0.85);
    albedo = albedo * (0.92 + 0.16 * fbm(p * 22.0, min(octaves, 6)));
    var tilt = vec3<f32>(0.0);
    if octaves >= 5 {
        tilt = 0.003 * rubble_tilt(p, 0.0015);
    }
    return Surface(albedo, vec3<f32>(0.0), 0.08, tilt);
}

fn lava(p: vec3<f32>, n: vec3<f32>, octaves: i32) -> Surface {
    let h = fbm(p * 2.4, octaves);
    let cracks = ridged(p * 3.0 + 4.0, min(octaves, 6));
    let albedo = mix(planet.color_a.rgb, planet.color_b.rgb, smoothstep(0.3, 0.7, h));
    // Molten rock shows in the cracks, and under the star of a locked world
    // as open seas with rafts of crust still floating on them.
    var molten = smoothstep(0.74, 0.90, cracks);
    if planet.flags.x > 0.5 {
        let noon = substellar(n);
        let heat = smoothstep(0.1, 0.8, noon + 0.35 * (h - 0.5));
        let rafts = smoothstep(0.42, 0.62, fbm(p * 6.0 + 3.0 * h + 21.0, octaves));
        molten = max(molten, heat * rafts);
    }
    // The hottest melt is brightest, so the seas are not one flat colour.
    let bright = 0.45 + 1.1 * fbm(p * 12.0 + 51.0, octaves);
    let emission = planet.glow.rgb * planet.glow.a * molten * 2.0 * bright;
    var tilt = vec3<f32>(0.0);
    if octaves >= 5 {
        tilt = 0.006 * (1.0 - molten) * rubble_tilt(p, 0.0015);
    }
    return Surface(mix(albedo, planet.color_c.rgb, molten), emission, 0.0, tilt);
}

fn bands(p: vec3<f32>, latitude_sin: f32, octaves: i32, hazy: bool) -> Surface {
    let count = planet.shape.y;
    let contrast = planet.shape.z;
    let latitude = asin(clamp(latitude_sin, -1.0, 1.0));
    // Stretch the turbulence along the bands.
    let q = vec3<f32>(p.x, p.y * 4.0, p.z);
    // Eddies carried along by eddies: each layer's noise is displaced by
    // the one before, which curls it as shear does.
    let swirl = fbm(q * 2.2 + 7.0, min(octaves, 5)) - 0.5;
    let curl = fbm(q * 5.0 + 6.0 * swirl + 21.0, octaves) - 0.5;
    // Waves along each band's edge, and slow meanders of the whole pattern.
    let meander = fbm(p * 1.3 + 19.0, 3) - 0.5;
    let phase = latitude * count + select(5.0, 1.5, hazy) * (swirl + 0.5 * curl) + select(2.5, 1.0, hazy) * meander;
    let wave = 0.5 + 0.5 * sin(phase);
    // Belts are narrower and sharper-edged than zones.
    let belt = smoothstep(0.35, 0.65, wave);
    let fine = fbm(q * 11.0 + 9.0 * curl + 3.0, octaves) - 0.5;
    var albedo = mix(planet.color_a.rgb, planet.color_b.rgb, clamp(belt + 0.9 * fine, 0.0, 1.0) * contrast);
    // Brightness varies as well as colour, so bands show even on a planet
    // whose two cloud colours are nearly the same.
    let relief = select(0.32, 0.30, hazy) * mix(0.6, 1.0, contrast);
    albedo = albedo * (1.0 + relief * (2.0 * (belt - 0.5) + 2.2 * fine));
    // A few large ovals.
    let storms = planet.shape.w;
    if storms > 0.0 && !hazy {
        let cell = fbm(vec3<f32>(p.x * 2.0, p.y * 6.0, p.z * 2.0) + 51.0 + 0.5 * curl, min(octaves, 3));
        let oval = smoothstep(0.80 - 0.08 * storms, 0.86, cell);
        albedo = mix(albedo, planet.color_c.rgb, oval * contrast);
    }
    // The poles darken a little, as on Jupiter and Saturn.
    albedo = albedo * (1.0 - 0.25 * smoothstep(0.75, 1.0, abs(latitude_sin)));
    // A hot giant shines by its own heat, and thin clouds let more of it
    // out: `emission` is the factor on that glow (`fragment`).
    let through = clamp(1.0 + 1.6 * (belt - 0.5) + 2.5 * fine, 0.1, 2.2);
    return Surface(albedo, vec3<f32>(through), 0.0, vec3<f32>(0.0));
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
        surface = rock(p, local, octaves);
    }

    // Clouds drift over the surface.
    let cloud_level = planet.cover.z;
    var cloud = 0.0;
    var cloud_tone = 1.0;
    if cloud_level < 1.5 && kind != 4 && kind != 5 {
        let cp = turned(local, planet.extra.x) + vec3<f32>(seed * 0.19 + 80.0);
        let cw = vec3<f32>(fbm(cp * 2.0 + 5.0, 3), fbm(cp * 2.0 + 15.0, 3), fbm(cp * 2.0 + 25.0, 3));
        let c = fbm(vec3<f32>(cp.x, cp.y * 1.6, cp.z) * 2.6 + 1.2 * cw, octaves);
        // Fine wisps along the edges, and thicker, greyer cores.
        let wisp = 0.09 * (fbm(cp * 15.0 + 4.0 * cw, min(octaves, 7)) - 0.5);
        cloud = smoothstep(cloud_level - 0.035, cloud_level + 0.035, c + wisp);
        cloud_tone = mix(1.0, 0.82, smoothstep(cloud_level + 0.04, cloud_level + 0.22, c));
    }
    let albedo = mix(surface.albedo, vec3<f32>(0.92) * cloud_tone, cloud);
    let gloss = surface.gloss * (1.0 - cloud);

    let wrap = 0.12 * density;
    // Relief catches the light; the sphere's own normal still sets the
    // terminator and the rim of the air below.
    let tilt = surface.tilt.x * planet.frame_x.xyz
        + surface.tilt.y * planet.frame_y.xyz
        + surface.tilt.z * planet.frame_z.xyz;
    var light = starlight(n, density, wrap);
    if dot(tilt, tilt) > 0.0 {
        // A wall facing the star can brighten only so far, and nothing
        // past the terminator lights up.
        light = min(starlight(normalize(n + tilt), density, wrap), light * 3.0);
    }
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
            let shine = pow(max(dot(n, half_way), 0.0), 320.0) * step(0.0, dot(n, l));
            color = color + planet.star_color[i].rgb * weight * shine * gloss * 1.4;
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
        emission = planet.glow.rgb * planet.glow.a * (0.25 + 0.5 * surface.albedo) * surface.emission;
    }
    color = color + emission * (1.0 - cloud);

    return vec4<f32>(color * exposure, 1.0);
}
