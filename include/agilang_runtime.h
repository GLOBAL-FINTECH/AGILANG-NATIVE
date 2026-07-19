#ifndef AGILANG_RUNTIME_H
#define AGILANG_RUNTIME_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef _WIN32
  #ifdef AGILANG_RUNTIME_BUILD
    #define AGI_API __declspec(dllexport)
  #else
    #define AGI_API __declspec(dllimport)
  #endif
#else
  #define AGI_API __attribute__((visibility("default")))
#endif

#ifdef __cplusplus
extern "C" {
#endif

typedef uint64_t AgiHandle;

typedef enum AgiStatus {
    AGI_OK = 0,
    AGI_INVALID_ARGUMENT = 1,
    AGI_INVALID_HANDLE = 2,
    AGI_TYPE_MISMATCH = 3,
    AGI_OUT_OF_BOUNDS = 4,
    AGI_UTF8_ERROR = 5,
    AGI_IO_ERROR = 6,
    AGI_SERIALIZATION_ERROR = 7,
    AGI_TIMEOUT = 8,
    AGI_CANCELLED = 9,
    AGI_PERMISSION_DENIED = 10,
    AGI_NOT_FOUND = 11,
    AGI_ALREADY_EXISTS = 12,
    AGI_UNSUPPORTED = 13,
    AGI_OVERFLOW = 14,
    AGI_INVALID_STATE = 15,
    AGI_PANIC = 16,
    AGI_INTERNAL_ERROR = 255
} AgiStatus;

typedef enum AgiType {
    AGI_TYPE_INVALID = 0,
    AGI_TYPE_NULL = 1,
    AGI_TYPE_BOOL = 2,
    AGI_TYPE_INT = 3,
    AGI_TYPE_UINT = 4,
    AGI_TYPE_FLOAT = 5,
    AGI_TYPE_STRING = 6,
    AGI_TYPE_BYTES = 7,
    AGI_TYPE_ARRAY = 8,
    AGI_TYPE_MAP = 9,
    AGI_TYPE_ERROR = 10
} AgiType;

typedef struct AgiBuffer {
    uint8_t *data;
    size_t len;
    size_t capacity;
} AgiBuffer;

AGI_API uint32_t agi_runtime_abi_version(void);
AGI_API const char *agi_runtime_identity(void);
AGI_API size_t agi_runtime_live_handles(void);
AGI_API void agi_runtime_reset(void);
AGI_API uint32_t agi_runtime_last_error_code(void);
AGI_API uint32_t agi_runtime_last_error_json(AgiBuffer *out);

AGI_API AgiHandle agi_value_null(void);
AGI_API AgiHandle agi_value_bool(bool value);
AGI_API AgiHandle agi_value_int(int64_t value);
AGI_API AgiHandle agi_value_uint(uint64_t value);
AGI_API AgiHandle agi_value_float(double value);
AGI_API AgiHandle agi_value_array(void);
AGI_API AgiHandle agi_value_map(void);
AGI_API uint32_t agi_value_string(const uint8_t *data, size_t len, AgiHandle *out);
AGI_API uint32_t agi_value_bytes(const uint8_t *data, size_t len, AgiHandle *out);
AGI_API uint32_t agi_value_from_json(const uint8_t *data, size_t len, AgiHandle *out);
AGI_API uint32_t agi_value_kind(AgiHandle handle);
AGI_API uint32_t agi_value_read_bool(AgiHandle handle, bool *out);
AGI_API uint32_t agi_value_read_int(AgiHandle handle, int64_t *out);
AGI_API uint32_t agi_value_read_uint(AgiHandle handle, uint64_t *out);
AGI_API uint32_t agi_value_read_float(AgiHandle handle, double *out);
AGI_API uint32_t agi_value_read_string(AgiHandle handle, AgiBuffer *out);
AGI_API uint32_t agi_value_read_bytes(AgiHandle handle, AgiBuffer *out);
AGI_API uint32_t agi_value_len(AgiHandle handle, size_t *out);
AGI_API uint32_t agi_value_is_empty(AgiHandle handle, bool *out);
AGI_API uint32_t agi_value_equal(AgiHandle left, AgiHandle right, bool *out);
AGI_API uint32_t agi_value_to_json(AgiHandle handle, AgiBuffer *out);

AGI_API uint32_t agi_array_push(AgiHandle array, AgiHandle value, AgiHandle *out);
AGI_API uint32_t agi_array_get(AgiHandle array, size_t index, AgiHandle *out);
AGI_API uint32_t agi_map_insert(AgiHandle map, const uint8_t *key, size_t key_len, AgiHandle value, AgiHandle *out);
AGI_API uint32_t agi_map_get(AgiHandle map, const uint8_t *key, size_t key_len, AgiHandle *out);

AGI_API uint32_t agi_platform_info_json(AgiBuffer *out);
AGI_API void agi_buffer_free(AgiBuffer *buffer);
AGI_API AgiHandle agi_handle_clone(AgiHandle handle);
AGI_API uint32_t agi_handle_release(AgiHandle handle);

#ifdef __cplusplus
}
#endif
#endif
