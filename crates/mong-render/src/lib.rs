//! mong-render — sprite batcher wgpu. Sàn bắt buộc WebGL2/GLES3 (mục 8 tài
//! liệu thiết kế): không compute shader, texture ≤ 4096, surface sRGB.
//!
//! Crate này không biết `Stage` là gì — nó nhận danh sách [`Sprite`] đã bố cục
//! sẵn. Bố cục là việc của `mong-runtime::draw_list`.

pub mod text;

#[cfg(feature = "png")]
mod image;

#[cfg(feature = "png")]
pub use image::decode_png;

use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;

/// Ràng buộc của sàn WebGL2. Vượt = từ chối nạp, không im lặng cắt xén.
pub const MAX_TEXTURE_DIM: u32 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureId(u32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fit {
    Cover,
    Anchor {
        x: f32,
        y: f32,
    },
    /// Toạ độ ảo tuyệt đối — glyph đã biết chính xác chỗ đứng của mình.
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
}

/// Một quad. Thứ tự trong slice = thứ tự vẽ (dưới lên trên).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sprite {
    pub texture: TextureId,
    pub fit: Fit,
    pub tint: [f32; 4],
    /// Vùng texture: `[u, v, du, dv]`. `FULL_UV` cho sprite thường.
    pub uv: [f32; 4],
    /// Texture là mask R8 (glyph) chứ không phải ảnh RGBA.
    pub mask: bool,
}

pub const FULL_UV: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

impl Sprite {
    pub fn image(texture: TextureId, fit: Fit, tint: [f32; 4]) -> Self {
        Sprite {
            texture,
            fit,
            tint,
            uv: FULL_UV,
            mask: false,
        }
    }
}

#[derive(Debug)]
pub enum RenderError {
    NoAdapter,
    TextureTooLarge { w: u32, h: u32 },
    Surface(wgpu::SurfaceError),
    Device(String),
    Decode(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::NoAdapter => write!(f, "khong tim thay GPU adapter phu hop"),
            RenderError::TextureTooLarge { w, h } => {
                write!(
                    f,
                    "texture {w}x{h} vuot gioi han {MAX_TEXTURE_DIM} cua WebGL2"
                )
            }
            RenderError::Surface(e) => write!(f, "loi surface: {e}"),
            RenderError::Device(m) => write!(f, "loi thiet bi: {m}"),
            RenderError::Decode(m) => write!(f, "khong giai ma duoc anh: {m}"),
        }
    }
}
impl std::error::Error for RenderError {}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    virtual_size: [f32; 2],
    _pad: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    rect: [f32; 4],
    tint: [f32; 4],
    uv: [f32; 4],
    mask: f32,
    _pad: [f32; 3],
}

struct Texture {
    bind_group: wgpu::BindGroup,
    size: (u32, u32),
}

/// Số quad tối đa mỗi frame. VN thực tế dùng < 20; 256 là dư thoải mái và
/// vẫn giữ instance buffer nhỏ (8 KB).
const MAX_INSTANCES: usize = 256;

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    globals_bg: wgpu::BindGroup,
    tex_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    instances: wgpu::Buffer,
    textures: HashMap<TextureId, Texture>,
    next_id: u32,
    virtual_size: (f32, f32),
}

