// OScope's image kernels for CUDA: the twins of the CPU kernels in
// src/cpu.rs, which are their reference (ADR-0010). build.rs compiles this
// file to a fat binary (ADR-0011).
//
// Each kernel adds in the same order as its twin, and CUDA's division is
// rounded as IEEE 754 requires unless fast maths is asked for, which it isn't.
// So the twins agree to the bit.
//
// The launch: blockIdx.x and threadIdx.x pick a sample along an output row;
// blockIdx.y picks the first row, and each block then steps down by gridDim.y.

// 16-bit samples as floats from 0 to 1. `stride` is in samples.
extern "C" __global__ void to_f32_u16(const unsigned short* in, unsigned long long stride,
                                      unsigned int row_len, unsigned int height, float* out) {
    unsigned int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i >= row_len) return;
    for (unsigned int y = blockIdx.y; y < height; y += gridDim.y) {
        out[(unsigned long long)y * row_len + i] = (float)in[y * stride + i] / 65535.0f;
    }
}

// 8-bit samples as floats from 0 to 1. `stride` is in samples.
extern "C" __global__ void to_f32_u8(const unsigned char* in, unsigned long long stride,
                                     unsigned int row_len, unsigned int height, float* out) {
    unsigned int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i >= row_len) return;
    for (unsigned int y = blockIdx.y; y < height; y += gridDim.y) {
        out[(unsigned long long)y * row_len + i] = (float)in[y * stride + i] / 255.0f;
    }
}

// 2 x 2 binning: each output sample is the sum of a 2 x 2 block.
extern "C" __global__ void bin2x2(const float* in, unsigned int in_width, unsigned int channels,
                                  float* out, unsigned int out_width, unsigned int out_height) {
    unsigned int i = blockIdx.x * blockDim.x + threadIdx.x;
    unsigned int row_len = out_width * channels;
    if (i >= row_len) return;
    unsigned int x = i / channels, c = i % channels;
    unsigned long long in_row = (unsigned long long)in_width * channels;
    for (unsigned int y = blockIdx.y; y < out_height; y += gridDim.y) {
        unsigned long long top = 2ull * y * in_row + 2ull * x * channels + c;
        unsigned long long bottom = top + in_row;
        out[(unsigned long long)y * row_len + i] =
            in[top] + in[top + channels] + in[bottom] + in[bottom + channels];
    }
}

// Box downsampling: each output sample is the mean of a factor x factor
// block, or of the smaller block at the right and bottom edges. Each row of
// the block is summed, then the rows, as the CPU twin does.
extern "C" __global__ void downsample(const float* in, unsigned int width, unsigned int height,
                                      unsigned int channels, unsigned int factor, float* out,
                                      unsigned int out_width, unsigned int out_height) {
    unsigned int i = blockIdx.x * blockDim.x + threadIdx.x;
    unsigned int row_len = out_width * channels;
    if (i >= row_len) return;
    unsigned int x = i / channels, c = i % channels;
    unsigned int x0 = x * factor, x1 = min(x0 + factor, width);
    for (unsigned int oy = blockIdx.y; oy < out_height; oy += gridDim.y) {
        unsigned int y0 = oy * factor, y1 = min(y0 + factor, height);
        float sum = 0.0f;
        for (unsigned int y = y0; y < y1; ++y) {
            float row = 0.0f;
            for (unsigned int column = x0; column < x1; ++column) {
                row += in[((unsigned long long)y * width + column) * channels + c];
            }
            sum += row;
        }
        out[(unsigned long long)oy * row_len + i] = sum / (float)((y1 - y0) * (x1 - x0));
    }
}
