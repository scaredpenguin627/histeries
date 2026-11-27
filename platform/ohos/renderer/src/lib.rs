//! Native XComponent renderer. All extern calls are serialized by the component's lifecycle lock.

use histeries_client::{ClientSession, surface_vertices};
use raw_window_handle::{OhosDisplayHandle, OhosNdkWindowHandle};
use std::{
    ffi::{CString, c_char, c_int, c_void},
    ptr::NonNull,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use wgpu::util::DeviceExt;

#[path = "../../../../crates/client/src/presenter/scene3d/pipeline.rs"]
mod pipeline;

#[link(name = "hilog_ndk.z")]
unsafe extern "C" {
    fn OH_LOG_Print(
        kind: c_int,
        level: c_int,
        domain: u32,
        tag: *const c_char,
        fmt: *const c_char,
        ...
    ) -> c_int;
}

fn log(level: c_int, message: &str) {
    let text = CString::new(message.replace('\0', " ")).unwrap();
    // HiLog consumes both NUL-terminated strings during this call.
    unsafe {
        OH_LOG_Print(
            0,
            level,
            0x0000,
            c"Histeries".as_ptr(),
            c"%{public}s".as_ptr(),
            text.as_ptr(),
        );
    }
}

struct Renderer {
    surface: Option<wgpu::Surface<'static>>,
    instance: wgpu::Instance,
    window: NonNull<c_void>,
    device: wgpu::Device,
    adapter: wgpu::Adapter,
    failed: Arc<AtomicBool>,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    depth: Option<wgpu::TextureView>,
}

/// The host retains its OHNativeWindow until the renderer has been dropped.
unsafe fn surface(
    instance: &wgpu::Instance,
    window: NonNull<c_void>,
) -> Result<wgpu::Surface<'static>, String> {
    let target = wgpu::SurfaceTargetUnsafe::RawHandle {
        raw_display_handle: Some(OhosDisplayHandle::new().into()),
        raw_window_handle: OhosNdkWindowHandle::new(window).into(),
    };
    // The C++ host holds a native window reference and serializes teardown against drawing.
    unsafe { instance.create_surface_unsafe(target) }.map_err(|error| error.to_string())
}

impl Renderer {
    unsafe fn new(window: NonNull<c_void>) -> Result<Self, String> {
        let mut options = wgpu::InstanceDescriptor::new_without_display_handle();
        options.backends = wgpu::Backends::VULKAN;
        let instance = wgpu::Instance::new(options);
        let surface = unsafe { surface(&instance, window) }?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .map_err(|error| error.to_string())?;
        let info = adapter.get_info();
        if info.backend != wgpu::Backend::Vulkan {
            return Err(format!("Unexpected graphics backend: {:?}", info.backend));
        }
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .map_err(|error| error.to_string())?;
        let failed = Arc::new(AtomicBool::new(false));
        let errors = Arc::clone(&failed);
        device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
            errors.store(true, Ordering::Release);
            log(6, &error.to_string());
        }));
        let lost = Arc::clone(&failed);
        device.set_device_lost_callback(move |_, reason| {
            lost.store(true, Ordering::Release);
            log(6, &reason);
        });
        let config = surface
            .get_default_config(&adapter, 1, 1)
            .ok_or("No presentable surface format")?;
        let pipeline = pipeline::pipeline(&device, config.format);
        log(4, &format!("Vulkan adapter: {}", info.name));
        Ok(Self {
            surface: Some(surface),
            instance,
            window,
            device,
            adapter,
            failed,
            queue,
            config,
            pipeline,
            depth: None,
        })
    }

    fn draw(&mut self, game: &ClientSession, width: u32, height: u32) -> Result<i32, String> {
        if self.failed.load(Ordering::Acquire) {
            return Ok(2); // Ask the owner of the native window to rebuild this renderer.
        }
        if width == 0 || height == 0 {
            return Ok(1);
        }
        if width.max(height) > self.device.limits().max_texture_dimension_2d {
            return Err("Surface exceeds the Vulkan device's texture limit".into());
        }
        if self.surface.is_none() {
            // The host still owns the window. The previous Vulkan surface was dropped on loss.
            self.surface = Some(unsafe { surface(&self.instance, self.window) }?);
        }
        let surface = self.surface.as_ref().unwrap();
        if self.depth.is_none() || (width, height) != (self.config.width, self.config.height) {
            let Some(config) = surface.get_default_config(&self.adapter, width, height) else {
                // Surface loss may first be reported as empty capabilities during resize.
                return Ok(2);
            };
            let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
            surface.configure(&self.device, &config);
            if let Some(error) = pollster::block_on(scope.pop()) {
                log(5, &error.to_string());
                self.depth = None;
                self.surface.take();
                return Ok(1);
            }
            if self.config.format != config.format {
                self.pipeline = pipeline::pipeline(&self.device, config.format);
            }
            self.config = config;
            let depth = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Globe depth"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth24Plus,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.depth = Some(depth.create_view(&Default::default()));
        }
        use wgpu::CurrentSurfaceTexture as Acquired;
        let (frame, reconfigure) = match surface.get_current_texture() {
            Acquired::Success(frame) => (frame, false),
            Acquired::Suboptimal(frame) => (frame, true),
            Acquired::Timeout | Acquired::Occluded => return Ok(1),
            Acquired::Outdated => {
                self.depth = None;
                return Ok(1);
            }
            Acquired::Lost => {
                self.surface.take();
                self.depth = None;
                return Ok(1);
            }
            Acquired::Validation => {
                return Err("Vulkan surface acquisition failed validation".into());
            }
        };
        let vertices = surface_vertices(game.observation(), width, height);
        if vertices.is_empty() {
            return Err("Game scene is empty".into());
        }
        let bytes: Vec<u8> = vertices
            .iter()
            .flatten()
            .flat_map(|component| component.to_le_bytes())
            .collect();
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Observed map mesh"),
                contents: &bytes,
                usage: wgpu::BufferUsages::VERTEX,
            });
        let color = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Observed globe"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: self.depth.as_ref().unwrap(),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        if self.failed.load(Ordering::Acquire) {
            return Ok(2);
        }
        self.queue.present(frame);
        if reconfigure {
            self.depth = None;
        }
        Ok(0)
    }
}

