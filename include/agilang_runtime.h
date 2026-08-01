#ifndef AGILANG_RUNTIME_H
#define AGILANG_RUNTIME_H
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#ifdef _WIN32
#define AGI_API __declspec(dllimport)
#else
#define AGI_API
#endif
#ifdef __cplusplus
extern "C" {
#endif

typedef struct AgiBuffer { uint8_t* data; size_t len; size_t capacity; } AgiBuffer;

AGI_API uint32_t agi_runtime_abi_version(void);
AGI_API void agi_runtime_reset(void);
AGI_API uint32_t agi_runtime_last_error_code(void);
AGI_API uint32_t agi_runtime_last_error_json(AgiBuffer* out);
AGI_API void agi_buffer_free(AgiBuffer* buffer);

AGI_API uint32_t agi_compute_capabilities_json(AgiBuffer* out);
AGI_API uint32_t agi_accelerator_capabilities_json(AgiBuffer* out);
AGI_API uint32_t agi_resource_capabilities_json(AgiBuffer* out);
AGI_API uint32_t agi_webrtc_capabilities_json(AgiBuffer* out);
AGI_API uint32_t agi_crypto_sha256(const uint8_t* data, size_t len, uint8_t* out, size_t out_len);
AGI_API uint32_t agi_crypto_hmac_sha256(const uint8_t* key, size_t key_len, const uint8_t* data, size_t data_len, uint8_t* out, size_t out_len);
AGI_API uint32_t agi_crypto_random(uint8_t* out, size_t len);
AGI_API uint32_t agi_crypto_constant_time_eq(const uint8_t* a, size_t a_len, const uint8_t* b, size_t b_len, bool* out);
AGI_API const char* agi_http_get_json(const char* url);
AGI_API const char* agi_http_get(const char* url);
AGI_API const char* agi_http_post(const char* url, const char* body);
AGI_API const char* agi_http_post_json(const char* url, const char* body);
AGI_API const char* agi_http_request_json(const char* request_json);

AGI_API uint32_t agi_tensor_create_f64(const size_t* shape, size_t rank, const double* data, size_t len, uint64_t* out);
AGI_API uint32_t agi_tensor_release(uint64_t handle);
AGI_API uint32_t agi_tensor_rank(uint64_t handle, size_t* out);
AGI_API uint32_t agi_tensor_len(uint64_t handle, size_t* out);
AGI_API uint32_t agi_tensor_shape(uint64_t handle, size_t* out, size_t capacity);
AGI_API uint32_t agi_tensor_read_f64(uint64_t handle, double* out, size_t capacity);
AGI_API uint32_t agi_tensor_add(uint64_t left, uint64_t right, uint64_t* out);
AGI_API uint32_t agi_tensor_sub(uint64_t left, uint64_t right, uint64_t* out);
AGI_API uint32_t agi_tensor_hadamard(uint64_t left, uint64_t right, uint64_t* out);
AGI_API uint32_t agi_tensor_matmul(uint64_t left, uint64_t right, uint64_t* out);
AGI_API uint32_t agi_tensor_scale(uint64_t handle, double factor, uint64_t* out);
AGI_API uint32_t agi_tensor_relu(uint64_t handle, uint64_t* out);
AGI_API uint32_t agi_tensor_softmax(uint64_t handle, uint64_t* out);
AGI_API uint32_t agi_tensor_mse(uint64_t left, uint64_t right, double* out);

AGI_API uint32_t agi_linear_model_create(size_t feature_count, uint64_t* out);
AGI_API uint32_t agi_model_release(uint64_t handle);
AGI_API uint32_t agi_linear_model_predict(uint64_t model, const double* features, size_t len, double* out);
AGI_API uint32_t agi_linear_model_train_batch(uint64_t model, uint64_t features, const double* targets, size_t target_len, double learning_rate, double* out_loss);

#ifdef __cplusplus
}
#endif
#endif
