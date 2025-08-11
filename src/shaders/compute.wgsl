// src/shaders/compute.wgsl
// Compute shader for parallel data processing

struct InputBuffer {
    data: array<f32>,
}

struct OutputBuffer {
    data: array<f32>,
}

@group(0) @binding(0) var<storage, read> input: InputBuffer;
@group(0) @binding(1) var<storage, read_write> output: OutputBuffer;

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let index = global_id.x;
    
    // Bounds check
    if (index >= arrayLength(&input.data)) {
        return;
    }
    
    // Read input data
    let input_value = input.data[index];
    
    // Apply computation (example: square the value)
    let computed_value = input_value * input_value;
    
    // Write result to output buffer
    output.data[index] = computed_value;
}
