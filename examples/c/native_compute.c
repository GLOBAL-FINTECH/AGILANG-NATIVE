#include "../../include/agilang_runtime.h"
#include <stdio.h>

int main(void) {
    size_t shape[] = {2, 2};
    double values[] = {1, 2, 3, 4};
    uint64_t tensor = 0, result = 0;
    if (agi_tensor_create_f64(shape, 2, values, 4, &tensor) != 0) return 1;
    if (agi_tensor_scale(tensor, 2.0, &result) != 0) return 2;
    double output[4] = {0};
    if (agi_tensor_read_f64(result, output, 4) != 0) return 3;
    printf("[%g, %g, %g, %g]\\n", output[0], output[1], output[2], output[3]);
    agi_tensor_release(result);
    agi_tensor_release(tensor);
    return 0;
}
