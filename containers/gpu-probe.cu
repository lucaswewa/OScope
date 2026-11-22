// R01 environment probe: allocate pinned/device memory, run and verify a kernel.
// This establishes CUDA execution, not scientific correctness or camera support.
#include <cuda_runtime.h>
#include <cstdio>
#include <cstdlib>

static void check(cudaError_t result, const char* operation) {
    if (result != cudaSuccess) {
        std::fprintf(stderr, "%s failed: %s\n", operation, cudaGetErrorString(result));
        std::exit(1);
    }
}

__global__ void calculate(int* output, int count) {
    const int index = blockIdx.x * blockDim.x + threadIdx.x;
    if (index < count) output[index] = 3 * index + 7;
}

int main() {
    int devices = 0;
    check(cudaGetDeviceCount(&devices), "find CUDA devices");
    if (devices == 0) {
        std::fprintf(stderr, "GPU check requires a real CUDA device\n");
        return 1;
    }
    check(cudaSetDevice(0), "select CUDA device");
    cudaDeviceProp properties{};
    check(cudaGetDeviceProperties(&properties, 0), "read device properties");
    constexpr int count = 4096;
    constexpr size_t bytes = count * sizeof(int);
    int* host = nullptr;
    int* device = nullptr;
    check(cudaHostAlloc(reinterpret_cast<void**>(&host), bytes, cudaHostAllocDefault),
          "allocate pinned memory");
    check(cudaMalloc(reinterpret_cast<void**>(&device), bytes), "allocate GPU memory");
    calculate<<<(count + 255) / 256, 256>>>(device, count);
    check(cudaGetLastError(), "launch CUDA kernel");
    check(cudaDeviceSynchronize(), "complete CUDA kernel");
    check(cudaMemcpy(host, device, bytes, cudaMemcpyDeviceToHost), "download results");
    for (int index = 0; index < count; ++index) {
        if (host[index] != 3 * index + 7) {
            std::fprintf(stderr, "GPU result mismatch at %d\n", index);
            return 1;
        }
    }
    check(cudaFree(device), "free GPU memory");
    check(cudaFreeHost(host), "free pinned memory");
    int driver = 0;
    int runtime = 0;
    check(cudaDriverGetVersion(&driver), "read driver version");
    check(cudaRuntimeGetVersion(&runtime), "read runtime version");
    std::printf("CUDA kernel passed: %d values on %s; driver %d, runtime %d\n",
                count, properties.name, driver, runtime);
    return 0;
}