// Native callbacks cannot unwind into ArkUI. Unexpected Rust panics fail the current operation.
fn ffi<T>(fallback: T, operation: impl FnOnce() -> T) -> T {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation)).unwrap_or_else(|_| {
        log(6, "Rust panicked in the native game client");
        fallback
    })
}

// The following pointers are opaque to ArkTS. board.cpp pairs every successful allocation
// with its destructor and holds one mutex for game commands, drawing and native window changes.
#[unsafe(no_mangle)]
extern "C" fn histeries_ohos_game_new() -> *mut ClientSession {
    ffi(std::ptr::null_mut(), || match ClientSession::globe(7, 3) {
        Ok(game) => Box::into_raw(Box::new(game)),
        Err(e) => {
            log(6, &e);
            std::ptr::null_mut()
        }
    })
}
#[unsafe(no_mangle)]
unsafe extern "C" fn histeries_ohos_game_destroy(game: *mut ClientSession) {
    ffi((), || {
        if !game.is_null() {
            drop(unsafe { Box::from_raw(game) });
        }
    })
}
#[unsafe(no_mangle)]
unsafe extern "C" fn histeries_ohos_create(window: *mut c_void) -> *mut Renderer {
    ffi(std::ptr::null_mut(), || {
        let Some(window) = NonNull::new(window) else {
            return std::ptr::null_mut();
        };
        match unsafe { Renderer::new(window) } {
            Ok(renderer) => Box::into_raw(Box::new(renderer)),
            Err(e) => {
                log(6, &e);
                std::ptr::null_mut()
            }
        }
    })
}
#[unsafe(no_mangle)]
unsafe extern "C" fn histeries_ohos_destroy(renderer: *mut Renderer) {
    ffi((), || {
        if !renderer.is_null() {
            drop(unsafe { Box::from_raw(renderer) });
        }
    })
}
#[unsafe(no_mangle)]
unsafe extern "C" fn histeries_ohos_end_turn(game: *mut ClientSession) -> i32 {
    ffi(-1, || {
        match unsafe { game.as_mut() }.map(ClientSession::end_turn) {
            Some(Ok(())) => 0,
            Some(Err(e)) => {
                log(6, &e);
                -1
            }
            None => -1,
        }
    })
}
#[unsafe(no_mangle)]
unsafe extern "C" fn histeries_ohos_present(
    renderer: *mut Renderer,
    game: *const ClientSession,
    width: u32,
    height: u32,
) -> i32 {
    ffi(-1, || {
        let (Some(renderer), Some(game)) = (unsafe { renderer.as_mut() }, unsafe { game.as_ref() })
        else {
            return -1;
        };
        match renderer.draw(game, width, height) {
            Ok(status) => status,
            Err(e) => {
                log(6, &e);
                -1
            }
        }
    })
}
