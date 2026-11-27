pub(super) fn pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Relief map"),
        source: wgpu::ShaderSource::Wgsl(include_str!("relief.wgsl").into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Relief map"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vertex"),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: (6 * size_of::<f32>()) as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
            })],
            compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState::default(),
        // Eframe clears depth to 1 each frame. Egui's own meshes leave it untouched.
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth24Plus,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some(if format.remove_srgb_suffix() != format {
                "linear_color"
            } else {
                "display_color"
            }),
            targets: &[Some(format.into())],
            compilation_options: Default::default(),
        }),
        multiview_mask: None,
        cache: None,
    })
}
