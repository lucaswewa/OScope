// The simulated camera's exposure on the GPU (P15): the twin of the CPU
// path in oscope-sim's camera, which is its reference. build.rs compiles
// this file to a fat binary (ADR-0011).
//
// Each output pixel takes the light through the camera's mounting from
// tiles of the rendered world, averaged over the stage's places during its
// row's exposure, and turns it into counts as the sensor model does: shot
// noise, dark current, read noise, a black level and a 12-bit ADC.
//
// The twins agree without noise to within a count: the CPU works in doubles,
// this in floats. With noise they agree in distribution, not draw by draw:
// each pixel here draws from a counter-based generator of its own.

// SplitMix64's finaliser (Steele, Lea and Flood, 2014), as oscope-sim's.
__device__ unsigned long long mix(unsigned long long z) {
    z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ull;
    z = (z ^ (z >> 27)) * 0x94d049bb133111ebull;
    return z ^ (z >> 31);
}

// A pixel's own sequence of numbers from 0 up to 1.
struct Draws {
    unsigned long long state;
    __device__ float uniform() {
        state += 0x9e3779b97f4a7c15ull;
        return (float)(mix(state) >> 40) * (1.0f / 16777216.0f);
    }
    // A standard normal number, by the Box-Muller transform.
    __device__ float normal() {
        float u = 1.0f - uniform();
        float v = uniform();
        return sqrtf(-2.0f * logf(u)) * cosf(6.28318530718f * v);
    }
    // A Poisson number of mean `mean`: Knuth's product of uniforms up to
    // 30, a rounded normal beyond, as the CPU's.
    __device__ float poisson(float mean) {
        if (mean <= 0.0f) return 0.0f;
        if (mean > 30.0f) return fmaxf(roundf(mean + sqrtf(mean) * normal()), 0.0f);
        float limit = expf(-mean), product = uniform(), k = 0.0f;
        while (product > limit) {
            k += 1.0f;
            product *= uniform();
        }
        return k;
    }
};

// Channel `c` at (x, y), in micrometres from the grid's corner: between the
// four nearest pixel centres, from the tile that holds them. Each tile has
// `tile` + 1 pixels a side, one beyond the next tile's start.
__device__ float sample(const float* atlas, const int* grid, unsigned int columns,
                        unsigned int rows, unsigned int tile, float pixel, float x, float y,
                        int c) {
    float fx = fminf(fmaxf(x / pixel - 0.5f, 0.0f), (float)(columns * tile - 1));
    float fy = fminf(fmaxf(y / pixel - 0.5f, 0.0f), (float)(rows * tile - 1));
    float gx = floorf(fx), gy = floorf(fy);
    float ax = fx - gx, ay = fy - gy;
    unsigned int ix = (unsigned int)gx, iy = (unsigned int)gy;
    unsigned int column = ix / tile, row = iy / tile;
    unsigned int lx = ix - column * tile, ly = iy - row * tile;
    unsigned long long side = tile + 1;
    const float* t = atlas + (unsigned long long)grid[row * columns + column] * side * side * 3;
    float p00 = t[(ly * side + lx) * 3 + c], p10 = t[(ly * side + lx + 1) * 3 + c];
    float p01 = t[((ly + 1) * side + lx) * 3 + c], p11 = t[((ly + 1) * side + lx + 1) * 3 + c];
    float top = p00 + (p10 - p00) * ax;
    float bottom = p01 + (p11 - p01) * ax;
    return top + (bottom - top) * ay;
}

// One frame, `width` x `height`. Pixel (u, v) sees the stage point
// (a u + b v + c, d u + e v + f) from the stage's place; row v's `samples[v]`
// places are at `places[(v * most + j) * 2]`, in micrometres from the grid's
// corner. Counts are black + electrons x cpe, rounded, from 0 to 4095.
extern "C" __global__ void expose(const float* atlas, const int* grid, unsigned int columns,
                                  unsigned int rows, unsigned int tile, float pixel,
                                  const float* places, const unsigned int* samples,
                                  unsigned int most, float a, float b, float c, float d, float e,
                                  float f, unsigned int width, unsigned int height, float scale,
                                  float dark, float read, float black, float cpe,
                                  unsigned int noise, unsigned long long seed,
                                  unsigned long long frame, float vx0, float vy0, float vk,
                                  float vr2, float vs, unsigned short* out) {
    unsigned int u = blockIdx.x * blockDim.x + threadIdx.x;
    if (u >= width) return;
    for (unsigned int v = blockIdx.y; v < height; v += gridDim.y) {
        float hx = a * u + b * v + c, hy = d * u + e * v + f;
        // RGGB: red, green / green, blue.
        int channel = (v % 2 == 0) ? ((u % 2 == 0) ? 0 : 1) : ((u % 2 == 0) ? 1 : 2);
        unsigned int n = samples[v];
        float light = 0.0f;
        for (unsigned int j = 0; j < n; j++) {
            const float* place = places + ((unsigned long long)v * most + j) * 2;
            light += sample(atlas, grid, columns, rows, tile, pixel, place[0] + hx,
                            place[1] + hy, channel);
        }
        // The vignetting, cos^4's law: 1 / (1 + s r^2)^2.
        float dx = vx0 + vk * u, dy = vy0 + vk * v;
        float fall = 1.0f + vs * (dx * dx + dy * dy) / vr2;
        float mean = light / (float)n * scale * (1.0f / (fall * fall)) + dark;
        float electrons = mean;
        if (noise) {
            Draws draws = {mix(seed ^ mix(frame ^ mix((unsigned long long)v * width + u)))};
            electrons = draws.poisson(mean) + draws.normal() * read;
        }
        float counts = roundf(black + electrons * cpe);
        out[(unsigned long long)v * width + u] = (unsigned short)fminf(fmaxf(counts, 0.0f), 4095.0f);
    }
}
