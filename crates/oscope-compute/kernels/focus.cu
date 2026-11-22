// OScope's focus measures for CUDA (P34): the twin of oscope-compute's
// focus (src/focus.rs), which is their reference (ADR-0010).
//
// Each thread computes one pixel's terms with the arithmetic of focus.rs's
// `terms`, step for step; build.rs compiles with fused multiply-adds off, so
// each rounds as the CPU's does. The CPU sums the terms.

// Pixel i's terms in an image of width x height: Brenner's, the
// differences two pixels on along x and y squared, where they're inside;
// Tenengrad's and the Laplacian, where its 3 x 3 neighbourhood is inside,
// and 0 elsewhere.
extern "C" __global__ void focus_terms(const float* in, float* brenner, float* tenengrad,
                                       float* laplacian, unsigned int width,
                                       unsigned int height) {
    unsigned long long i = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;
    unsigned long long n = (unsigned long long)width * height;
    if (i >= n) return;
    unsigned int x = (unsigned int)(i % width);
    unsigned int y = (unsigned int)(i / width);
    float c = in[i];
    float b = 0.0f;
    if (x + 2 < width) {
        float d = in[i + 2] - c;
        b = b + d * d;
    }
    if (y + 2 < height) {
        float d = in[i + 2 * (unsigned long long)width] - c;
        b = b + d * d;
    }
    brenner[i] = b;
    if (x == 0 || y == 0 || x + 1 >= width || y + 1 >= height) {
        tenengrad[i] = 0.0f;
        laplacian[i] = 0.0f;
        return;
    }
    unsigned long long w = width;
    float a00 = in[i - w - 1], a01 = in[i - w], a02 = in[i - w + 1];
    float a10 = in[i - 1], a12 = in[i + 1];
    float a20 = in[i + w - 1], a21 = in[i + w], a22 = in[i + w + 1];
    float gx = (a02 + 2.0f * a12 + a22) - (a00 + 2.0f * a10 + a20);
    float gy = (a20 + 2.0f * a21 + a22) - (a00 + 2.0f * a01 + a02);
    tenengrad[i] = gx * gx + gy * gy;
    laplacian[i] = a01 + a21 + a10 + a12 - 4.0f * c;
}

// Pixel i's modified Laplacian with neighbours `step` pixels away (P42):
// |2I - I(x - s) - I(x + s)| + |2I - I(y - s) - I(y + s)|, where all four
// are inside, and 0 elsewhere. focus.rs's `modified_laplacian`, step for
// step.
extern "C" __global__ void modified_laplacian(const float* in, float* out, unsigned int width,
                                              unsigned int height, unsigned int step) {
    unsigned long long i = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;
    unsigned long long n = (unsigned long long)width * height;
    if (i >= n) return;
    unsigned int x = (unsigned int)(i % width);
    unsigned int y = (unsigned int)(i / width);
    if (x < step || y < step || x + step >= width || y + step >= height) {
        out[i] = 0.0f;
        return;
    }
    unsigned long long s = step;
    unsigned long long sw = s * width;
    float c = in[i];
    float across = fabsf(2.0f * c - in[i - s] - in[i + s]);
    float down = fabsf(2.0f * c - in[i - sw] - in[i + sw]);
    out[i] = across + down;
}

// Each pixel's sum over the window of `radius` about it, within the image:
// along its row if `across`, else down its column, in order. focus.rs's
// `window_sums`.
extern "C" __global__ void window_sums(const float* in, float* out, unsigned int width,
                                       unsigned int height, unsigned int radius,
                                       unsigned int across) {
    unsigned long long i = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;
    unsigned long long n = (unsigned long long)width * height;
    if (i >= n) return;
    unsigned int x = (unsigned int)(i % width);
    unsigned int y = (unsigned int)(i / width);
    unsigned long long w = width;
    float sum = 0.0f;
    if (across) {
        unsigned int from = x > radius ? x - radius : 0;
        unsigned int to = x + radius + 1 < width ? x + radius + 1 : width;
        for (unsigned int k = from; k < to; k++) sum = sum + in[y * w + k];
    } else {
        unsigned int from = y > radius ? y - radius : 0;
        unsigned int to = y + radius + 1 < height ? y + radius + 1 : height;
        for (unsigned int k = from; k < to; k++) sum = sum + in[k * w + x];
    }
    out[i] = sum;
}
