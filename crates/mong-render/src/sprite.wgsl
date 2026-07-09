// Sprite batcher — instancing bằng vertex buffer (WebGL2 không có storage buffer).
// Không compute shader, không texture > 4096, surface sRGB. Sàn = WebGL2/GLES3.

struct Globals {
    // Kích thước khung ảo; đổi toạ độ ảo → clip space.
    virtual_size: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var t_sprite: texture_2d<f32>;
@group(1) @binding(1) var s_sprite: sampler;

struct Instance {
    @location(0) rect: vec4<f32>,   // x, y, w, h (toạ độ ảo, gốc trái-trên)
    @location(1) tint: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Instance) -> VsOut {
    // Quad đơn vị dựng từ vertex_index — không cần vertex buffer riêng.
    let corner = vec2<f32>(f32(vi & 1u), f32((vi >> 1u) & 1u));
    let pos = inst.rect.xy + corner * inst.rect.zw;
    let ndc = vec2<f32>(
        pos.x / globals.virtual_size.x * 2.0 - 1.0,
        1.0 - pos.y / globals.virtual_size.y * 2.0,
    );
    var out: VsOut;
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = corner;
    out.tint = inst.tint;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(t_sprite, s_sprite, in.uv);
    // Alpha premultiplied ở blend state, nên nhân tint trực tiếp là đúng.
    return c * in.tint;
}
