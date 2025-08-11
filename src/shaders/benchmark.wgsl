// src/shaders/benchmark.wgsl
// Benchmark shader for GPU performance testing

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
    
    // Perform benchmark computation (complex mathematical operations)
    var result = input_value;
    
    // Multiple mathematical operations to test GPU performance
    for (var i = 0u; i < 100u; i++) {
        result = sin(result) + cos(result) + sqrt(abs(result));
        result = result * 1.001 + 0.001;
    }
    
    // Write result to output buffer
    output.data[index] = result;
}
