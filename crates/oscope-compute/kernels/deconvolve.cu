// OScope's Richardson-Lucy for CUDA (P43): the twin of oscope-volume's
// deconvolve (src/deconvolve.rs), its reference. cuFFT's 3D real
// transforms make the convolutions; these kernels the steps between them,
// with the CPU's arithmetic. A volume is x fastest, then y, then z.

// Each value of a half spectrum times the transfer function's, or its
// conjugate's if `mirrored`, and `scale`, the inverse transform's 1/N.
extern "C" __global__ void rl_spectrum(float2* spectrum, const float2* otf, unsigned long long n,
                                       int mirrored, float scale) {
    unsigned long long i = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;
    if (i >= n) return;
    float2 s = spectrum[i];
    float2 h = otf[i];
    float hi = mirrored ? -h.y : h.y;
    float re = s.x * h.x - s.y * hi;
    float im = s.x * hi + s.y * h.x;
    spectrum[i] = make_float2(re * scale, im * scale);
}

// The data over the blurred estimate, in place: 0 where it is `floor` or
// under.
extern "C" __global__ void rl_ratio(const float* data, float* blurred, unsigned long long n,
                                    float floor) {
    unsigned long long i = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;
    if (i >= n) return;
    float b = blurred[i];
    blurred[i] = b > floor ? data[i] / b : 0.0f;
}

// Total variation's unit normals, periodic: the gradient by forward
// differences over its length, three values a voxel.
extern "C" __global__ void rl_normals(const float* u, float* normals, unsigned int nx,
                                      unsigned int ny, unsigned int nz) {
    unsigned long long i = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;
    unsigned long long n = (unsigned long long)nx * ny * nz;
    if (i >= n) return;
    unsigned long long x = i % nx, y = (i / nx) % ny, z = i / ((unsigned long long)nx * ny);
    unsigned long long row = (unsigned long long)nx, slab = (unsigned long long)nx * ny;
    float c = u[i];
    float gx = u[z * slab + y * row + (x + 1) % nx] - c;
    float gy = u[z * slab + ((y + 1) % ny) * row + x] - c;
    float gz = u[((z + 1) % nz) * slab + y * row + x] - c;
    float length = sqrtf(gx * gx + gy * gy + gz * gz + 1e-12f);
    normals[3 * i] = gx / length;
    normals[3 * i + 1] = gy / length;
    normals[3 * i + 2] = gz / length;
}

// The estimate times its correction and, with total variation, by
// 1 / max(0.5, 1 - lambda div), the divergence by backward differences.
extern "C" __global__ void rl_update(float* u, const float* correction, const float* normals,
                                     unsigned int nx, unsigned int ny, unsigned int nz,
                                     float lambda, int tv) {
    unsigned long long i = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;
    unsigned long long n = (unsigned long long)nx * ny * nz;
    if (i >= n) return;
    float factor = 1.0f;
    if (tv) {
        unsigned long long x = i % nx, y = (i / nx) % ny, z = i / ((unsigned long long)nx * ny);
        unsigned long long row = (unsigned long long)nx, slab = (unsigned long long)nx * ny;
        unsigned long long px = z * slab + y * row + (x + nx - 1) % nx;
        unsigned long long py = z * slab + ((y + ny - 1) % ny) * row + x;
        unsigned long long pz = ((z + nz - 1) % nz) * slab + y * row + x;
        float div = normals[3 * i] - normals[3 * px] + normals[3 * i + 1] - normals[3 * py + 1] +
                    normals[3 * i + 2] - normals[3 * pz + 2];
        factor = 1.0f / fmaxf(1.0f - lambda * div, 0.5f);
    }
    u[i] = u[i] * (correction[i] * factor);
}
