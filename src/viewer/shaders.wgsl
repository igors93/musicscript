// Shaders of the 3D view.
//
// Colors arrive in linear space; the render target is sRGB, so the hardware
// encodes on write. Edges and points are camera-facing quads expanded in
// screen space, which gives constant pixel widths at any zoom.
//
// A section plane removes everything in front of it (dot(n, p) > d) from
// the objects that are clipped. The cut faces are filled ("capped") with
// the stencil buffer: each clipped object is drawn once more into the
// stencil only, inverting it per surface, so the pixels where the plane
// lies inside the solid end with an odd count; a quad on the plane is then
// drawn where the stencil is set, which also clears it.
//
// In the hidden-line display mode faces are filled with the background, so
// they only hide what is behind them; the hidden edges are drawn again as
// faint dashes where the depth test fails, and section caps are hatched.

struct Camera {
    view_proj: mat4x4<f32>,
    // xyz: eye position; w: 1 for perspective projection, 0 for orthographic.
    eye: vec4<f32>,
    forward: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    // Width and height in pixels, then their reciprocals.
    viewport: vec4<f32>,
    background_top: vec4<f32>,
    background_bottom: vec4<f32>,
    // xyz: normal of the section plane; w: its offset d.
    clip_plane: vec4<f32>,
    // x: 1 while a section plane is active.
    section: vec4<f32>,
    // Corners of the quad covering the section plane around the scene.
    cap_corners: array<vec4<f32>, 4>,
    // xy: offset of this pass in the whole image (large captures are drawn
    // in tiles); zw: size of the whole image, in pixels.
    frame: vec4<f32>,
};

struct Object {
    model: mat4x4<f32>,
    face_color: vec4<f32>,
    edge_color: vec4<f32>,
    // rgb: highlight color; a: how strongly faces are tinted with it.
    highlight: vec4<f32>,
    // x: edge width (px); y: point diameter (px); z: 1 when the section
    // plane clips this object; w: 1 to fill faces with the background
    // (hidden-line mode).
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var<uniform> object: Object;

// Whether the section plane removes this point of the current object.
fn is_cut_away(world: vec3<f32>) -> bool {
    return camera.section.x > 0.5 && object.params.z > 0.5
        && dot(camera.clip_plane.xyz, world) > camera.clip_plane.w;
}

// The background behind a fragment at framebuffer row `y`, alpha included
// (0 for captures with a transparent background).
fn background_at(y: f32) -> vec4<f32> {
    let height = clamp(1.0 - (y + camera.frame.y) / camera.frame.w, 0.0, 1.0);
    return mix(camera.background_bottom, camera.background_top, height);
}

fn fills_with_background() -> bool {
    return object.params.w > 0.5;
}

// Normal facing the viewer, for two-sided lighting.
fn facing_normal(normal: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    var to_eye = -camera.forward.xyz;
    if (camera.eye.w > 0.5) {
        to_eye = normalize(camera.eye.xyz - world);
    }
    var n = normal;
    let length_n = length(n);
    if (length_n < 1e-6) {
        n = to_eye;
    } else {
        n = n / length_n;
    }
    // Light both sides: open shells and inverted faces look the same as
    // closed solids.
    if (dot(n, to_eye) < 0.0) {
        n = -n;
    }
    return n;
}

// Three-point lighting attached to the camera: key light from the upper
// left, a dimmer fill from the lower right and a little rim light.
fn lit(base: vec3<f32>, n: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    var to_eye = -camera.forward.xyz;
    if (camera.eye.w > 0.5) {
        to_eye = normalize(camera.eye.xyz - world);
    }
    let back = -camera.forward.xyz;
    let key = normalize(-0.35 * camera.right.xyz + 0.55 * camera.up.xyz + 0.76 * back);
    let fill = normalize(0.65 * camera.right.xyz - 0.25 * camera.up.xyz + 0.72 * back);
    let diffuse = 0.62 * max(dot(n, key), 0.0) + 0.24 * max(dot(n, fill), 0.0);
    let rim = 0.10 * pow(1.0 - max(dot(n, to_eye), 0.0), 3.0);
    let ambient = 0.24;
    let half_vector = normalize(key + to_eye);
    let specular = 0.22 * pow(max(dot(n, half_vector), 0.0), 48.0);
    return base * (ambient + diffuse) + vec3<f32>(specular + rim);
}

// --- Background ---------------------------------------------------------

struct BackgroundOut {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vs_background(@builtin(vertex_index) index: u32) -> BackgroundOut {
    // One triangle covering the whole viewport.
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: BackgroundOut;
    out.position = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    return out;
}

@fragment
fn fs_background(in: BackgroundOut) -> @location(0) vec4<f32> {
    return background_at(in.position.y);
}

// --- Faces --------------------------------------------------------------

struct MeshIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct MeshOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

@vertex
fn vs_mesh(in: MeshIn) -> MeshOut {
    var out: MeshOut;
    let world = object.model * vec4<f32>(in.position, 1.0);
    out.clip = camera.view_proj * world;
    out.world = world.xyz;
    let rotation = mat3x3<f32>(object.model[0].xyz, object.model[1].xyz, object.model[2].xyz);
    out.normal = rotation * in.normal;
    return out;
}

@fragment
fn fs_mesh(in: MeshOut) -> @location(0) vec4<f32> {
    if (is_cut_away(in.world)) {
        discard;
    }
    if (fills_with_background()) {
        let background = background_at(in.clip.y);
        let tint = object.highlight.a * 0.5;
        return vec4<f32>(
            mix(background.rgb, object.highlight.rgb, tint),
            mix(background.a, 1.0, tint),
        );
    }
    let n = facing_normal(in.normal, in.world);
    let base = mix(object.face_color.rgb, object.highlight.rgb, object.highlight.a);
    return vec4<f32>(lit(base, n, in.world), object.face_color.a);
}

// Counts the object's remaining surfaces into the stencil; no color.
@fragment
fn fs_section_count(in: MeshOut) {
    if (is_cut_away(in.world)) {
        discard;
    }
}

// --- Section caps -------------------------------------------------------

struct CapOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
};

@vertex
fn vs_cap(@builtin(vertex_index) index: u32) -> CapOut {
    var corner_of = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
    let world = camera.cap_corners[corner_of[index]].xyz;
    var out: CapOut;
    out.clip = camera.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    return out;
}

@fragment
fn fs_cap(in: CapOut) -> @location(0) vec4<f32> {
    if (fills_with_background()) {
        // Hatched, as a section on a drawing, as wide apart as the edges
        // are wide.
        let spacing = max(object.params.x, 1.0) * 5.0;
        let position = in.clip.xy + camera.frame.xy;
        if (fract((position.x + position.y) / spacing) < 0.2) {
            return vec4<f32>(object.edge_color.rgb, 1.0);
        }
        return background_at(in.clip.y);
    }
    // A little darker than the outside, so the cut reads as a cut.
    let n = facing_normal(camera.clip_plane.xyz, in.world);
    let base = mix(object.face_color.rgb, object.highlight.rgb, object.highlight.a) * 0.78;
    return vec4<f32>(lit(base, n, in.world), object.face_color.a);
}

// --- Edges --------------------------------------------------------------

struct SegmentIn {
    @location(0) start: vec3<f32>,
    @location(1) end: vec3<f32>,
};

// Pulls a clip-space point in front of the near plane, along the segment.
fn clip_to_near(point: vec4<f32>, other: vec4<f32>) -> vec4<f32> {
    let min_w = 1e-5;
    if (point.w >= min_w || camera.eye.w < 0.5) {
        return point;
    }
    let t = (min_w - point.w) / (other.w - point.w);
    return mix(point, other, t);
}

struct EdgeOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    // Pixels from the start of the segment, for dashes.
    @location(1) @interpolate(linear) along_px: f32,
};

