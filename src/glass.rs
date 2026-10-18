//! The pane itself: a wgpu surface whose alpha reaches the desktop.
//!
//! The sill has to fade to fully see-through above the ledge, so the swapchain
//! must carry per-pixel alpha all the way to the compositor. A plain DXGI
//! swapchain made from an HWND can't: wgpu only offers it `Opaque`. What works
//! is the DirectComposition path, where wgpu makes a composition visual over the
//! window and presents into that with premultiplied alpha. The window is made
//! without a redirection bitmap so nothing opaque sits underneath the visual.
//! See `docs/transparency.md` for what each path did on real hardware.
//!
//! `WINDOWSILL_GLASS` picks a path, for comparing them: `visual` (the default),
//! `vulkan`, or `hwnd` (DX12 straight from the HWND, opaque).

use std::sync::Arc;

use anyhow::Context;
use winit::dpi::PhysicalSize;
use winit::window::Window;

/// How the swapchain reaches the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    /// DX12 into a DirectComposition visual. Per-pixel alpha.
    Visual,
    /// Vulkan, if its surface offers premultiplied alpha.
    Vulkan,
    /// DX12 straight from the HWND. Always opaque; kept for comparison.
    Hwnd,
}

impl Path {
    /// The path asked for in `WINDOWSILL_GLASS`, or the default.
    pub fn from_env() -> Self {
        match std::env::var("WINDOWSILL_GLASS")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "vulkan" => Self::Vulkan,
            "hwnd" => Self::Hwnd,
            _ => Self::Visual,
        }
    }

    /// Whether the window must be made without a redirection bitmap, which is
    /// what lets a composition visual show the desktop through it.
    pub fn wants_no_redirection_bitmap(self) -> bool {
        self == Self::Visual
    }

    fn instance(self) -> wgpu::Instance {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        match self {
            Self::Visual | Self::Hwnd => {
                descriptor.backends = wgpu::Backends::DX12;
                descriptor.backend_options.dx12.presentation_system = if self == Self::Visual {
                    wgpu::Dx12SwapchainKind::DxgiFromVisual
                } else {
                    wgpu::Dx12SwapchainKind::DxgiFromHwnd
                };
            }
            Self::Vulkan => descriptor.backends = wgpu::Backends::VULKAN,
        }
        wgpu::Instance::new(descriptor)
    }
}

pub struct Glass {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

/// What the shader needs to know about the strip; matches `Sill` in `glass.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    height: f32,
    scale: f32,
    presence: f32,
    _pad: f32,
}

impl Glass {
    pub fn new(window: Arc<Window>, path: Path) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let instance = path.instance();
        let surface = instance
            .create_surface(window)
            .context("could not make a surface for the sill")?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            // A strip this small never needs the discrete GPU on a laptop.
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .with_context(|| format!("no GPU adapter for the {path:?} path"))?;
        let info = adapter.get_info();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("windowsill"),
                ..Default::default()
            }))
            .context("could not open the GPU device")?;

        let caps = surface.get_capabilities(&adapter);
        tracing::info!(
            "glass: {path:?} on {} ({:?}, driver {}), alpha modes {:?}",
            info.name,
            info.backend,
            info.driver_info,
            caps.alpha_modes
        );
        let alpha_mode = if caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else {
            tracing::warn!("the {path:?} path has no premultiplied alpha; the glass will be opaque");
            caps.alpha_modes[0]
        };
        // A plain (non-sRGB) format, so the shader's premultiplied colour is
        // written as is and the compositor blends exactly what was drawn.
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::include_wgsl!("glass.wgsl"));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("glass"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sill"),
            size: size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sill"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            uniforms,
            bind_group,
        })
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        if (self.config.width, self.config.height) != (size.width, size.height) {
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    /// Draws one frame. `scale` is the window's DPI scale, and `presence` how
    /// much of the sill is there, from 0 to 1.
    pub fn draw(&mut self, scale: f64, presence: f32) {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            // Hidden or covered: there is nothing to show, so skip the frame.
            other => {
                tracing::trace!("skipping a frame: {other:?}");
                return;
            }
        };
        let uniforms = Uniforms {
            height: self.config.height as f32,
            scale: scale as f32,
            presence,
            _pad: 0.0,
        };
        self.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("glass") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("glass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
    }
}
