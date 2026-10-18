//! The pane itself: a wgpu surface whose alpha reaches the desktop.
//!
//! The sill is see-through everywhere but the things on it, so the swapchain
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

use anyhow::{Context, anyhow};
use winit::dpi::PhysicalSize;
use winit::window::Window;

use crate::carry::Pose;
use crate::palette::Look;
use crate::sill::SillState;

/// The shader the sill was built with, for when its source isn't on disk.
pub const SHADER: &str = include_str!("glass.wgsl");

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
    config: wgpu::SurfaceConfiguration,
    painter: Painter,
}

/// Everything that draws the sill, apart from where it is drawn to, so the
/// bench can draw exactly what the window does into a texture of its own.
pub struct Painter {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    layout: wgpu::PipelineLayout,
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
    pad: f32,
    /// The jar's lid, and its lit top edge.
    wood: [f32; 4],
    lip: [f32; 4],
    /// Beam angle, colour temperature, shadow side and moonlight.
    light: [f32; 4],
    /// How much sun the window lets in, which side its light comes from, and
    /// which way the moon's shadows fall.
    room: [f32; 4],
    /// The colour of the room's light, and in `.w` how vivid things are in it.
    sky: [f32; 4],
    /// The candle's flame, how much of it is left, and its flicker.
    candle: [f32; 4],
    /// How stirred the dust is, how far through its cycle it has drifted, how
    /// far the plant leans, and how bright the jar's ring is.
    music: [f32; 4],
    /// Where the plant, jar and candle stand, in physical pixels across, and
    /// which of them is in front.
    places: [f32; 4],
    /// How high each is lifted, in logical pixels.
    lift: [f32; 4],
    /// How far each tips, in degrees to the right.
    tip: [f32; 4],
    /// How far what is inside each trails, in degrees to the right.
    swing: [f32; 4],
    pot: [f32; 4],
    leaf: [f32; 4],
    glass: [f32; 4],
    water: [f32; 4],
    wax: [f32; 4],
    flame: [f32; 4],
}

impl Glass {
    /// Opens the glass over `window`, drawn with the WGSL in `shader`.
    pub fn new(window: Arc<Window>, path: Path, shader: &str) -> anyhow::Result<Self> {
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
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
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
            tracing::warn!(
                "the {path:?} path has no premultiplied alpha; the glass will be opaque"
            );
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

        let painter = Painter::new(device, queue, format, shader)?;
        Ok(Self {
            surface,
            config,
            painter,
        })
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        if (self.config.width, self.config.height) != (size.width, size.height) {
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.painter.device, &self.config);
        }
    }

    /// Swaps in a new shader. If it doesn't build, the old one stays and the
    /// error says why.
    pub fn reload(&mut self, shader: &str) -> anyhow::Result<()> {
        self.painter.reload(shader)
    }

    /// Draws one frame. `scale` is the window's DPI scale, `presence` how much
    /// of the sill is there, from 0 to 1, `look` its colours and where things
    /// stand on it, `sill` the light it is in, `flicker` how the candle's
    /// flame breathes, and `music` how the sill answers the music: how far the
    /// dust has come through its cycle, how many degrees the plant's leaves
    /// lean, and how bright the jar's ring is. `pose` is how the things stand,
    /// and how a hand is carrying them.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        scale: f64,
        presence: f32,
        look: &Look,
        sill: &SillState,
        flicker: [f32; 2],
        music: [f32; 3],
        pose: &Pose,
    ) {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.painter.device, &self.config);
                return;
            }
            // Hidden or covered: there is nothing to show, so skip the frame.
            other => {
                tracing::trace!("skipping a frame: {other:?}");
                return;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let size = [self.config.width, self.config.height];
        self.painter.paint(
            &view, size, scale, presence, look, sill, flicker, music, pose,
        );
        self.painter.queue.present(frame);
    }
}

