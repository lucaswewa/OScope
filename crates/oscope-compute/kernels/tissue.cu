// The H&E section's tiles on the GPU (ADR-0062): the twin of oscope-sim's
// HeTissue, World::render in transmitted light and its tile cache, which
// are the reference. build.rs compiles this file to a fat binary
// (ADR-0011).
//
// A tile is made in two stages, each kept in an atlas of half-precision
// samples:
//
// 1. he_layers: each pixel's two layers, as the optical density each stain
//    gives per unit of its vector: (hematoxylin, second stain) for the
//    lower layer, then the upper. It depends on the pixel's size and place,
//    not on the focus.
// 2. gather_layers, blur_line or box_line, then compose: a tile and its margin of
//    3σ + 1 pixels from the layers of its 3 × 3 neighbourhood, as
//    transmittances; each layer blurred as blur.rs blurs it; the layers
//    multiplied, then the glass and the sensor's colour response.
//
// Positions on the slide, and the noise's lattice, are doubles, as on the
// CPU; everything after them is floats, which a laptop's GPU runs at 64
// times the rate of doubles. GPU tiles agree with the CPU's within a
// tolerance, not to the bit (ADR-0062).

#include <cuda_fp16.h>

// SplitMix64's finaliser (Steele, Lea and Flood, 2014), as oscope-sim's.
__device__ unsigned long long mix(unsigned long long z) {
    z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ull;
    z = (z ^ (z >> 27)) * 0x94d049bb133111ebull;
    return z ^ (z >> 31);
}

// oscope-sim's hash of a seed, a salt and a cell, from the salted seed `s`,
// mix(seed ^ salt × 0x9e3779b97f4a7c15), which the host makes once.
__device__ unsigned long long cell_hash(unsigned long long s, long long i, long long j) {
    return mix(mix(s ^ (unsigned long long)i) ^ (unsigned long long)j);
}

// A number from 0 up to 1 from a hash's top 24 bits: the CPU takes 53, and
// the difference is under 2^-24, far below half precision's 2^-11.
__device__ float unit(unsigned long long h) {
    return (float)(h >> 40) * (1.0f / 16777216.0f);
}

// The n-th of several numbers from 0 up to 1 drawn from one hash.
__device__ float nth(unsigned long long h, unsigned long long n) {
    return unit(mix(h ^ (n * 0xd6e8feb86659fd93ull)));
}

// Perlin's quintic fade.
__device__ float fade(float t) {
    return t * t * t * (t * (t * 6.0f - 15.0f) + 10.0f);
}

__device__ float lerp(float a, float b, float t) {
    return a + (b - a) * t;
}

// Smooth value noise from 0 to 1, with features about `w` apart. The
// lattice is found in doubles, for places tens of millimetres across the
// slide; the rest is floats.
__device__ float value(unsigned long long s, double x, double y, double w) {
    double u = x / w, v = y / w;
    double i = floor(u), j = floor(v);
    float fx = fade((float)(u - i)), fy = fade((float)(v - j));
    long long ii = (long long)i, jj = (long long)j;
    float c00 = unit(cell_hash(s, ii, jj)), c10 = unit(cell_hash(s, ii + 1, jj));
    float c01 = unit(cell_hash(s, ii, jj + 1)), c11 = unit(cell_hash(s, ii + 1, jj + 1));
    return lerp(lerp(c00, c10, fx), lerp(c01, c11, fx), fy);
}

// Fractal noise from 0 to 1: `octaves` octaves from `w` down, octave k
// salted as `s[k]`, each shorter than half a pixel replaced by its mean.
__device__ float fractal(const unsigned long long* s, double x, double y, double w, int octaves,
                         double pixel) {
    float sum = 0.0f, total = 0.0f, weight = 1.0f;
    for (int o = 0; o < octaves; o++) {
        float shown = fminf(fmaxf((float)(2.0 * w / pixel) - 1.0f, 0.0f), 1.0f);
        if (shown > 0.0f) {
            float n = value(s[o], x, y, w);
            sum += weight * (shown * n + (1.0f - shown) * 0.5f);
        } else {
            sum += weight * 0.5f;
        }
        total += weight;
        weight *= 0.5f;
        w *= 0.5;
    }
    return sum / total;
}

