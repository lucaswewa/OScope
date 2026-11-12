// OScope's colour kernels for CUDA: the twins of oscope-imaging's colour
// functions (src/color/), which are their reference (ADR-0010).
//
// Every expression adds and multiplies in the order its twin does, and
// build.rs compiles with fused multiply-adds off, so that each step rounds
// as the CPU's does. So the twins agree to the bit.
//
// `cells` describes a Bayer pattern's 2 x 2 cell: two bits a colour (0 red,
// 1 green, 2 blue), for (0, 0), (1, 0), (0, 1) and (1, 1), lowest first.

#define RED 0u
#define BLUE 2u

__device__ unsigned int colour_at(unsigned int cells, unsigned int x, unsigned int y) {
    return (cells >> (2u * ((y & 1u) * 2u + (x & 1u)))) & 3u;
}

// i mirrored into 0..n about the edge pixels (... 2 1 0 1 2 ...); a side of
// 1 or 2 pixels keeps i's parity instead, as the CPU's `mirror` does.
__device__ long long mirror(long long i, long long n) {
    long long m = i < 0 ? -i : (i >= n ? 2 * (n - 1) - i : i);
    if (m >= 0 && m < n) return m;
    long long parity = ((i % 2) + 2) % 2;
    return parity < n - 1 ? parity : n - 1;
}

// Superpixel: each 2 x 2 cell's red, the mean of its greens, and its blue.
extern "C" __global__ void superpixel(const float* in, unsigned int width, unsigned int cells,
                                      float* out, unsigned int out_width, unsigned int out_height) {
    unsigned int x = blockIdx.x * blockDim.x + threadIdx.x;
    if (x >= out_width) return;
    // Where each colour sits in the cell, the greens in the CPU's order.
    unsigned int red = 0, blue = 0, green[2] = {0, 0}, greens = 0;
    for (unsigned int k = 0; k < 4; ++k) {
        unsigned int c = (cells >> (2u * k)) & 3u;
        if (c == RED) red = k;
        else if (c == BLUE) blue = k;
        else green[greens++ & 1u] = k;
    }
    for (unsigned int y = blockIdx.y; y < out_height; y += gridDim.y) {
        const float* cell = in + 2ull * y * width + 2ull * x;
#define AT(k) cell[((k) >> 1) * width + ((k) & 1u)]
        float* o = out + 3ull * ((unsigned long long)y * out_width + x);
        o[0] = AT(red);
        o[1] = (AT(green[0]) + AT(green[1])) / 2.0f;
        o[2] = AT(blue);
#undef AT
    }
}

// The pixel dx across and dy down from (x, y), mirrored at the edges.
struct Window {
    const float* in;
    long long width, height, x, y;
    __device__ float at(int dx, int dy) const {
        return in[mirror(y + dy, height) * width + mirror(x + dx, width)];
    }
};

// What colour a pixel has, and for green, which colour shares its row.
enum Site { SITE_RED, SITE_BLUE, SITE_GREEN_IN_RED_ROW, SITE_GREEN_IN_BLUE_ROW };

__device__ Site site(unsigned int cells, unsigned int x, unsigned int y) {
    unsigned int c = colour_at(cells, x, y);
    if (c == RED) return SITE_RED;
    if (c == BLUE) return SITE_BLUE;
    bool red_row = colour_at(cells, 0, y) == RED || colour_at(cells, 1, y) == RED;
    return red_row ? SITE_GREEN_IN_RED_ROW : SITE_GREEN_IN_BLUE_ROW;
}

__device__ void put(float* o, float r, float g, float b) {
    o[0] = r;
    o[1] = g;
    o[2] = b;
}

__device__ void bilinear(const Window& w, Site s, float* o) {
    float c = w.at(0, 0);
    float cross = (w.at(-1, 0) + w.at(1, 0) + w.at(0, -1) + w.at(0, 1)) / 4.0f;
    float diagonal = (w.at(-1, -1) + w.at(1, -1) + w.at(-1, 1) + w.at(1, 1)) / 4.0f;
    float across = (w.at(-1, 0) + w.at(1, 0)) / 2.0f;
    float down = (w.at(0, -1) + w.at(0, 1)) / 2.0f;
    switch (s) {
        case SITE_RED: put(o, c, cross, diagonal); break;
        case SITE_BLUE: put(o, diagonal, cross, c); break;
        case SITE_GREEN_IN_RED_ROW: put(o, across, c, down); break;
        default: put(o, down, c, across); break;
    }
}