impl Painter {
    /// Builds the pipeline for drawing into `format` with the WGSL in `shader`.
    fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        format: wgpu::TextureFormat,
        shader: &str,
    ) -> anyhow::Result<Self> {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sill"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        // Made by hand rather than taken from the pipeline, so a reloaded shader
        // can reuse the same bind group.
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("glass"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = build_pipeline(&device, &layout, format, shader)
            .context("the sill's shader would not build")?;

        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sill"),
            size: size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sill"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });

        Ok(Self {
            device,
            queue,
            format,
            layout,
            pipeline,
            uniforms,
            bind_group,
        })
    }

    /// A painter with no window, for the bench: on the GPU the sill would
    /// choose, or on Windows' software renderer where there is none, as on CI.
    pub fn headless(
        path: Path,
        format: wgpu::TextureFormat,
        shader: &str,
    ) -> anyhow::Result<(Self, wgpu::AdapterInfo)> {
        let instance = path.instance();
        let options = |fallback| wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: fallback,
            apply_limit_buckets: false,
        };
        let adapter = pollster::block_on(instance.request_adapter(&options(false)))
            .or_else(|_| pollster::block_on(instance.request_adapter(&options(true))))
            .with_context(|| format!("no GPU adapter for the {path:?} path"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("windowsill bench"),
            ..Default::default()
        }))
        .context("could not open the GPU device")?;
        let painter = Self::new(device, queue, format, shader)?;
        Ok((painter, adapter.get_info()))
    }

    /// Waits for the GPU to finish everything submitted so far.
    pub fn wait(&self) {
        if let Err(err) = self.device.poll(wgpu::PollType::wait_indefinitely()) {
            tracing::warn!("could not wait for the GPU: {err}");
        }
    }

    /// Swaps in a new shader, keeping the old one if it doesn't build.
    pub fn reload(&mut self, shader: &str) -> anyhow::Result<()> {
        self.pipeline = build_pipeline(&self.device, &self.layout, self.format, shader)?;
        Ok(())
    }

    /// Draws one frame into `view`, which is `size` physical pixels, and
    /// submits it. The arguments are [`Glass::draw`]'s.
    #[allow(clippy::too_many_arguments)]
    pub fn paint(
        &self,
        view: &wgpu::TextureView,
        size: [u32; 2],
        scale: f64,
        presence: f32,
        look: &Look,
        sill: &SillState,
        flicker: [f32; 2],
        music: [f32; 3],
        pose: &Pose,
    ) {
        let palette = &look.palette;
        let scale = scale as f32;
        let [plant, jar, candle] = pose.x.map(|x| x * scale);
        let four = |[a, b, c]: [f32; 3]| [a, b, c, 0.0];
        let uniforms = Uniforms {
            height: size[1] as f32,
            scale,
            presence,
            pad: 0.0,
            wood: palette.wood.padded(),
            lip: palette.lip.padded(),
            light: [
                sill.beam_angle,
                sill.colour_temperature,
                sill.shadow_side,
                sill.moonlight,
            ],
            room: [sill.sunshine, sill.window_side, sill.moon_side, 0.0],
            sky: [
                sill.ambient[0],
                sill.ambient[1],
                sill.ambient[2],
                sill.vivid,
            ],
            candle: [sill.candle, sill.wax, flicker[0], flicker[1]],
            music: [sill.dust, music[0], music[1], music[2]],
            places: [plant, jar, candle, pose.front as f32],
            lift: four(pose.lift),
            tip: four(pose.tip),
            swing: four(pose.swing),
            pot: look.plant.pot.padded(),
            leaf: look.plant.leaf.padded(),
            glass: look.jar.glass.padded(),
            water: look.jar.water.padded(),
            wax: look.candle.wax.padded(),
            flame: look.candle.flame.padded(),
        };
        self.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("glass"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("glass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
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
    }
}

/// Builds the glass pipeline from WGSL. Mistakes in the shader come back as an
/// error rather than wgpu's default of panicking.
fn build_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    shader: &str,
) -> anyhow::Result<wgpu::RenderPipeline> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("glass"),
        source: wgpu::ShaderSource::Wgsl(shader.into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("glass"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
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
    match pollster::block_on(scope.pop()) {
        None => Ok(pipeline),
        Some(err) => Err(anyhow!("{err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module() -> naga::Module {
        naga::front::wgsl::parse_str(SHADER)
            .unwrap_or_else(|err| panic!("{}", err.emit_to_string(SHADER)))
    }

    #[test]
    fn the_shader_is_valid() {
        let module = module();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap_or_else(|err| panic!("{}", err.emit_to_string(SHADER)));
    }

    #[test]
    fn the_uniforms_match_the_shader() {
        let module = module();
        let span = module
            .types
            .iter()
            .find_map(|(_, ty)| match &ty.inner {
                naga::TypeInner::Struct { span, .. } if ty.name.as_deref() == Some("Sill") => {
                    Some(*span)
                }
                _ => None,
            })
            .expect("the shader has no Sill");
        assert_eq!(span as usize, size_of::<Uniforms>());
    }
}