// How much of a pixel of side `pixel` a shape covers, at signed distance
// `d` from its edge.
__device__ float coverage(float d, float pixel) {
    return fminf(fmaxf(0.5f - d / pixel, 0.0f), 1.0f);
}

// The signed distance from an ellipse's edge, near enough for
// anti-aliasing: semi-axes a along the angle, b across it, from the
// ellipse's centre to the point (dx, dy).
__device__ float ellipse(float dx, float dy, float cosa, float sina, float a, float b) {
    float u = dx * cosa + dy * sina, v = -dx * sina + dy * cosa;
    return (sqrtf((u / a) * (u / a) + (v / b) * (v / b)) - 1.0f) * b;
}

// The model's numbers, as oscope-sim's HeTissue::gpu_model lays them out.
#define CELL 0
#define HOLE_CELL 1
#define PER_UM_H 2
#define PER_UM_SECOND 3
#define THICKNESS 4
#define DAB 5
#define DAB_POSITIVE 6
#define DAB_LEVELS 7
#define FIBRE_SIN 10
#define FIBRE_COS 11
#define ISLANDS 16
// Each island: its centre, its radii, its angle's cosine and sine.
#define ISLAND 6

// The salted seeds, in order.
#define S_EDGES 0
#define S_HOLES 3
#define S_THICKNESS 4
#define S_NUCLEI 7
#define S_CELLULARITY 8
#define S_CHROMATIN 9
#define S_BEND_ALONG 12
#define S_BEND_ACROSS 13
#define S_FIBRES 14

// The signed distance from the nearest island's roughened edge, in µm.
__device__ float island_distance(const double* m, const unsigned long long* s, unsigned int islands,
                                 double x, double y, double pixel) {
    if (islands == 0) return INFINITY;
    float plain = INFINITY, radius = 0.0f;
    for (unsigned int k = 0; k < islands; k++) {
        const double* is = m + ISLANDS + k * ISLAND;
        float dx = (float)(x - is[0]), dy = (float)(y - is[1]);
        float cosa = (float)is[4], sina = (float)is[5];
        float u = dx * cosa + dy * sina, v = -dx * sina + dy * cosa;
        float rx = (float)is[2], ry = (float)is[3];
        float d = sqrtf((u / rx) * (u / rx) + (v / ry) * (v / ry)) - 1.0f;
        if (d < plain) {
            plain = d;
            radius = ry;
        }
    }
    if (plain > 0.35f) return plain * radius;
    float rough = fractal(s + S_EDGES, x, y, 500.0, 3, pixel) - 0.5f;
    return (plain + 0.6f * rough) * radius;
}

// How much of the pixel is tissue, and how deep into it, as tissue() is.
__device__ void tissue(const double* m, const unsigned long long* s, unsigned int islands, double x,
                       double y, double pixel, float* cover, float* depth) {
    float soft = fmaxf((float)pixel, 1.0f);
    float edge = island_distance(m, s, islands, x, y, pixel);
    float inside = coverage(edge, soft);
    *cover = 0.0f;
    *depth = 0.0f;
    if (inside == 0.0f) return;
    double cell = m[HOLE_CELL];
    long long i = (long long)floor(x / cell), j = (long long)floor(y / cell);
    float hole = 0.0f;
    for (int dj = -1; dj <= 1; dj++) {
        for (int di = -1; di <= 1; di++) {
            unsigned long long h = cell_hash(s[S_HOLES], i + di, j + dj);
            if (nth(h, 0) >= 0.35f) continue;
            float r = 15.0f + 85.0f * nth(h, 3);
            double cx = ((double)(i + di) + 0.2 + 0.6 * (double)nth(h, 1)) * cell;
            double cy = ((double)(j + dj) + 0.2 + 0.6 * (double)nth(h, 2)) * cell;
            float minor = r * (0.5f + 0.5f * nth(h, 4));
            float sina, cosa;
            sincospif(nth(h, 5), &sina, &cosa);
            float d = ellipse((float)(x - cx), (float)(y - cy), cosa, sina, r, minor);
            hole = fmaxf(hole, coverage(d, soft));
        }
    }
    *cover = inside * (1.0f - hole);
    *depth = -edge;
}

