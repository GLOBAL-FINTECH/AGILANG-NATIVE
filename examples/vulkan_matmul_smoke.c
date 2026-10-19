#include <stdio.h>
#include "agilang_runtime.h"

// Build the Vulkan-featured AGILANG runtime DLL first. Compile with MSVC:
// cl /I include examples\\vulkan_matmul_smoke.c /link target\\release\\agilang_runtime_abi.lib
int main(void) {
    const float a[] = { 1, 2, 3, 4, 5, 6 };
    const float b[] = { 7, 8, 9, 10, 11, 12 };
    float output[4] = { 0 };
    // Policy 2 requires a real Vulkan device (never falls back to CPU).
    const uint32_t status = agi_accelerator_matmul_f32(
        a, 6, b, 6, 2, 3, 2, 2, output, 4
    );
    if (status != 0) {
        AgiBuffer error = { 0 };
        if (agi_runtime_last_error_json(&error) == 0) {
            fprintf(stderr, "GPU error: %.*s\n", (int)error.len, error.data);
            agi_buffer_free(&error);
        }
        return (int)status;
    }
    const float expected[] = { 58, 64, 139, 154 };
    for (size_t i = 0; i < 4; i++) {
        printf("%zu: %.2f (expected %.2f)\n", i, output[i], expected[i]);
        if (output[i] < expected[i] - 0.001f || output[i] > expected[i] + 0.001f) {
            return 1;
        }
    }
    puts("AGILANG Vulkan matmul: GPU verification PASSED");
    return 0;
}