impl Renderer {
    /// `surface_format` phải là biến thể sRGB — màu tint tính trong không gian
    /// tuyến tính, để phần cứng lo chuyển đổi.
    pub async fn new(
        adapter: &wgpu::Adapter,
        surface_format: wgpu::TextureFormat,
        virtual_size: (f32, f32),
    ) -> Result<Self, RenderError> {
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("mong-render"),
                    required_features: wgpu::Features::empty(),
                    // Sàn WebGL2: từ chối mọi thứ vượt downlevel defaults ngay
                    // trên desktop, để bug lộ ở M3 chứ không đợi tới M4.
                    required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                        .using_resolution(adapter.limits()),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .map_err(|e| RenderError::Device(e.to_string()))?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sprite"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sprite.wgsl").into()),
        });

        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite_texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let globals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(
            &globals_buf,
            0,
            bytemuck::bytes_of(&Globals {
                virtual_size: [virtual_size.0, virtual_size.1],
                _pad: [0.0; 2],
            }),
        );
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buf.as_entire_binding(),
            }],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sprite"),
            bind_group_layouts: &[&globals_layout, &tex_layout],
            push_constant_ranges: &[], // push constant không có trên WebGL2
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sprite"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Instance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sprite"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: (MAX_INSTANCES * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Renderer {
            device,
            queue,
            pipeline,
            globals_bg,
            tex_layout,
            sampler,
            instances,
            textures: HashMap::new(),
            next_id: 0,
            virtual_size,
        })
    }

    /// Đăng ký atlas glyph như một texture thường: draw call gộp chung,
    /// bind group đi chung đường.
    pub fn create_glyph_atlas(&mut self) -> text::GlyphAtlas {
        let tex = text::create_atlas_texture(&self.device);
        let id = self.register(&tex, (text::ATLAS_DIM, text::ATLAS_DIM));
        text::GlyphAtlas::new(tex, id)
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// `rgba` là RGBA8 chưa premultiply, hàng liền nhau. Giải mã PNG là việc
    /// của shell (giữ mong-render mỏng, web build nhỏ).
    pub fn upload(&mut self, rgba: &[u8], w: u32, h: u32) -> Result<TextureId, RenderError> {
        if w > MAX_TEXTURE_DIM || h > MAX_TEXTURE_DIM {
            return Err(RenderError::TextureTooLarge { w, h });
        }
        let size = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * w),
                rows_per_image: Some(h),
            },
            size,
        );

        Ok(self.register(&tex, (w, h)))
    }

    /// Cấp `TextureId` và bind group cho một texture đã tồn tại. Atlas glyph
    /// đi qua đây để được đối xử y hệt texture thường — cùng draw call, cùng
    /// đường bind. `upload` chỉ là `register` cộng phần tạo + nạp pixel.
    fn register(&mut self, tex: &wgpu::Texture, size: (u32, u32)) -> TextureId {
        let view = tex.create_view(&Default::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.tex_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let id = TextureId(self.next_id);
        self.next_id += 1;
        self.textures.insert(id, Texture { bind_group, size });
        id
    }

    /// Toạ độ ảo của một sprite. `Cover` giữ tỉ lệ, cắt phần thừa.
    fn rect(&self, fit: Fit, tex: (u32, u32)) -> [f32; 4] {
        let (vw, vh) = self.virtual_size;
        let (tw, th) = (tex.0 as f32, tex.1 as f32);
        match fit {
            Fit::Cover => {
                let s = (vw / tw).max(vh / th);
                let (w, h) = (tw * s, th * s);
                [(vw - w) * 0.5, (vh - h) * 0.5, w, h]
            }
            Fit::Anchor { x, y } => [x - tw * 0.5, y - th, tw, th],
            Fit::Rect { x, y, w, h } => [x, y, w, h],
        }
    }

    /// Vẽ một frame. Gộp các sprite liền kề cùng texture thành một draw call —
    /// đủ cho VN, không cần sort lại (sort sẽ phá thứ tự chồng lớp).
    pub fn draw(
        &mut self,
        view: &wgpu::TextureView,
        viewport: (f32, f32, f32, f32),
        sprites: &[Sprite],
        offset: (f32, f32),
    ) {
        let n = sprites.len().min(MAX_INSTANCES);
        if n < sprites.len() {
            eprintln!(
                "canh bao: {} sprite bi bo (gioi han {MAX_INSTANCES})",
                sprites.len() - n
            );
        }
        let (instances, tex_ids): (Vec<Instance>, Vec<TextureId>) = sprites[..n]
            .iter()
            .filter_map(|s| {
                let t = self.textures.get(&s.texture)?;
                let mut rect = self.rect(s.fit, t.size);
                rect[0] += offset.0;
                rect[1] += offset.1;
                Some((
                    Instance {
                        rect: self.rect(s.fit, t.size),
                        tint: s.tint,
                        uv: s.uv,
                        mask: if s.mask { 1.0 } else { 0.0 },
                        _pad: [0.0; 3],
                    },
                    s.texture,
                ))
            })
            .unzip();
        self.queue
            .write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));

        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sprite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // Ngoài khung ảo là nền đen — letterbox.
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_viewport(viewport.0, viewport.1, viewport.2, viewport.3, 0.0, 1.0);
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.globals_bg, &[]);
            pass.set_vertex_buffer(0, self.instances.slice(..));

            // Gộp chạy liên tiếp cùng texture.
            let mut i = 0usize;
            while i < tex_ids.len() {
                let tex = tex_ids[i];
                let mut j = i + 1;
                while j < tex_ids.len() && tex_ids[j] == tex {
                    j += 1;
                }
                pass.set_bind_group(1, &self.textures[&tex].bind_group, &[]);
                pass.draw(0..4, i as u32..j as u32);
                i = j;
            }
        }
        self.queue.submit([enc.finish()]);
    }
}

/// Khung ảo đặt giữa cửa sổ, giữ tỉ lệ. Trả `(x, y, w, h)` pixel thật.
pub fn letterbox(window: (u32, u32), virtual_size: (f32, f32)) -> (f32, f32, f32, f32) {
    let (ww, wh) = (window.0 as f32, window.1 as f32);
    let s = (ww / virtual_size.0).min(wh / virtual_size.1);
    let (w, h) = (virtual_size.0 * s, virtual_size.1 * s);
    ((ww - w) * 0.5, (wh - h) * 0.5, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letterbox_cua_so_rong_thi_cot_hai_ben() {
        let (x, y, w, h) = letterbox((2000, 1000), (1920.0, 1080.0));
        assert!((h - 1000.0).abs() < 1e-3, "cao khop, rong bi thu");
        assert!(x > 0.0 && y.abs() < 1e-3);
        assert!((w / h - 1920.0 / 1080.0).abs() < 1e-3, "giu ti le");
    }

    #[test]
    fn letterbox_dung_ti_le_thi_phu_kin() {
        let (x, y, w, h) = letterbox((1280, 720), (1920.0, 1080.0));
        assert_eq!((x, y), (0.0, 0.0));
        assert_eq!((w, h), (1280.0, 720.0));
    }
}