// One pixel of each of `count` layer tiles of `tile` pixels a side, at
// global pixels from `origins` (x, y for each), into `slots` of `atlas`:
// (hematoxylin, second) for the lower layer, then the upper, as the
// optical densities each stain's vector is multiplied by.
extern "C" __global__ void he_layers(const double* m, const unsigned long long* s,
                                     unsigned int islands, double pixel, double slide_x,
                                     double slide_y, const long long* origins,
                                     const unsigned int* slots, unsigned int tile,
                                     __half* atlas) {
    unsigned int x = blockIdx.x * blockDim.x + threadIdx.x;
    unsigned int y = blockIdx.y * blockDim.y + threadIdx.y;
    unsigned int t = blockIdx.z;
    if (x >= tile || y >= tile) return;
    long long gx = origins[2 * t] + x, gy = origins[2 * t + 1] + y;
    // The pixel's centre, on the slide.
    double px = (double)gx * pixel - slide_x + pixel / 2.0;
    double py = (double)gy * pixel - slide_y + pixel / 2.0;
    float p = (float)pixel;
    float out[4] = {0.0f, 0.0f, 0.0f, 0.0f};
    float cover, depth;
    tissue(m, s, islands, px, py, pixel, &cover, &depth);
    if (cover != 0.0f) {
        float thinning = 0.6f + 0.4f * fminf(fmaxf(depth / 150.0f, 0.0f), 1.0f);
        float thickness =
            (float)m[THICKNESS] * (0.75f + 0.5f * fractal(s + S_THICKNESS, px, py, 700.0, 3, pixel)) *
            thinning;
        // Nuclei and the cytoplasm around them, for each layer at once.
        double cell = m[CELL];
        long long i = (long long)floor(px / cell), j = (long long)floor(py / cell);
        float nucleus[2] = {0.0f, 0.0f}, cytoplasm[2] = {0.0f, 0.0f}, dab[2] = {0.0f, 0.0f};
        bool stained = m[DAB] != 0.0;
        for (int dj = -1; dj <= 1; dj++) {
            for (int di = -1; di <= 1; di++) {
                long long ci = i + di, cj = j + dj;
                unsigned long long h = cell_hash(s[S_NUCLEI], ci, cj);
                double cx = ((double)ci + 0.15 + 0.7 * (double)nth(h, 1)) * cell;
                double cy = ((double)cj + 0.15 + 0.7 * (double)nth(h, 2)) * cell;
                float density = 0.25f + 0.6f * value(s[S_CELLULARITY], cx, cy, 250.0);
                if (nth(h, 0) >= density) continue;
                float major = 2.8f + 1.8f * nth(h, 3);
                float minor = major * (0.55f + 0.4f * nth(h, 4));
                float sina, cosa;
                sincospif(nth(h, 5), &sina, &cosa);
                int layer = (int)(h >> 63);
                float hematoxylin = 0.75f + 0.5f * nth(h, 6);
                float amount = 0.0f;
                if (stained && nth(h, 7) < (float)m[DAB_POSITIVE]) {
                    amount = (float)m[DAB_LEVELS + ((unsigned int)(nth(h, 8) * 3.0f)) % 3];
                }
                float d = ellipse((float)(px - cx), (float)(py - cy), cosa, sina, major, minor);
                nucleus[layer] = fmaxf(nucleus[layer], coverage(d, p) * hematoxylin);
                cytoplasm[layer] = fmaxf(cytoplasm[layer], coverage(d - 2.5f, p));
                dab[layer] = fmaxf(dab[layer], coverage(d, p) * amount);
            }
        }
        if (nucleus[0] > 0.0f || nucleus[1] > 0.0f) {
            float chromatin = 0.7f + 0.6f * fractal(s + S_CHROMATIN, px, py, 1.6, 3, pixel);
            for (int l = 0; l < 2; l++) {
                if (nucleus[l] > 0.0f) nucleus[l] *= chromatin;
            }
        }
        // Stroma: fibres along the seed's direction, bent by a gentle warp.
        float bend_along = 60.0f * (value(s[S_BEND_ALONG], px, py, 200.0) - 0.5f);
        float bend_across = 60.0f * (value(s[S_BEND_ACROSS], px, py, 200.0) - 0.5f);
        double along = px * m[FIBRE_COS] + py * m[FIBRE_SIN] + (double)bend_along;
        double across = -px * m[FIBRE_SIN] + py * m[FIBRE_COS] + (double)bend_across;
        float fibres = fractal(s + S_FIBRES, along / 4.0, across, 8.0, 6, pixel);
        float stroma = 0.25f + 0.6f * fibres;
        float amount = cover * thickness / 2.0f;
        for (int l = 0; l < 2; l++) {
            float eosin = (stroma + (0.45f - stroma) * cytoplasm[l]) * (1.0f - 0.6f * fminf(nucleus[l], 1.0f));
            float hematoxylin = nucleus[l] + 0.04f;
            float second = stained ? dab[l] : eosin;
            out[2 * l] = amount * ((float)m[PER_UM_H] * hematoxylin);
            out[2 * l + 1] = amount * ((float)m[PER_UM_SECOND] * second);
        }
    }
    __half* at = atlas + ((unsigned long long)slots[t] * tile * tile + (unsigned long long)y * tile + x) * 4;
    for (int k = 0; k < 4; k++) at[k] = __float2half_rn(out[k]);
}

