//! Depth-tested terrain and marker triangles inside the map painter's viewport.

use eframe::{egui, egui_wgpu};

use super::{Vertex, pipeline::pipeline};

const VERTEX_BYTES: usize = 6 * size_of::<f32>();
// Keep individual uploads small and split only between complete triangles.
const BATCH_VERTICES: usize = 3 * 16_384;

struct Resources {
    pipeline: wgpu::RenderPipeline,
    buffers: Vec<wgpu::Buffer>,
    upload: Vec<u8>,
}

/// Installs one renderer per device. Headless egui contexts still support picking.
pub(crate) fn install(cc: &eframe::CreationContext<'_>) {
    let Some(state) = &cc.wgpu_render_state else {
        return;
    };
    state
        .renderer
        .write()
        .callback_resources
        .entry::<Resources>()
        .or_insert_with(|| Resources {
            pipeline: pipeline(&state.device, state.target_format),
            buffers: Vec::new(),
            upload: Vec::new(),
        });
}

/// Queues the map's triangle list in the painter's current layer and clip rect.
pub(in crate::presenter) fn paint(
    painter: &egui::Painter,
    rect: egui::Rect,
    vertices: Vec<Vertex>,
) {
    painter.add(egui_wgpu::Callback::new_paint_callback(
        rect,
        MapDraw(vertices),
    ));
}

struct MapDraw(Vec<Vertex>);

impl egui_wgpu::CallbackTrait for MapDraw {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(map) = resources.get_mut::<Resources>() {
            let batches = self.0.chunks(BATCH_VERTICES);
            map.buffers.resize_with(batches.len(), || {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Relief map vertices"),
                    size: (BATCH_VERTICES * VERTEX_BYTES) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            });
            for (batch, buffer) in batches.zip(&map.buffers) {
                map.upload.clear();
                map.upload.reserve(batch.len() * VERTEX_BYTES);
                for vertex in batch {
                    for component in vertex.clip.into_iter().chain(vertex.color) {
                        map.upload.extend_from_slice(&component.to_le_bytes());
                    }
                }
                queue.write_buffer(buffer, 0, &map.upload);
            }
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(map) = resources.get::<Resources>() {
            pass.set_pipeline(&map.pipeline);
            for (batch, buffer) in self.0.chunks(BATCH_VERTICES).zip(&map.buffers) {
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..batch.len() as u32, 0..1);
            }
        }
    }
}
