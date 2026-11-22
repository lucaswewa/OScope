// OScope's filter kernels for CUDA (P23): the twins of oscope-compute's
// filters (src/filters.rs), which are their reference (ADR-0010).
//
// Every expression adds and multiplies in the order its twin does, and
// build.rs compiles with fused multiply-adds off, so that each step rounds
// as the CPU's does. The Gaussian and median filters and CLAHE agree to the
// bit; the bilateral filter's exponentials may differ by an ulp or two.
// Pixels beyond the edges repeat the edge pixels.

// i within 0..n: the edge's index beyond the edges.
__device__ long long clamp_index(long long i, long long n) {
    return i < 0 ? 0 : (i >= n ? n - 1 : i);
}

// One pass of a separable convolution with 2r + 1 weights, along rows or
// down columns: a thread for each sample of a row, each block of rows
// stepping down.
extern "C" __global__ void convolve(const float* in, float* out, unsigned int width,
                                    unsigned int height, unsigned int channels,
                                    const float* weights, unsigned int radius,
                                    unsigned int along_rows) {
    unsigned int i = blockIdx.x * blockDim.x + threadIdx.x;
    unsigned int row_len = width * channels;
    if (i >= row_len) return;
    long long x = i / channels;
    unsigned int c = i % channels;
    long long r = radius;
    for (unsigned int y = blockIdx.y; y < height; y += gridDim.y) {
        float sum = 0.0f;
        for (long long k = -r; k <= r; ++k) {
            long long xx = along_rows ? clamp_index(x + k, width) : x;
            long long yy = along_rows ? (long long)y : clamp_index((long long)y + k, height);
            sum = sum + weights[k + r] * in[(yy * width + xx) * channels + c];
        }
        out[(unsigned long long)y * row_len + i] = sum;
    }
}

// The k-th smallest of a[0..n], by quickselect: the same value the CPU's
// selection finds.
__device__ float select_kth(float* a, int n, int k) {
    int lo = 0, hi = n - 1;
    while (lo < hi) {
        float pivot = a[(lo + hi) / 2];
        int i = lo, j = hi;
        while (i <= j) {
            while (a[i] < pivot) ++i;
            while (a[j] > pivot) --j;
            if (i <= j) {
                float t = a[i];
                a[i] = a[j];
                a[j] = t;
                ++i;
                --j;
            }
        }
        if (k <= j) hi = j;
        else if (k >= i) lo = i;
        else break;
    }
    return a[k];
}

#define MEDIAN_SIDE 15

// The median of each (2r + 1)^2 window, channel by channel, for r up to 7.
extern "C" __global__ void median(const float* in, float* out, unsigned int width,
                                  unsigned int height, unsigned int channels,
                                  unsigned int radius) {
    unsigned int i = blockIdx.x * blockDim.x + threadIdx.x;
    unsigned int row_len = width * channels;
    if (i >= row_len) return;
    long long x = i / channels;
    unsigned int c = i % channels;
    long long r = radius;
    float window[MEDIAN_SIDE * MEDIAN_SIDE];
    for (unsigned int y = blockIdx.y; y < height; y += gridDim.y) {
        int n = 0;
        for (long long dy = -r; dy <= r; ++dy) {
            long long yy = clamp_index((long long)y + dy, height);
            for (long long dx = -r; dx <= r; ++dx) {
                window[n++] = in[(yy * width + clamp_index(x + dx, width)) * channels + c];
            }
        }
        out[(unsigned long long)y * row_len + i] = select_kth(window, n, n / 2);
    }
}

// The bilateral filter: a thread for each pixel of a row, up to four
// channels; `spatial` holds the window's distance weights, row by row.
extern "C" __global__ void bilateral(const float* in, float* out, unsigned int width,
                                     unsigned int height, unsigned int channels,
                                     const float* spatial, unsigned int radius,
                                     float inverse) {
    unsigned int x = blockIdx.x * blockDim.x + threadIdx.x;
    if (x >= width) return;
    long long r = radius;
    for (unsigned int y = blockIdx.y; y < height; y += gridDim.y) {
        const float* centre = in + ((unsigned long long)y * width + x) * channels;
        float sums[4] = {0.0f, 0.0f, 0.0f, 0.0f};
        float total = 0.0f;
        unsigned int t = 0;
        for (long long dy = -r; dy <= r; ++dy) {
            long long yy = clamp_index((long long)y + dy, height);
            for (long long dx = -r; dx <= r; ++dx) {
                const float* q = in + (yy * width + clamp_index((long long)x + dx, width)) * channels;
                float d2 = 0.0f;
                for (unsigned int c = 0; c < channels; ++c) {
                    float d = q[c] - centre[c];
                    d2 = d2 + d * d;
                }
                float w = spatial[t] * expf(-d2 * inverse);
                t += 1;
                total = total + w;
                for (unsigned int c = 0; c < channels; ++c) sums[c] = sums[c] + w * q[c];
            }
        }
        float* to = out + ((unsigned long long)y * width + x) * channels;
        for (unsigned int c = 0; c < channels; ++c) to[c] = sums[c] / total;
    }
}