// A tile and its margin, `width` pixels a side, from the layers of its 3 × 3
// neighbourhood (`neighbours`: their slots, row by row, -1 for none), as
// each layer's transmittance in R, G and B: 10^-(h H + s V), with the
// stains' vectors in `vectors`, H's then V's. Job t's lower layer goes to
// plane 2t of `planes`, its upper to 2t + 1.
extern "C" __global__ void gather_layers(const __half* layers, const int* neighbours, unsigned int tile,
                                  unsigned int width, unsigned int margin, const float* vectors,
                                  float* planes) {
    unsigned int x = blockIdx.x * blockDim.x + threadIdx.x;
    unsigned int y = blockIdx.y * blockDim.y + threadIdx.y;
    unsigned int t = blockIdx.z;
    if (x >= width || y >= width) return;
    int lx = (int)x - (int)margin, ly = (int)y - (int)margin;
    int di = lx < 0 ? -1 : (lx >= (int)tile ? 1 : 0);
    int dj = ly < 0 ? -1 : (ly >= (int)tile ? 1 : 0);
    lx -= di * (int)tile;
    ly -= dj * (int)tile;
    int slot = neighbours[t * 9 + (dj + 1) * 3 + (di + 1)];
    float stains[4] = {0.0f, 0.0f, 0.0f, 0.0f};
    if (slot >= 0) {
        const __half* at =
            layers + ((unsigned long long)slot * tile * tile + (unsigned long long)ly * tile + lx) * 4;
        for (int k = 0; k < 4; k++) stains[k] = __half2float(at[k]);
    }
    unsigned long long plane = (unsigned long long)width * width * 3;
    for (int l = 0; l < 2; l++) {
        float* out = planes + (2ull * t + l) * plane + ((unsigned long long)y * width + x) * 3;
        for (int c = 0; c < 3; c++) {
            float od = stains[2 * l] * vectors[c] + stains[2 * l + 1] * vectors[3 + c];
            out[c] = powf(10.0f, -od);
        }
    }
}

// One plane of each job, plane `layer` of each pair, through a kernel of
// `taps` weights along rows (`columns` 0) or columns (1), repeating the
// edge pixels: blur.rs's convolve.
extern "C" __global__ void blur_line(const float* in, float* out, unsigned int width,
                                     unsigned int layer, unsigned int columns,
                                     const float* weights, unsigned int taps) {
    unsigned int x = blockIdx.x * blockDim.x + threadIdx.x;
    unsigned int y = blockIdx.y * blockDim.y + threadIdx.y;
    unsigned int t = blockIdx.z;
    if (x >= width || y >= width) return;
    unsigned long long base = (2ull * t + layer) * width * width * 3;
    int radius = (int)taps / 2, n = (int)width;
    int along = columns ? (int)y : (int)x;
    for (int c = 0; c < 3; c++) {
        float acc = 0.0f;
        for (int k = 0; k < (int)taps; k++) {
            int at = min(max(along + k - radius, 0), n - 1);
            unsigned long long i = columns ? ((unsigned long long)at * width + x) : ((unsigned long long)y * width + at);
            acc += weights[k] * in[base + i * 3 + c];
        }
        out[base + ((unsigned long long)y * width + x) * 3 + c] = acc;
    }
}

