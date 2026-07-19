#include "agilang_runtime.h"
#include <stdio.h>
#include <string.h>

static int require_ok(uint32_t status, const char *operation) {
    if (status == AGI_OK) return 1;
    AgiBuffer error = {0};
    agi_runtime_last_error_json(&error);
    fprintf(stderr, "%s failed (%u): %.*s\n", operation, status, (int)error.len, error.data ? (char *)error.data : "");
    agi_buffer_free(&error);
    return 0;
}

int main(void) {
    AgiHandle map = agi_value_map();
    AgiHandle value = agi_value_int(1990);
    AgiHandle updated = 0;
    const char *key = "chain_id";
    if (!require_ok(agi_map_insert(map, (const uint8_t *)key, strlen(key), value, &updated), "map insert")) return 1;

    AgiBuffer json = {0};
    if (!require_ok(agi_value_to_json(updated, &json), "JSON conversion")) return 1;
    printf("%.*s\n", (int)json.len, (char *)json.data);
    agi_buffer_free(&json);

    agi_handle_release(updated);
    agi_handle_release(value);
    agi_handle_release(map);
    return agi_runtime_live_handles() == 0 ? 0 : 2;
}