// The bin of v among `bins` over 0-1.
__device__ unsigned int clahe_bin(float v, unsigned int bins) {
    float t = v < 0.0f ? 0.0f : (v > 1.0f ? 1.0f : v);
    unsigned int b = (unsigned int)(t * (float)bins);
    return b < bins ? b : bins - 1;
}

// Tile t of `tiles` along n pixels: its first pixel.
__device__ unsigned int tile_start(unsigned int t, unsigned int tiles, unsigned int n) {
    return (unsigned int)((unsigned long long)t * n / tiles);
}

// Each tile's lookup table, for one channel: a block for each tile, row by
// row, along x, and each channel along y; the histogram in shared memory.
// The clip, from the tile's pixels, and the clipping, sharing and
// cumulating are the CPU's, on one thread.
extern "C" __global__ void clahe_maps(const float* in, unsigned int width, unsigned int height,
                                      unsigned int channels, unsigned int tiles_x,
                                      unsigned int tiles_y, unsigned int bins,
                                      float clip_factor, float* maps) {
    extern __shared__ unsigned int histogram[];
    unsigned int tile = blockIdx.x, c = blockIdx.y;
    unsigned int tx = tile % tiles_x, ty = tile / tiles_x;
    unsigned int x0 = tile_start(tx, tiles_x, width), x1 = tile_start(tx + 1, tiles_x, width);
    unsigned int y0 = tile_start(ty, tiles_y, height), y1 = tile_start(ty + 1, tiles_y, height);
    for (unsigned int b = threadIdx.x; b < bins; b += blockDim.x) histogram[b] = 0;
    __syncthreads();
    unsigned int w = x1 - x0, count = w * (y1 - y0);
    for (unsigned int k = threadIdx.x; k < count; k += blockDim.x) {
        unsigned int x = x0 + k % w, y = y0 + k / w;
        float v = in[((unsigned long long)y * width + x) * channels + c];
        atomicAdd(&histogram[clahe_bin(v, bins)], 1u);
    }
    __syncthreads();
    if (threadIdx.x != 0) return;
    unsigned int clip = (unsigned int)(clip_factor * (float)count / (float)bins);
    if (clip < 1) clip = 1;
    unsigned int excess = 0;
    for (unsigned int b = 0; b < bins; ++b) {
        if (histogram[b] > clip) {
            excess += histogram[b] - clip;
            histogram[b] = clip;
        }
    }
    unsigned int each = excess / bins, rest = excess % bins, sum = 0;
    float* map = maps + ((unsigned long long)c * tiles_y * tiles_x + tile) * bins;
    for (unsigned int b = 0; b < bins; ++b) {
        sum += histogram[b] + each + (b < rest ? 1u : 0u);
        map[b] = count > 0 ? (float)sum / (float)count : 0.0f;
    }
}

// The tiles whose centres are either side of pixel p, along n pixels in
// `tiles` tiles, and how far p is from the first towards the second.
__device__ void around(unsigned int p, unsigned int n, unsigned int tiles, unsigned int* first,
                       unsigned int* second, float* f) {
    float at = (float)p + 0.5f;
    float low = 0.5f * (float)(tile_start(0, tiles, n) + tile_start(1, tiles, n));
    *first = 0;
    *second = 0;
    *f = 0.0f;
    if (at <= low) return;
    for (unsigned int t = 0; t + 1 < tiles; ++t) {
        float high = 0.5f * (float)(tile_start(t + 1, tiles, n) + tile_start(t + 2, tiles, n));
        if (at < high) {
            *first = t;
            *second = t + 1;
            *f = (at - low) / (high - low);
            return;
        }
        low = high;
    }
    *first = tiles - 1;
    *second = tiles - 1;
}

// Each sample through its tile's table, interpolated between the four
// tiles around it: a thread for each sample of a row.
extern "C" __global__ void clahe_apply(const float* in, float* out, unsigned int width,
                                       unsigned int height, unsigned int channels,
                                       unsigned int tiles_x, unsigned int tiles_y,
                                       unsigned int bins, const float* maps) {
    unsigned int i = blockIdx.x * blockDim.x + threadIdx.x;
    unsigned int row_len = width * channels;
    if (i >= row_len) return;
    unsigned int x = i / channels, c = i % channels;
    unsigned int i0, i1;
    float fx;
    around(x, width, tiles_x, &i0, &i1, &fx);
    const float* m = maps + (unsigned long long)c * tiles_y * tiles_x * bins;
    for (unsigned int y = blockIdx.y; y < height; y += gridDim.y) {
        unsigned int j0, j1;
        float fy;
        around(y, height, tiles_y, &j0, &j1, &fy);
        unsigned long long at = (unsigned long long)y * row_len + i;
        unsigned int b = clahe_bin(in[at], bins);
        float top = (1.0f - fx) * m[(j0 * tiles_x + i0) * bins + b] + fx * m[(j0 * tiles_x + i1) * bins + b];
        float bottom = (1.0f - fx) * m[(j1 * tiles_x + i0) * bins + b] + fx * m[(j1 * tiles_x + i1) * bins + b];
        out[at] = (1.0f - fy) * top + fy * bottom;
    }
}