// One plane of each job through a box of `radius`, by a running sum in
// doubles along each row (`columns` 0) or column (1), repeating the edge
// pixels: blur.rs's boxes. A thread for each line and channel.
extern "C" __global__ void box_line(const float* in, float* out, unsigned int width,
                                    unsigned int layer, unsigned int columns,
                                    unsigned int radius) {
    unsigned int line = blockIdx.x * blockDim.x + threadIdx.x;
    unsigned int c = blockIdx.y;
    unsigned int t = blockIdx.z;
    if (line >= width) return;
    unsigned long long base = (2ull * t + layer) * width * width * 3;
    long long n = width, r = radius;
    double span = (double)(2 * r + 1);
    #define AT(i) ((double)in[base + (columns ? ((unsigned long long)min(max((long long)(i), 0ll), n - 1) * width + line) \
                                               : ((unsigned long long)line * width + min(max((long long)(i), 0ll), n - 1))) * 3 + c])
    double sum = 0.0;
    for (long long i = -r; i <= r; i++) sum += AT(i);
    for (long long x = 0; x < n; x++) {
        unsigned long long i = columns ? ((unsigned long long)x * width + line) : ((unsigned long long)line * width + x);
        out[base + i * 3 + c] = (float)(sum / span);
        sum += AT(x + r + 1) - AT(x - r);
    }
    #undef AT
}

// How much of the pixel of side p around (x, y) lies inside a rectangle
// (x, y, width, height), and how much in the dark band along its edge.
__device__ double outside(const double* rect, double x, double y) {
    return fmax(fmax(rect[0] - x, x - rect[0] - rect[2]), fmax(rect[1] - y, y - rect[1] - rect[3]));
}

// Each job's tile, `tile` + 1 pixels a side from global pixels `origins`,
// into `slots` of `atlas`: its layers' blurred transmittances from
// `planes`, inside their margin, multiplied, upper then lower; then the
// glass: `glass` holds the slide's rectangle, the coverslip's, a surface's
// transmittance, an edge's, and the edge's band; then, if `respond`, the
// sensor's colour response, by rows.
extern "C" __global__ void compose(const float* planes, unsigned int width, unsigned int margin,
                                   const long long* origins, const unsigned int* slots,
                                   double pixel, double slide_x, double slide_y,
                                   const double* glass, const float* response,
                                   unsigned int respond, unsigned int tile, __half* atlas) {
    unsigned int x = blockIdx.x * blockDim.x + threadIdx.x;
    unsigned int y = blockIdx.y * blockDim.y + threadIdx.y;
    unsigned int t = blockIdx.z;
    unsigned int side = tile + 1;
    if (x >= side || y >= side) return;
    unsigned long long plane = (unsigned long long)width * width * 3;
    unsigned long long at = ((unsigned long long)(y + margin) * width + (x + margin)) * 3;
    const float* lower = planes + 2ull * t * plane + at;
    const float* upper = planes + (2ull * t + 1) * plane + at;
    double sx = ((double)(origins[2 * t] + x) + 0.5) * pixel - slide_x;
    double sy = ((double)(origins[2 * t + 1] + y) + 0.5) * pixel - slide_y;
    double on_slide = coverage(outside(glass, sx, sy), pixel);
    double covered = coverage(outside(glass + 4, sx, sy), pixel);
    double bands = coverage(fabs(outside(glass, sx, sy)) - glass[10] / 2.0, pixel) +
                   coverage(fabs(outside(glass + 4, sx, sy)) - glass[10] / 2.0, pixel);
    double edges = 1.0 - (1.0 - glass[9]) * fmin(bands, 1.0);
    float through = (float)(pow(glass[8], 2.0 * (on_slide + covered)) * edges);
    float rgb[3];
    for (int c = 0; c < 3; c++) rgb[c] = through * ((1.0f * upper[c]) * lower[c]);
    float shown[3] = {rgb[0], rgb[1], rgb[2]};
    if (respond) {
        for (int c = 0; c < 3; c++) {
            shown[c] = response[c * 3] * rgb[0] + response[c * 3 + 1] * rgb[1] + response[c * 3 + 2] * rgb[2];
        }
    }
    __half* out = atlas + ((unsigned long long)slots[t] * side * side + (unsigned long long)y * side + x) * 3;
    for (int c = 0; c < 3; c++) out[c] = __float2half_rn(shown[c]);
}

// `n` samples to half precision, rounded to nearest: a tile from the CPU.
extern "C" __global__ void to_half(const float* in, unsigned long long n, __half* out) {
    unsigned long long i = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) out[i] = __float2half_rn(in[i]);
}
