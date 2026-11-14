// OScope's phase correlation kernels for CUDA (P31): the twins of
// oscope-imaging's registration steps (src/registration.rs), around cuFFT's
// transforms. Each expression adds and multiplies in the order its twin
// does, and build.rs compiles with fused multiply-adds off.

// Each image's mean taken away, and the window applied, in place: `count`
// images of `width` x `height`, one after another.
extern "C" __global__ void prepare(float* images, const float* means, const float* across,
                                   const float* down, unsigned int width, unsigned int height,
                                   unsigned int count) {
    unsigned long long len = (unsigned long long)width * height;
    unsigned long long total = len * count;
    for (unsigned long long i = blockIdx.x * (unsigned long long)blockDim.x + threadIdx.x;
         i < total; i += (unsigned long long)gridDim.x * blockDim.x) {
        unsigned long long image = i / len, p = i % len;
        unsigned int x = (unsigned int)(p % width), y = (unsigned int)(p / width);
        images[i] = (images[i] - means[image]) * across[x] * down[y];
    }
}

// Each pair's normalised cross power, F_B conj(F_A) / |F_A| |F_B|, weighted:
// the pairs' half spectra, A's then B's, `half` x `height` each, into `out`,
// one a pair. Below a magnitude of 1e-30, 0.
extern "C" __global__ void cross_power(const float2* spectra, const float* across,
                                       const float* down, unsigned int half, unsigned int height,
                                       unsigned int pairs, float2* out) {
    unsigned long long len = (unsigned long long)half * height;
    unsigned long long total = len * pairs;
    for (unsigned long long i = blockIdx.x * (unsigned long long)blockDim.x + threadIdx.x;
         i < total; i += (unsigned long long)gridDim.x * blockDim.x) {
        unsigned long long pair = i / len, k = i % len;
        unsigned int kx = (unsigned int)(k % half), ky = (unsigned int)(k / half);
        float2 a = spectra[2ull * pair * len + k];
        float2 b = spectra[(2ull * pair + 1ull) * len + k];
        float re = b.x * a.x + b.y * a.y;
        float im = b.y * a.x - b.x * a.y;
        float magnitude = sqrtf(a.x * a.x + a.y * a.y) * sqrtf(b.x * b.x + b.y * b.y);
        float2 r = make_float2(0.0f, 0.0f);
        if (!(magnitude < 1e-30f)) {
            float scale = across[kx] * down[ky] / magnitude;
            r = make_float2(re * scale, im * scale);
        }
        out[i] = r;
    }
}

// The region of each pair's surface, from shift (x0, y0), `region_width` x
// `region_height` of them, wrapping round the surface, and scaled.
extern "C" __global__ void gather(const float* surfaces, unsigned int width, unsigned int height,
                                  unsigned int pairs, long long x0, long long y0,
                                  unsigned int region_width, unsigned int region_height,
                                  float scale, float* out) {
    unsigned long long len = (unsigned long long)region_width * region_height;
    unsigned long long total = len * pairs;
    for (unsigned long long i = blockIdx.x * (unsigned long long)blockDim.x + threadIdx.x;
         i < total; i += (unsigned long long)gridDim.x * blockDim.x) {
        unsigned long long pair = i / len, k = i % len;
        long long x = (x0 + (long long)(k % region_width)) % (long long)width;
        long long y = (y0 + (long long)(k / region_width)) % (long long)height;
        x = x < 0 ? x + width : x;
        y = y < 0 ? y + height : y;
        unsigned long long surface = pair * width * height;
        out[i] = surfaces[surface + (unsigned long long)y * width + (unsigned long long)x] * scale;
    }
}
