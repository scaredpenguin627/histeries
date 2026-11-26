struct SurfaceVertex {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
}

@vertex
fn vertex(@location(0) clip: vec3<f32>, @location(1) color: vec3<f32>) -> SurfaceVertex {
    return SurfaceVertex(vec4<f32>(clip, 1.0), color);
}

@fragment
fn display_color(input: SurfaceVertex) -> @location(0) vec4<f32> {
    return vec4<f32>(input.color, 1.0);
}

// The native surface can perform linear-to-sRGB conversion on write.
@fragment
fn linear_color(input: SurfaceVertex) -> @location(0) vec4<f32> {
    let c = input.color;
    let nonlinear = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    let linear = select(nonlinear, c / 12.92, c <= vec3<f32>(0.04045));
    return vec4<f32>(linear, 1.0);
}