// Malvar, He and Cutler's filters (ICASSP 2004, figure 2), in eighths.
__device__ void malvar_he_cutler(const Window& w, Site s, float* o) {
    float c = w.at(0, 0);
    float across = w.at(-1, 0) + w.at(1, 0);
    float down = w.at(0, -1) + w.at(0, 1);
    float diagonal = w.at(-1, -1) + w.at(1, -1) + w.at(-1, 1) + w.at(1, 1);
    float far_across = w.at(-2, 0) + w.at(2, 0);
    float far_down = w.at(0, -2) + w.at(0, 2);
    float green = (4.0f * c + 2.0f * (across + down) - far_across - far_down) / 8.0f;
    float opposite = (6.0f * c + 2.0f * diagonal - 1.5f * (far_across + far_down)) / 8.0f;
    float in_row = (5.0f * c + 4.0f * across - diagonal - far_across + 0.5f * far_down) / 8.0f;
    float in_column = (5.0f * c + 4.0f * down - diagonal - far_down + 0.5f * far_across) / 8.0f;
    switch (s) {
        case SITE_RED: put(o, c, green, opposite); break;
        case SITE_BLUE: put(o, opposite, green, c); break;
        case SITE_GREEN_IN_RED_ROW: put(o, in_row, c, in_column); break;
        default: put(o, in_column, c, in_row); break;
    }
    for (int i = 0; i < 3; ++i) o[i] = fminf(fmaxf(o[i], 0.0f), 1.0f);
}

// Bilinear (method 0) or Malvar-He-Cutler (method 1), at full size.
extern "C" __global__ void demosaic(const float* in, unsigned int width, unsigned int height,
                                    unsigned int cells, unsigned int method, float* out) {
    unsigned int x = blockIdx.x * blockDim.x + threadIdx.x;
    if (x >= width) return;
    for (unsigned int y = blockIdx.y; y < height; y += gridDim.y) {
        Window w = {in, width, height, x, y};
        float* o = out + 3ull * ((unsigned long long)y * width + x);
        if (method == 0) bilinear(w, site(cells, x, y), o);
        else malvar_he_cutler(w, site(cells, x, y), o);
    }
}

// (v - black) * scale, and 0 below black, in place.
extern "C" __global__ void subtract_black(float* data, unsigned long long len, float black,
                                          float scale) {
    for (unsigned long long i = blockIdx.x * (unsigned long long)blockDim.x + threadIdx.x; i < len;
         i += (unsigned long long)gridDim.x * blockDim.x) {
        data[i] = fmaxf((data[i] - black) * scale, 0.0f);
    }
}

// White-balance gains, then a 3 x 3 matrix by rows, in place.
extern "C" __global__ void correct(float* rgb, unsigned long long pixels, float gr, float gg,
                                   float gb, float m00, float m01, float m02, float m10, float m11,
                                   float m12, float m20, float m21, float m22) {
    for (unsigned long long i = blockIdx.x * (unsigned long long)blockDim.x + threadIdx.x;
         i < pixels; i += (unsigned long long)gridDim.x * blockDim.x) {
        float* p = rgb + 3ull * i;
        float r = p[0] * gr, g = p[1] * gg, b = p[2] * gb;
        p[0] = m00 * r + m01 * g + m02 * b;
        p[1] = m10 * r + m11 * g + m12 * b;
        p[2] = m20 * r + m21 * g + m22 * b;
    }
}

// Each sample through the display's table of 65,536 levels.
extern "C" __global__ void display(const float* in, unsigned long long len,
                                   const unsigned char* table, unsigned char* out) {
    for (unsigned long long i = blockIdx.x * (unsigned long long)blockDim.x + threadIdx.x; i < len;
         i += (unsigned long long)gridDim.x * blockDim.x) {
        out[i] = table[(unsigned int)roundf(fminf(fmaxf(in[i], 0.0f), 1.0f) * 65535.0f)];
    }
}