@vertex
fn vs_edge(@builtin(vertex_index) index: u32, in: SegmentIn) -> EdgeOut {
    let start_world = object.model * vec4<f32>(in.start, 1.0);
    let end_world = object.model * vec4<f32>(in.end, 1.0);
    let start_raw = camera.view_proj * start_world;
    let end_raw = camera.view_proj * end_world;
    let start = clip_to_near(start_raw, end_raw);
    let end = clip_to_near(end_raw, start_raw);

    let half_size = camera.viewport.xy * 0.5;
    let start_px = start.xy / start.w * half_size;
    let end_px = end.xy / end.w * half_size;
    var along = end_px - start_px;
    let length_px = length(along);
    if (length_px > 1e-4) {
        along = along / length_px;
    } else {
        along = vec2<f32>(1.0, 0.0);
    }
    let across = vec2<f32>(-along.y, along.x);

    // Two triangles: (start,-), (end,-), (end,+), (start,-), (end,+), (start,+).
    var at_end = array<f32, 6>(0.0, 1.0, 1.0, 0.0, 1.0, 0.0);
    var side = array<f32, 6>(-1.0, -1.0, 1.0, -1.0, 1.0, 1.0);
    let t = at_end[index];
    var clip = mix(start, end, t);
    let half_width = object.params.x * 0.5;
    // Extending past the endpoints by half a width gives square caps that
    // close the gaps between consecutive segments.
    let offset_px = across * side[index] * half_width + along * (t * 2.0 - 1.0) * half_width;
    clip = vec4<f32>(clip.xy + offset_px * camera.viewport.zw * 2.0 * clip.w, clip.zw);
    var out: EdgeOut;
    out.clip = clip;
    out.world = mix(start_world.xyz, end_world.xyz, t);
    out.along_px = t * length_px + (t * 2.0 - 1.0) * half_width;
    return out;
}

@fragment
fn fs_edge(in: EdgeOut) -> @location(0) vec4<f32> {
    if (is_cut_away(in.world)) {
        discard;
    }
    return object.edge_color;
}

// Edges behind faces, in hidden-line mode: faint dashes.
@fragment
fn fs_hidden_edge(in: EdgeOut) -> @location(0) vec4<f32> {
    let period = max(object.params.x, 1.0) * 6.5;
    if (is_cut_away(in.world) || fract(in.along_px / period) > 0.55) {
        discard;
    }
    return vec4<f32>(object.edge_color.rgb, object.edge_color.a * 0.45);
}

// --- Points -------------------------------------------------------------

struct PointOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) world: vec3<f32>,
};

@vertex
fn vs_point(@builtin(vertex_index) index: u32, @location(0) position: vec3<f32>) -> PointOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let corner = corners[index];
    let world = object.model * vec4<f32>(position, 1.0);
    var clip = camera.view_proj * world;
    let radius_px = object.params.y * 0.5;
    clip = vec4<f32>(clip.xy + corner * radius_px * camera.viewport.zw * 2.0 * clip.w, clip.zw);
    var out: PointOut;
    out.clip = clip;
    out.local = corner;
    out.world = world.xyz;
    return out;
}

@fragment
fn fs_point(in: PointOut) -> @location(0) vec4<f32> {
    if (dot(in.local, in.local) > 1.0 || is_cut_away(in.world)) {
        discard;
    }
    return object.edge_color;
}
