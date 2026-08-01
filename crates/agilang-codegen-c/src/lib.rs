use agilang_ir::{HirBinaryOp, HirExpr, HirProgram, HirStmt};
use agilang_types::Type;

pub fn generate(program: &HirProgram) -> String {
    let mut out = String::new();
    out.push_str("#include <stdint.h>\n");
    out.push_str("#include <stdio.h>\n");
    out.push_str("#include <stdbool.h>\n\n");
    out.push_str("#include <math.h>\n\n");
    out.push_str("#include <string.h>\n\n");
    out.push_str("#include <stdlib.h>\n\n");

    out.push_str("typedef struct {\n");
    out.push_str("    double* data;\n");
    out.push_str("    size_t length;\n");
    out.push_str("    size_t capacity;\n");
    out.push_str("    bool owns_data;\n");
    out.push_str("} agi_list_f64;\n\n");

    out.push_str("typedef struct {\n");
    out.push_str("    agi_list_f64* data;\n");
    out.push_str("    size_t length;\n");
    out.push_str("    size_t capacity;\n");
    out.push_str("    bool owns_data;\n");
    out.push_str("} agi_list_list_f64;\n\n");

    // Declare the runtime print function
    out.push_str("// Linked AGILANG runtime ABI\n");
    out.push_str("extern void agi_print(const char* msg);\n");
    out.push_str("extern const char* agi_http_get(const char* url);\n");
    out.push_str("extern const char* agi_http_post(const char* url, const char* body);\n");
    out.push_str("extern const char* agi_http_get_json(const char* url);\n");
    out.push_str("extern const char* agi_http_post_json(const char* url, const char* body);\n");
    out.push_str("extern const char* agi_http_request_json(const char* request_json);\n\n");
    out.push_str("static char* agi_json_escape_and_quote(const char* input) {\n");
    out.push_str("    if (!input) input = \"\";\n");
    out.push_str("    size_t len = 2;\n");
    out.push_str("    for (const unsigned char* p = (const unsigned char*)input; *p; ++p) {\n");
    out.push_str("        switch (*p) {\n");
    out.push_str("            case '\\\\': case '\"': case '\\n': case '\\r': case '\\t': len += 2; break;\n");
    out.push_str("            default: len += 1; break;\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("    char* out = (char*)malloc(len + 1);\n");
    out.push_str("    if (!out) return NULL;\n");
    out.push_str("    size_t i = 0;\n");
    out.push_str("    out[i++] = '\"';\n");
    out.push_str("    for (const unsigned char* p = (const unsigned char*)input; *p; ++p) {\n");
    out.push_str("        switch (*p) {\n");
    out.push_str("            case '\\\\': out[i++]='\\\\'; out[i++]='\\\\'; break;\n");
    out.push_str("            case '\"': out[i++]='\\\\'; out[i++]='\"'; break;\n");
    out.push_str("            case '\\n': out[i++]='\\\\'; out[i++]='n'; break;\n");
    out.push_str("            case '\\r': out[i++]='\\\\'; out[i++]='r'; break;\n");
    out.push_str("            case '\\t': out[i++]='\\\\'; out[i++]='t'; break;\n");
    out.push_str("            default: out[i++]=(char)(*p); break;\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("    out[i++] = '\"';\n");
    out.push_str("    out[i] = '\\0';\n");
    out.push_str("    return out;\n");
    out.push_str("}\n\n");
    out.push_str("static char* agi_json_object(const char** keys, const char** values, size_t count) {\n");
    out.push_str("    size_t len = 2;\n");
    out.push_str("    for (size_t i = 0; i < count; ++i) {\n");
    out.push_str("        len += strlen(keys[i]) + strlen(values[i]) + 4;\n");
    out.push_str("        if (i + 1 < count) len += 1;\n");
    out.push_str("    }\n");
    out.push_str("    char* out = (char*)malloc(len + 1);\n");
    out.push_str("    if (!out) return NULL;\n");
    out.push_str("    size_t pos = 0;\n");
    out.push_str("    out[pos++] = '{';\n");
    out.push_str("    for (size_t i = 0; i < count; ++i) {\n");
    out.push_str("        out[pos++] = '\"';\n");
    out.push_str("        size_t key_len = strlen(keys[i]);\n");
    out.push_str("        memcpy(out + pos, keys[i], key_len);\n");
    out.push_str("        pos += key_len;\n");
    out.push_str("        out[pos++] = '\"';\n");
    out.push_str("        out[pos++] = ':';\n");
    out.push_str("        size_t value_len = strlen(values[i]);\n");
    out.push_str("        memcpy(out + pos, values[i], value_len);\n");
    out.push_str("        pos += value_len;\n");
    out.push_str("        if (i + 1 < count) out[pos++] = ',';\n");
    out.push_str("    }\n");
    out.push_str("    out[pos++] = '}';\n");
    out.push_str("    out[pos] = '\\0';\n");
    out.push_str("    return out;\n");
    out.push_str("}\n\n");
    out.push_str("static char* agi_json_array(const char** values, size_t count) {\n");
    out.push_str("    size_t len = 2;\n");
    out.push_str("    for (size_t i = 0; i < count; ++i) {\n");
    out.push_str("        len += strlen(values[i]);\n");
    out.push_str("        if (i + 1 < count) len += 1;\n");
    out.push_str("    }\n");
    out.push_str("    char* out = (char*)malloc(len + 1);\n");
    out.push_str("    if (!out) return NULL;\n");
    out.push_str("    size_t pos = 0;\n");
    out.push_str("    out[pos++] = '[';\n");
    out.push_str("    for (size_t i = 0; i < count; ++i) {\n");
    out.push_str("        size_t value_len = strlen(values[i]);\n");
    out.push_str("        memcpy(out + pos, values[i], value_len);\n");
    out.push_str("        pos += value_len;\n");
    out.push_str("        if (i + 1 < count) out[pos++] = ',';\n");
    out.push_str("    }\n");
    out.push_str("    out[pos++] = ']';\n");
    out.push_str("    out[pos] = '\\0';\n");
    out.push_str("    return out;\n");
    out.push_str("}\n\n");
    out.push_str("static char* agi_json_i64(int64_t value) {\n");
    out.push_str("    char buffer[32];\n");
    out.push_str("    snprintf(buffer, sizeof(buffer), \"%lld\", (long long)value);\n");
    out.push_str("    size_t len = strlen(buffer);\n");
    out.push_str("    char* out = (char*)malloc(len + 1);\n");
    out.push_str("    if (!out) return NULL;\n");
    out.push_str("    memcpy(out, buffer, len + 1);\n");
    out.push_str("    return out;\n");
    out.push_str("}\n\n");
    out.push_str("static char* agi_json_f64(double value) {\n");
    out.push_str("    char buffer[64];\n");
    out.push_str("    snprintf(buffer, sizeof(buffer), \"%.17g\", value);\n");
    out.push_str("    size_t len = strlen(buffer);\n");
    out.push_str("    char* out = (char*)malloc(len + 1);\n");
    out.push_str("    if (!out) return NULL;\n");
    out.push_str("    memcpy(out, buffer, len + 1);\n");
    out.push_str("    return out;\n");
    out.push_str("}\n\n");
    out.push_str("static void agi_print_i64(int64_t value) {\n");
    out.push_str("    char buffer[32];\n");
    out.push_str("    snprintf(buffer, sizeof(buffer), \"%lld\", (long long)value);\n");
    out.push_str("    agi_print(buffer);\n");
    out.push_str("}\n\n");
    out.push_str("static void agi_print_u64(uint64_t value) {\n");
    out.push_str("    char buffer[32];\n");
    out.push_str("    snprintf(buffer, sizeof(buffer), \"%llu\", (unsigned long long)value);\n");
    out.push_str("    agi_print(buffer);\n");
    out.push_str("}\n\n");
    out.push_str("static void agi_print_f64(double value) {\n");
    out.push_str("    char buffer[32];\n");
    out.push_str("    snprintf(buffer, sizeof(buffer), \"%.17g\", value);\n");
    out.push_str("    agi_print(buffer);\n");
    out.push_str("}\n\n");
    out.push_str("static void agi_print_bool(bool value) {\n");
    out.push_str("    agi_print(value ? \"true\" : \"false\");\n");
    out.push_str("}\n\n");

    out.push_str("static int agi_cmp_f64(const void* a, const void* b) {\n");
    out.push_str("    double da = *(const double*)a;\n");
    out.push_str("    double db = *(const double*)b;\n");
    out.push_str("    return (da > db) - (da < db);\n");
    out.push_str("}\n\n");

    out.push_str("static agi_list_f64 agi_list_from_array_f64(const double* xs, size_t n) {\n");
    out.push_str("    agi_list_f64 out = {0};\n");
    out.push_str("    if (n == 0) return out;\n");
    out.push_str("    double* data = (double*)malloc(sizeof(double) * n);\n");
    out.push_str("    if (!data) return out;\n");
    out.push_str("    for (size_t i = 0; i < n; ++i) data[i] = xs[i];\n");
    out.push_str("    out.data = data;\n");
    out.push_str("    out.length = n;\n");
    out.push_str("    out.capacity = n;\n");
    out.push_str("    out.owns_data = true;\n");
    out.push_str("    return out;\n");
    out.push_str("}\n\n");

    out.push_str("static agi_list_f64 agi_list_clone_f64(const agi_list_f64* xs) {\n");
    out.push_str("    if (!xs || xs->length == 0 || !xs->data) {\n");
    out.push_str("        agi_list_f64 empty = {0};\n");
    out.push_str("        return empty;\n");
    out.push_str("    }\n");
    out.push_str("    return agi_list_from_array_f64(xs->data, xs->length);\n");
    out.push_str("}\n\n");

    out.push_str("static void agi_list_free_f64(agi_list_f64* xs) {\n");
    out.push_str("    if (!xs) return;\n");
    out.push_str("    if (xs->owns_data && xs->data) free(xs->data);\n");
    out.push_str("    xs->data = NULL; xs->length = 0; xs->capacity = 0; xs->owns_data = false;\n");
    out.push_str("}\n\n");

    out.push_str("static bool agi_list_reserve_f64(agi_list_f64* xs, size_t requested) {\n");
    out.push_str("    if (!xs) return false;\n");
    out.push_str("    if (requested <= xs->capacity && xs->owns_data) return true;\n");
    out.push_str("    size_t capacity = requested > xs->length ? requested : xs->length;\n");
    out.push_str("    double* data = (double*)malloc(sizeof(double) * capacity);\n");
    out.push_str("    if (!data && capacity > 0) return false;\n");
    out.push_str("    for (size_t i = 0; i < xs->length; ++i) data[i] = xs->data[i];\n");
    out.push_str("    if (xs->owns_data && xs->data) free(xs->data);\n");
    out.push_str("    xs->data = data; xs->capacity = capacity; xs->owns_data = true;\n");
    out.push_str("    return true;\n");
    out.push_str("}\n\n");

    out.push_str("static bool agi_list_pop_f64(agi_list_f64* xs, double* out_value) {\n");
    out.push_str("    if (!xs || xs->length == 0) return false;\n");
    out.push_str("    if (out_value) *out_value = xs->data[xs->length - 1];\n");
    out.push_str("    xs->length -= 1; return true;\n");
    out.push_str("}\n\n");

    out.push_str(
        "static bool agi_list_insert_f64(agi_list_f64* xs, size_t index, double value) {\n",
    );
    out.push_str("    if (!xs || index > xs->length) return false;\n");
    out.push_str("    if (xs->length == xs->capacity || !xs->owns_data) {\n");
    out.push_str("        size_t next = xs->capacity > 0 ? xs->capacity * 2 : 4;\n");
    out.push_str("        if (next < xs->length + 1) next = xs->length + 1;\n");
    out.push_str("        if (!agi_list_reserve_f64(xs, next)) return false;\n");
    out.push_str("    }\n");
    out.push_str(
        "    for (size_t i = xs->length; i > index; --i) xs->data[i] = xs->data[i - 1];\n",
    );
    out.push_str("    xs->data[index] = value; xs->length += 1; return true;\n");
    out.push_str("}\n\n");

    out.push_str(
        "static bool agi_list_remove_f64(agi_list_f64* xs, size_t index, double* out_value) {\n",
    );
    out.push_str("    if (!xs || index >= xs->length) return false;\n");
    out.push_str("    if (out_value) *out_value = xs->data[index];\n");
    out.push_str(
        "    for (size_t i = index + 1; i < xs->length; ++i) xs->data[i - 1] = xs->data[i];\n",
    );
    out.push_str("    xs->length -= 1; return true;\n");
    out.push_str("}\n\n");

    out.push_str("static bool agi_list_set_f64(agi_list_f64* xs, int64_t idx, double v) {\n");
    out.push_str("    if (!xs || !xs->data) return false;\n");
    out.push_str("    if (idx < 0 || (size_t)idx >= xs->length) return false;\n");
    out.push_str("    xs->data[idx] = v;\n");
    out.push_str("    return true;\n");
    out.push_str("}\n\n");

    out.push_str("static bool agi_list_push_f64(agi_list_f64* xs, double v) {\n");
    out.push_str("    if (!xs) return false;\n");
    out.push_str("    if (!xs->owns_data) {\n");
    out.push_str("        size_t new_cap = xs->length > 0 ? (xs->length * 2) : 4;\n");
    out.push_str("        double* data = (double*)malloc(sizeof(double) * new_cap);\n");
    out.push_str("        if (!data) return false;\n");
    out.push_str("        for (size_t i = 0; i < xs->length; ++i) data[i] = xs->data[i];\n");
    out.push_str("        xs->data = data;\n");
    out.push_str("        xs->capacity = new_cap;\n");
    out.push_str("        xs->owns_data = true;\n");
    out.push_str("    } else if (xs->length >= xs->capacity) {\n");
    out.push_str("        size_t new_cap = xs->capacity > 0 ? (xs->capacity * 2) : 4;\n");
    out.push_str("        double* data = (double*)realloc(xs->data, sizeof(double) * new_cap);\n");
    out.push_str("        if (!data) return false;\n");
    out.push_str("        xs->data = data;\n");
    out.push_str("        xs->capacity = new_cap;\n");
    out.push_str("    }\n");
    out.push_str("    xs->data[xs->length++] = v;\n");
    out.push_str("    return true;\n");
    out.push_str("}\n\n");

    out.push_str("static agi_list_list_f64 agi_list_from_array_list_f64(const agi_list_f64* xs, size_t n) {\n");
    out.push_str("    agi_list_list_f64 out = {0};\n");
    out.push_str("    if (n == 0) return out;\n");
    out.push_str("    agi_list_f64* data = (agi_list_f64*)malloc(sizeof(agi_list_f64) * n);\n");
    out.push_str("    if (!data) return out;\n");
    out.push_str("    for (size_t i = 0; i < n; ++i) data[i] = agi_list_clone_f64(&xs[i]);\n");
    out.push_str("    out.data = data;\n");
    out.push_str("    out.length = n;\n");
    out.push_str("    out.capacity = n;\n");
    out.push_str("    out.owns_data = true;\n");
    out.push_str("    return out;\n");
    out.push_str("}\n\n");

    out.push_str("static void agi_list_free_list_f64(agi_list_list_f64* xs) {\n");
    out.push_str("    if (!xs) return;\n");
    out.push_str("    if (xs->owns_data && xs->data) {\n");
    out.push_str(
        "        for (size_t i = 0; i < xs->length; ++i) agi_list_free_f64(&xs->data[i]);\n",
    );
    out.push_str("        free(xs->data);\n");
    out.push_str("    }\n");
    out.push_str("    xs->data = NULL; xs->length = 0; xs->capacity = 0; xs->owns_data = false;\n");
    out.push_str("}\n\n");

    out.push_str(
        "static agi_list_f64 agi_list_get_list_f64(const agi_list_list_f64* xs, int64_t idx) {\n",
    );
    out.push_str("    agi_list_f64 empty = {0};\n");
    out.push_str("    if (!xs || !xs->data) return empty;\n");
    out.push_str("    if (idx < 0 || (size_t)idx >= xs->length) return empty;\n");
    out.push_str("    return xs->data[idx];\n");
    out.push_str("}\n\n");

    out.push_str("static double agi_list_get_f64(const agi_list_f64* xs, int64_t idx) {\n");
    out.push_str("    if (!xs || !xs->data) return NAN;\n");
    out.push_str("    if (idx < 0 || (size_t)idx >= xs->length) return NAN;\n");
    out.push_str("    return xs->data[idx];\n");
    out.push_str("}\n\n");

    out.push_str("static double agi_stats_sum(const double* xs, size_t n) {\n");
    out.push_str("    double acc = 0.0;\n");
    out.push_str("    for (size_t i = 0; i < n; ++i) acc += xs[i];\n");
    out.push_str("    return acc;\n");
    out.push_str("}\n\n");

    out.push_str("static double agi_stats_product(const double* xs, size_t n) {\n");
    out.push_str("    double acc = 1.0;\n");
    out.push_str("    for (size_t i = 0; i < n; ++i) acc *= xs[i];\n");
    out.push_str("    return acc;\n");
    out.push_str("}\n\n");

    out.push_str("static double agi_stats_mean(const double* xs, size_t n) {\n");
    out.push_str("    return n == 0 ? NAN : (agi_stats_sum(xs, n) / (double)n);\n");
    out.push_str("}\n\n");

    out.push_str("static double agi_stats_variance(const double* xs, size_t n) {\n");
    out.push_str("    if (n == 0) return NAN;\n");
    out.push_str("    double m = agi_stats_mean(xs, n);\n");
    out.push_str("    double acc = 0.0;\n");
    out.push_str("    for (size_t i = 0; i < n; ++i) {\n");
    out.push_str("        double d = xs[i] - m;\n");
    out.push_str("        acc += d * d;\n");
    out.push_str("    }\n");
    out.push_str("    return acc / (double)n;\n");
    out.push_str("}\n\n");

    out.push_str("static double agi_stats_stddev(const double* xs, size_t n) {\n");
    out.push_str("    return sqrt(agi_stats_variance(xs, n));\n");
    out.push_str("}\n\n");

    out.push_str("static double agi_stats_median(const double* xs, size_t n) {\n");
    out.push_str("    if (n == 0) return NAN;\n");
    out.push_str("    double* copy = (double*)malloc(sizeof(double) * n);\n");
    out.push_str("    if (!copy) return NAN;\n");
    out.push_str("    for (size_t i = 0; i < n; ++i) copy[i] = xs[i];\n");
    out.push_str("    qsort(copy, n, sizeof(double), agi_cmp_f64);\n");
    out.push_str(
        "    double out = (n % 2 == 1) ? copy[n / 2] : (copy[n / 2 - 1] + copy[n / 2]) / 2.0;\n",
    );
    out.push_str("    free(copy);\n");
    out.push_str("    return out;\n");
    out.push_str("}\n\n");

    out.push_str("static double agi_stats_quantile(const double* xs, size_t n, double q) {\n");
    out.push_str("    if (n == 0) return NAN;\n");
    out.push_str("    if (q < 0.0) q = 0.0;\n");
    out.push_str("    if (q > 1.0) q = 1.0;\n");
    out.push_str("    double* copy = (double*)malloc(sizeof(double) * n);\n");
    out.push_str("    if (!copy) return NAN;\n");
    out.push_str("    for (size_t i = 0; i < n; ++i) copy[i] = xs[i];\n");
    out.push_str("    qsort(copy, n, sizeof(double), agi_cmp_f64);\n");
    out.push_str("    double pos = q * (double)(n - 1);\n");
    out.push_str("    size_t lo = (size_t)floor(pos);\n");
    out.push_str("    size_t hi = (size_t)ceil(pos);\n");
    out.push_str("    double t = pos - (double)lo;\n");
    out.push_str("    double out = copy[lo] + (copy[hi] - copy[lo]) * t;\n");
    out.push_str("    free(copy);\n");
    out.push_str("    return out;\n");
    out.push_str("}\n\n");

    out.push_str("static double agi_stats_percentile(const double* xs, size_t n, double p) {\n");
    out.push_str("    return agi_stats_quantile(xs, n, p / 100.0);\n");
    out.push_str("}\n\n");

    out.push_str("static double agi_stats_mode_or_nan(const double* xs, size_t n) {\n");
    out.push_str("    if (n == 0) return NAN;\n");
    out.push_str("    double* copy = (double*)malloc(sizeof(double) * n);\n");
    out.push_str("    if (!copy) return NAN;\n");
    out.push_str("    for (size_t i = 0; i < n; ++i) copy[i] = xs[i];\n");
    out.push_str("    qsort(copy, n, sizeof(double), agi_cmp_f64);\n");
    out.push_str("    size_t best_count = 1;\n");
    out.push_str("    size_t cur_count = 1;\n");
    out.push_str("    double best_value = copy[0];\n");
    out.push_str("    for (size_t i = 1; i < n; ++i) {\n");
    out.push_str("        if (copy[i] == copy[i - 1]) {\n");
    out.push_str("            cur_count += 1;\n");
    out.push_str("        } else {\n");
    out.push_str("            if (cur_count > best_count) {\n");
    out.push_str("                best_count = cur_count;\n");
    out.push_str("                best_value = copy[i - 1];\n");
    out.push_str("            }\n");
    out.push_str("            cur_count = 1;\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("    if (cur_count > best_count) {\n");
    out.push_str("        best_count = cur_count;\n");
    out.push_str("        best_value = copy[n - 1];\n");
    out.push_str("    }\n");
    out.push_str("    free(copy);\n");
    out.push_str("    return best_count > 1 ? best_value : NAN;\n");
    out.push_str("}\n\n");

    // Forward declarations of custom functions
    for func in &program.functions {
        out.push_str(&format!("{} {}_agi(", c_type(&func.return_type), func.name));
        for (i, param) in func.params.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!("{} {}", c_type(&param.ty), param.name));
        }
        out.push_str(");\n");
    }
    out.push('\n');

    // Function definitions
    for func in &program.functions {
        out.push_str(&format!("{} {}_agi(", c_type(&func.return_type), func.name));
        for (i, param) in func.params.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!("{} {}", c_type(&param.ty), param.name));
        }
        out.push_str(") {\n");

        for stmt in &func.body {
            out.push_str(&generate_stmt(stmt, 4));
        }

        out.push_str("}\n\n");
    }

    let main_return_type = program
        .functions
        .iter()
        .find(|func| func.name == "main")
        .map(|func| &func.return_type);

    // Generate C main standard entry point
    out.push_str("// Standard C Main Entry Point\n");
    out.push_str("int32_t agilang_main(void) {\n");
    match main_return_type {
        Some(Type::Void) | None => {
            out.push_str("    main_agi();\n");
            out.push_str("    return 0;\n");
        }
        _ => {
            out.push_str("    return (int32_t)main_agi();\n");
        }
    }
    out.push_str("}\n\n");
    out.push_str("int main(void) {\n");
    out.push_str("    return (int)agilang_main();\n");
    out.push_str("}\n");

    out
}

fn c_type(ty: &Type) -> &'static str {
    match ty {
        Type::I32 => "int32_t",
        Type::I64 => "int64_t",
        Type::U32 => "uint32_t",
        Type::U64 => "uint64_t",
        Type::F32 => "float",
        Type::F64 => "double",
        Type::Bool => "bool",
        Type::String => "const char*",
        Type::List(inner) if inner.is_numeric() || inner.as_ref() == &Type::Unknown => {
            "agi_list_f64"
        }
        Type::List(inner) if matches!(inner.as_ref(), Type::List(n) if n.is_numeric() || n.as_ref() == &Type::Unknown) => {
            "agi_list_list_f64"
        }
        Type::Optional(inner) => c_type(inner),
        Type::Void => "void",
        _ => "void*",
    }
}

fn generate_stmt(stmt: &HirStmt, indent: usize) -> String {
    let ind = " ".repeat(indent);
    match stmt {
        HirStmt::Let {
            name, ty, value, ..
        } => {
            if let Type::List(inner) = ty {
                if inner.is_numeric() || inner.as_ref() == &Type::Unknown {
                    if let Some(items) = list_literal_numeric_elements(value) {
                        let data_name = format!("{}_data", name);
                        return format!(
                            "{}double {}[] = {{{}}};\n{}agi_list_f64 {} = {{ {}, {}, {}, false }};\n",
                            ind,
                            data_name,
                            items.join(", "),
                            ind,
                            name,
                            data_name,
                            items.len(),
                            items.len()
                        );
                    }

                    return format!("{}agi_list_f64 {} = {};\n", ind, name, generate_expr(value));
                }

                if matches!(inner.as_ref(), Type::List(n) if n.is_numeric() || n.as_ref() == &Type::Unknown)
                {
                    if let Some(rows) = list_literal_nested_numeric_elements(value) {
                        let mut out = String::new();
                        for (row_idx, row) in rows.iter().enumerate() {
                            let row_data_name = format!("{}_row_{}_data", name, row_idx);
                            let row_name = format!("{}_row_{}", name, row_idx);
                            out.push_str(&format!(
                                "{}double {}[] = {{{}}};\n",
                                ind,
                                row_data_name,
                                row.join(", ")
                            ));
                            out.push_str(&format!(
                                "{}agi_list_f64 {} = {{ {}, {}, {}, false }};\n",
                                ind,
                                row_name,
                                row_data_name,
                                row.len(),
                                row.len()
                            ));
                        }
                        let row_refs: Vec<String> = (0..rows.len())
                            .map(|idx| format!("{}_row_{}", name, idx))
                            .collect();
                        let top_data_name = format!("{}_data", name);
                        out.push_str(&format!(
                            "{}agi_list_f64 {}[] = {{{}}};\n",
                            ind,
                            top_data_name,
                            row_refs.join(", ")
                        ));
                        out.push_str(&format!(
                            "{}agi_list_list_f64 {} = {{ {}, {}, {}, false }};\n",
                            ind,
                            name,
                            top_data_name,
                            rows.len(),
                            rows.len()
                        ));
                        return out;
                    }
                }
            }

            format!(
                "{}{} {} = {};\n",
                ind,
                c_type(ty),
                name,
                generate_expr(value)
            )
        }
        HirStmt::Return { value, .. } => {
            if let Some(val) = value {
                if matches!(val.ty(), Type::List(inner) if inner.is_numeric() || inner.as_ref() == &Type::Unknown)
                {
                    return format!(
                        "{}return agi_list_clone_f64(&{});\n",
                        ind,
                        generate_expr(val)
                    );
                }
                format!("{}return {};\n", ind, generate_expr(val))
            } else {
                format!("{}return;\n", ind)
            }
        }
        HirStmt::Break { .. } => format!("{}break;\n", ind),
        HirStmt::Continue { .. } => format!("{}continue;\n", ind),
        HirStmt::Assign { target, value, .. } => match target {
            HirExpr::Identifier(name, _, _) => {
                format!("{}{} = {};\n", ind, name, generate_expr(value))
            }
            HirExpr::Index { object, index, .. } if matches!(object.ty(), Type::List(inner) if inner.is_numeric() || inner.as_ref() == &Type::Unknown) =>
            {
                format!(
                    "{}agi_list_set_f64(&{}, (int64_t)({}), (double)({}));\n",
                    ind,
                    generate_expr(object),
                    generate_expr(index),
                    generate_expr(value)
                )
            }
            _ => format!("{}/* unsupported assignment target */;\n", ind),
        },
        HirStmt::Expr(expr) => {
            format!("{}{};\n", ind, generate_expr(expr))
        }
        HirStmt::If {
            condition,
            then_body,
            else_body,
            ..
        } => {
            let mut out = format!("{}if ({}) {{\n", ind, generate_expr(condition));
            for stmt in then_body {
                out.push_str(&generate_stmt(stmt, indent + 4));
            }
            out.push_str(&format!("{}}}", ind));
            if !else_body.is_empty() {
                out.push_str(" else {\n");
                for stmt in else_body {
                    out.push_str(&generate_stmt(stmt, indent + 4));
                }
                out.push_str(&format!("{}}}", ind));
            }
            out.push('\n');
            out
        }
        HirStmt::While {
            condition, body, ..
        } => {
            let mut out = format!("{}while ({}) {{\n", ind, generate_expr(condition));
            for stmt in body {
                out.push_str(&generate_stmt(stmt, indent + 4));
            }
            out.push_str(&format!("{}}}\n", ind));
            out
        }
        HirStmt::ForIn { .. } => format!("{}/* unsupported for-in loop */;\n", ind),
    }
}

fn generate_expr(expr: &HirExpr) -> String {
    match expr {
        HirExpr::Identifier(name, _, _) => name.clone(),
        HirExpr::Integer(val, _, _) => val.to_string(),
        HirExpr::Float(val, _, _) => val.to_string(),
        HirExpr::String(val, _, _) => format!("\"{}\"", val.replace("\"", "\\\"")),
        HirExpr::Bool(val, _, _) => val.to_string(),
        HirExpr::ListLiteral(_, ty, _) => {
            if let Some(items) = list_literal_numeric_elements(expr) {
                return format!(
                    "agi_list_from_array_f64((double[]){{{}}}, {})",
                    items.join(", "),
                    items.len()
                );
            }
            if matches!(ty, Type::List(inner) if matches!(inner.as_ref(), Type::List(n) if n.is_numeric() || n.as_ref() == &Type::Unknown))
            {
                if let Some(rows) = list_literal_nested_numeric_elements(expr) {
                    let row_exprs: Vec<String> = rows
                        .iter()
                        .map(|row| {
                            format!(
                                "agi_list_from_array_f64((double[]){{{}}}, {})",
                                row.join(", "),
                                row.len()
                            )
                        })
                        .collect();
                    return format!(
                        "agi_list_from_array_list_f64((agi_list_f64[]){{{}}}, {})",
                        row_exprs.join(", "),
                        row_exprs.len()
                    );
                }
            }
            "/* unsupported list literal */ 0".to_string()
        }
        HirExpr::ObjectLiteral(items, _, _) => generate_json_object_expr(items),
        HirExpr::MemberAccess { object, member, .. } => {
            format!("/* member access */ {}.{}", generate_expr(object), member)
        }
        HirExpr::Index { object, index, .. } => {
            if matches!(object.ty(), Type::List(inner) if inner.is_numeric() || inner.as_ref() == &Type::Unknown)
            {
                format!(
                    "agi_list_get_f64(&{}, (int64_t)({}))",
                    generate_expr(object),
                    generate_expr(index)
                )
            } else if matches!(object.ty(), Type::List(inner) if matches!(inner.as_ref(), Type::List(n) if n.is_numeric() || n.as_ref() == &Type::Unknown))
            {
                format!(
                    "agi_list_get_list_f64(&{}, (int64_t)({}))",
                    generate_expr(object),
                    generate_expr(index)
                )
            } else {
                format!(
                    "/* index */ {}[{}]",
                    generate_expr(object),
                    generate_expr(index)
                )
            }
        }
        HirExpr::Call { callee, args, .. } => {
            let callee_name = match &**callee {
                HirExpr::Identifier(name, _, _) => name.clone(),
                _ => "unknown".to_string(),
            };
            if callee_name == "print" {
                if args.is_empty() {
                    "/* print() with no arguments */".to_string()
                } else {
                    let mut out = String::new();
                    for arg in args {
                        let value = generate_expr(arg);
                        let call = match arg.ty() {
                            Type::I32 | Type::I64 => format!("agi_print_i64((int64_t)({value}))"),
                            Type::U32 | Type::U64 => format!("agi_print_u64((uint64_t)({value}))"),
                            Type::F32 | Type::F64 => format!("agi_print_f64((double)({value}))"),
                            Type::Bool => format!("agi_print_bool((bool)({value}))"),
                            _ => format!("agi_print({value})"),
                        };
                        out.push_str(&call);
                        out.push_str("; ");
                    }
                    out.trim_end().to_string()
                }
            } else if let Some(intrinsic_expr) = generate_intrinsic_call(&callee_name, args) {
                intrinsic_expr
            } else {
                let mut arg_strs = vec![];
                for arg in args {
                    arg_strs.push(generate_expr(arg));
                }
                format!("{}_agi({})", callee_name, arg_strs.join(", "))
            }
        }
        HirExpr::Binary {
            left, op, right, ..
        } => {
            let op_str = match op {
                HirBinaryOp::Add => "+",
                HirBinaryOp::Subtract => "-",
                HirBinaryOp::Multiply => "*",
                HirBinaryOp::Divide => "/",
                HirBinaryOp::Equal => "==",
                HirBinaryOp::NotEqual => "!=",
                HirBinaryOp::Less => "<",
                HirBinaryOp::LessEqual => "<=",
                HirBinaryOp::Greater => ">",
                HirBinaryOp::GreaterEqual => ">=",
                HirBinaryOp::And => "&&",
                HirBinaryOp::Or => "||",
            };
            format!(
                "({} {} {})",
                generate_expr(left),
                op_str,
                generate_expr(right)
            )
        }
    }
}

fn generate_intrinsic_call(name: &str, args: &[HirExpr]) -> Option<String> {
    let arg_strs: Vec<String> = args.iter().map(generate_expr).collect();
    if name == "get_json" && arg_strs.len() == 2 {
        return Some(format!("agi_http_get_json({})", arg_strs[1]));
    }
    if name == "get" && arg_strs.len() == 2 {
        return Some(format!("agi_http_get({})", arg_strs[1]));
    }
    if name == "post_json" && arg_strs.len() == 3 {
        return Some(format!(
            "agi_http_post_json({}, {})",
            arg_strs[1], arg_strs[2]
        ));
    }
    if name == "post" && arg_strs.len() == 3 {
        return Some(format!("agi_http_post({}, {})", arg_strs[1], arg_strs[2]));
    }
    if name == "request" && arg_strs.len() == 2 {
        return Some(format!("agi_http_request_json({})", arg_strs[1]));
    }

    let list_elems = args.first().and_then(list_literal_numeric_elements);

    if let Some(elems) = &list_elems {
        let array = format!("(double[]){{{}}}", elems.join(", "));
        let n = elems.len();
        match name {
            "sum" => return Some(format!("agi_stats_sum({}, {})", array, n)),
            "product" => return Some(format!("agi_stats_product({}, {})", array, n)),
            "mean" => return Some(format!("agi_stats_mean({}, {})", array, n)),
            "variance" => return Some(format!("agi_stats_variance({}, {})", array, n)),
            "stddev" => return Some(format!("agi_stats_stddev({}, {})", array, n)),
            "median" => return Some(format!("agi_stats_median({}, {})", array, n)),
            "mode" => return Some(format!("agi_stats_mode_or_nan({}, {})", array, n)),
            "quantile" if arg_strs.len() == 2 => {
                return Some(format!(
                    "agi_stats_quantile({}, {}, {})",
                    array, n, arg_strs[1]
                ));
            }
            "percentile" if arg_strs.len() == 2 => {
                return Some(format!(
                    "agi_stats_percentile({}, {}, {})",
                    array, n, arg_strs[1]
                ));
            }
            _ => {}
        }
    }

    if let Some(first) = args.first() {
        if matches!(first.ty(), Type::List(inner) if inner.is_numeric() || inner.as_ref() == &Type::Unknown)
        {
            let list_expr = generate_expr(first);
            match name {
                "len" => return Some(format!("((int64_t)({}.length))", list_expr)),
                "append" if arg_strs.len() == 2 => {
                    return Some(format!(
                        "agi_list_push_f64(&{}, (double)({}))",
                        list_expr, arg_strs[1]
                    ));
                }
                "sum" => {
                    return Some(format!(
                        "agi_stats_sum({}.data, {}.length)",
                        list_expr, list_expr
                    ));
                }
                "product" => {
                    return Some(format!(
                        "agi_stats_product({}.data, {}.length)",
                        list_expr, list_expr
                    ));
                }
                "mean" => {
                    return Some(format!(
                        "agi_stats_mean({}.data, {}.length)",
                        list_expr, list_expr
                    ));
                }
                "variance" => {
                    return Some(format!(
                        "agi_stats_variance({}.data, {}.length)",
                        list_expr, list_expr
                    ));
                }
                "stddev" => {
                    return Some(format!(
                        "agi_stats_stddev({}.data, {}.length)",
                        list_expr, list_expr
                    ));
                }
                "median" => {
                    return Some(format!(
                        "agi_stats_median({}.data, {}.length)",
                        list_expr, list_expr
                    ));
                }
                "mode" => {
                    return Some(format!(
                        "agi_stats_mode_or_nan({}.data, {}.length)",
                        list_expr, list_expr
                    ));
                }
                "quantile" if arg_strs.len() == 2 => {
                    return Some(format!(
                        "agi_stats_quantile({}.data, {}.length, {})",
                        list_expr, list_expr, arg_strs[1]
                    ));
                }
                "percentile" if arg_strs.len() == 2 => {
                    return Some(format!(
                        "agi_stats_percentile({}.data, {}.length, {})",
                        list_expr, list_expr, arg_strs[1]
                    ));
                }
                _ => {}
            }
        }
    }

    match name {
        "abs" => Some(format!("fabs({})", arg_strs[0])),
        "sign" => Some(format!(
            "(({}) > 0 ? 1.0 : (({}) < 0 ? -1.0 : 0.0))",
            arg_strs[0], arg_strs[0]
        )),
        "sqrt" => Some(format!("sqrt({})", arg_strs[0])),
        "cbrt" => Some(format!("cbrt({})", arg_strs[0])),
        "pow" => Some(format!("pow({}, {})", arg_strs[0], arg_strs[1])),
        "hypot" => Some(format!("hypot({}, {})", arg_strs[0], arg_strs[1])),
        "exp" => Some(format!("exp({})", arg_strs[0])),
        "exp2" => Some(format!("exp2({})", arg_strs[0])),
        "ln" => Some(format!("log({})", arg_strs[0])),
        "log" => Some(format!("(log({}) / log({}))", arg_strs[0], arg_strs[1])),
        "log2" => Some(format!("log2({})", arg_strs[0])),
        "log10" => Some(format!("log10({})", arg_strs[0])),
        "sin" => Some(format!("sin({})", arg_strs[0])),
        "cos" => Some(format!("cos({})", arg_strs[0])),
        "tan" => Some(format!("tan({})", arg_strs[0])),
        "asin" => Some(format!("asin({})", arg_strs[0])),
        "acos" => Some(format!("acos({})", arg_strs[0])),
        "atan" => Some(format!("atan({})", arg_strs[0])),
        "atan2" => Some(format!("atan2({}, {})", arg_strs[0], arg_strs[1])),
        "floor" => Some(format!("floor({})", arg_strs[0])),
        "ceil" => Some(format!("ceil({})", arg_strs[0])),
        "round" => Some(format!("round({})", arg_strs[0])),
        "trunc" => Some(format!("trunc({})", arg_strs[0])),
        "fract" => Some(format!("(({}) - floor({}))", arg_strs[0], arg_strs[0])),
        "is_nan" => Some(format!("isnan({})", arg_strs[0])),
        "is_finite" => Some(format!("isfinite({})", arg_strs[0])),
        "is_infinite" => Some(format!("isinf({})", arg_strs[0])),
        "min" => Some(format!("fmin({}, {})", arg_strs[0], arg_strs[1])),
        "max" => Some(format!("fmax({}, {})", arg_strs[0], arg_strs[1])),
        "clamp" => Some(format!(
            "fmin(fmax({}, {}), {})",
            arg_strs[0], arg_strs[1], arg_strs[2]
        )),
        "sum" => Some(Some(("0.0", "+", &arg_strs)).map(|(init, op, xs)| {
            xs.iter().fold(init.to_string(), |acc, item| {
                format!("(({}) {} ({}))", acc, op, item)
            })
        })?),
        "product" => Some(Some(("1.0", "*", &arg_strs)).map(|(init, op, xs)| {
            xs.iter().fold(init.to_string(), |acc, item| {
                format!("(({}) {} ({}))", acc, op, item)
            })
        })?),
        "mean" => Some(format!(
            "(({}) / (double)({}))",
            fold_sum(&arg_strs),
            arg_strs.len()
        )),
        "variance" => {
            let mean = format!("(({}) / (double)({}))", fold_sum(&arg_strs), arg_strs.len());
            let mut terms = vec![];
            for arg in &arg_strs {
                terms.push(format!(
                    "(({}) - ({})) * (({}) - ({}))",
                    arg, mean, arg, mean
                ));
            }
            Some(format!(
                "(({}) / (double)({}))",
                fold_sum(&terms),
                arg_strs.len()
            ))
        }
        "stddev" => {
            let variance = generate_intrinsic_call("variance", args)?;
            Some(format!("sqrt({})", variance))
        }
        "median" => Some(format!(
            "agi_stats_median((double[]){{{}}}, {})",
            arg_strs.join(", "),
            arg_strs.len()
        )),
        "mode" => Some(format!(
            "agi_stats_mode_or_nan((double[]){{{}}}, {})",
            arg_strs.join(", "),
            arg_strs.len()
        )),
        "quantile" if arg_strs.len() >= 2 => {
            let n = arg_strs.len() - 1;
            Some(format!(
                "agi_stats_quantile((double[]){{{}}}, {}, {})",
                arg_strs[..n].join(", "),
                n,
                arg_strs[n]
            ))
        }
        "percentile" if arg_strs.len() >= 2 => {
            let n = arg_strs.len() - 1;
            Some(format!(
                "agi_stats_percentile((double[]){{{}}}, {}, {})",
                arg_strs[..n].join(", "),
                n,
                arg_strs[n]
            ))
        }
        _ => None,
    }
}

fn generate_json_object_expr(items: &[(String, HirExpr)]) -> String {
    let keys = items
        .iter()
        .map(|(key, _)| format!("\"{}\"", key.replace("\"", "\\\"")))
        .collect::<Vec<_>>()
        .join(", ");
    let values = items
        .iter()
        .map(|(key, value)| generate_json_value_expr(Some(key.as_str()), value))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "agi_json_object((const char*[]){{{}}}, (const char*[]){{{}}}, {})",
        keys,
        values,
        items.len()
    )
}

fn generate_json_array_expr(items: &[HirExpr]) -> String {
    let values = items
        .iter()
        .map(|value| generate_json_value_expr(None, value))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "agi_json_array((const char*[]){{{}}}, {})",
        values,
        items.len()
    )
}

fn generate_json_value_expr(key: Option<&str>, expr: &HirExpr) -> String {
    match expr {
        HirExpr::String(_, _, _) => {
            if matches!(key, Some("query" | "headers")) {
                generate_expr(expr)
            } else {
                format!("agi_json_escape_and_quote({})", generate_expr(expr))
            }
        }
        HirExpr::Identifier(_, ty, _) => {
            if matches!(key, Some("query" | "headers")) && ty == &Type::String {
                generate_expr(expr)
            } else if ty == &Type::String || ty == &Type::Unknown {
                format!("agi_json_escape_and_quote({})", generate_expr(expr))
            } else if matches!(ty, Type::I32 | Type::I64 | Type::U32 | Type::U64) {
                format!("agi_json_i64((int64_t)({}))", generate_expr(expr))
            } else if matches!(ty, Type::F32 | Type::F64) {
                format!("agi_json_f64((double)({}))", generate_expr(expr))
            } else {
                generate_expr(expr)
            }
        }
        HirExpr::Integer(_, _, _) => format!("agi_json_i64((int64_t)({}))", generate_expr(expr)),
        HirExpr::Float(_, _, _) => format!("agi_json_f64((double)({}))", generate_expr(expr)),
        HirExpr::Bool(_, _, _) => format!("(({}) ? \"true\" : \"false\")", generate_expr(expr)),
        HirExpr::ObjectLiteral(items, _, _) => generate_json_object_expr(items),
        HirExpr::ListLiteral(items, _, _) => generate_json_array_expr(items),
        _ => format!("agi_json_escape_and_quote({})", generate_expr(expr)),
    }
}

fn list_literal_numeric_elements(expr: &HirExpr) -> Option<Vec<String>> {
    match expr {
        HirExpr::ListLiteral(items, _, _) => {
            let mut out = vec![];
            for item in items {
                match item {
                    HirExpr::Integer(_, _, _)
                    | HirExpr::Float(_, _, _)
                    | HirExpr::Binary { .. }
                    | HirExpr::Call { .. }
                    | HirExpr::Identifier(_, _, _) => {
                        out.push(format!("(double)({})", generate_expr(item)));
                    }
                    _ => return None,
                }
            }
            Some(out)
        }
        _ => None,
    }
}

fn list_literal_nested_numeric_elements(expr: &HirExpr) -> Option<Vec<Vec<String>>> {
    match expr {
        HirExpr::ListLiteral(items, _, _) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(list_literal_numeric_elements(item)?);
            }
            Some(out)
        }
        _ => None,
    }
}

fn fold_sum(args: &[String]) -> String {
    args.iter().fold("0.0".to_string(), |acc, item| {
        format!("(({}) + ({}))", acc, item)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use agilang_source::SourceFile;
    use std::fs;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_codegen_basic() {
        let source = SourceFile::new(
            "test.agi",
            "fn main() -> i32:\n    let a = sqrt(9.0)\n    let b = pow(2.0, 3.0)\n    let m = median([1.0, 2.0, 3.0, 4.0])\n    print(\"Hello C Codegen\", a, b, m)\n    return 0\n",
        );
        let hir = agilang_compiler::hir(&source).unwrap();
        let c_code = generate(&hir);

        assert!(c_code.contains("int32_t main_agi("));
        assert!(c_code.contains("#include <math.h>"));
        assert!(c_code.contains("#include <stdlib.h>"));
        assert!(c_code.contains("sqrt(9"));
        assert!(c_code.contains("pow(2"));
        assert!(c_code.contains("agi_stats_median((double[]){(double)(1"));
        assert!(c_code.contains("agi_print(\"Hello C Codegen\");"));
        assert!(c_code.contains("agi_print_f64((double)(a))"));
        assert!(c_code.contains("agi_print_f64((double)(b))"));
        assert!(c_code.contains("agi_print_f64((double)(m))"));
        assert!(c_code.contains("return 0;"));
        assert!(c_code.contains("int main(void)"));
    }

    #[test]
    fn test_codegen_void_main_bridge_returns_zero() {
        let source = SourceFile::new(
            "void_main.agi",
            "fn main() -> void:\n    print(\"hello\")\n",
        );
        let hir = agilang_compiler::hir(&source).unwrap();
        let c_code = generate(&hir);

        assert!(c_code.contains("void main_agi("));
        assert!(c_code.contains("    main_agi();\n    return 0;"));
        assert!(!c_code.contains("return (int32_t)main_agi();"));
    }

    #[test]
    fn test_codegen_inferred_void_main_bridge_returns_zero() {
        let source = SourceFile::new("inferred_void_main.agi", "fn main():\n    print(\"hello\")\n");
        let hir = agilang_compiler::hir(&source).unwrap();
        let c_code = generate(&hir);

        assert!(c_code.contains("void main_agi("));
        assert!(c_code.contains("    main_agi();\n    return 0;"));
        assert!(!c_code.contains("return (int32_t)main_agi();"));
    }

    #[test]
    fn test_codegen_stats_list_overload_hir() {
        let source = SourceFile::new(
            "stats.agi",
            "fn main() -> f64:\n    let m = mean([1.0, 2.0, 3.0, 4.0])\n    let q = quantile([1.0, 2.0, 3.0, 4.0], 0.5)\n    let p = percentile([1.0, 2.0, 3.0, 4.0], 50.0)\n    let s = stddev([1.0, 2.0, 3.0, 4.0])\n    return m + q + p + s\n",
        );
        let hir = agilang_compiler::hir(&source).unwrap();
        let c_code = generate(&hir);

        assert!(c_code.contains("agi_stats_mean((double[]){(double)(1"));
        assert!(c_code.contains("agi_stats_quantile((double[]){(double)(1"));
        assert!(c_code.contains("agi_stats_percentile((double[]){(double)(1"));
        assert!(c_code.contains("agi_stats_stddev((double[]){(double)(1"));
    }

    #[test]
    fn test_codegen_stats_named_list_variable_hir() {
        let source = SourceFile::new(
            "stats_var.agi",
            "fn main() -> f64:\n    let xs = [1.0, 2.0, 3.0, 4.0]\n    let m = mean(xs)\n    let q = quantile(xs, 0.5)\n    let p = percentile(xs, 50.0)\n    return m + q + p\n",
        );
        let hir = agilang_compiler::hir(&source).unwrap();
        let c_code = generate(&hir);

        assert!(c_code.contains("agi_list_f64 xs = { xs_data, 4, 4, false }"));
        assert!(c_code.contains("agi_stats_mean(xs.data, xs.length)"));
        assert!(c_code.contains("agi_stats_quantile(xs.data, xs.length, 0.5)"));
        assert!(c_code.contains("agi_stats_percentile(xs.data, xs.length, 50)"));
    }

    #[test]
    fn test_codegen_list_index_uses_bounds_checked_helper() {
        let source = SourceFile::new(
            "list_index.agi",
            "fn main() -> f64:\n    let xs = [10.0, 20.0, 30.0]\n    let v = xs[1]\n    return v\n",
        );
        let hir = agilang_compiler::hir(&source).unwrap();
        let c_code = generate(&hir);

        assert!(
            c_code.contains("static double agi_list_get_f64(const agi_list_f64* xs, int64_t idx)")
        );
        assert!(c_code.contains("agi_list_get_f64(&xs, (int64_t)(1))"));
    }

    #[test]
    fn test_codegen_list_assignment_and_append_lowering() {
        let source = SourceFile::new(
            "list_mut.agi",
            "fn main() -> i32:\n    let values = [1.0, 2.0, 3.0]\n    values[1] = 9.0\n    append(values, 10.0)\n    return 0\n",
        );
        let hir = agilang_compiler::hir(&source).unwrap();
        let c_code = generate(&hir);

        assert!(c_code.contains("agi_list_set_f64(&values, (int64_t)(1), (double)(9"));
        assert!(c_code.contains("agi_list_push_f64(&values, (double)(10"));
    }

    #[test]
    fn test_codegen_nested_numeric_list_literal() {
        let source = SourceFile::new(
            "nested.agi",
            "fn main() -> i32:\n    let grid = [[1.0, 2.0], [3.0, 4.0]]\n    let row = grid[1]\n    let x = row[0]\n    print(x)\n    return 0\n",
        );
        let hir = agilang_compiler::hir(&source).unwrap();
        let c_code = generate(&hir);

        assert!(c_code.contains("typedef struct {\n    agi_list_f64* data;"));
        assert!(c_code.contains("agi_list_list_f64 grid = { grid_data, 2, 2, false }"));
        assert!(c_code.contains("agi_list_get_list_f64(&grid, (int64_t)(1))"));
    }

    #[test]
    fn test_codegen_while_loop_lowering() {
        let source = SourceFile::new(
            "while_loop.agi",
            "fn main() -> i32:\n    let value = 0\n    while value < 3:\n        value = value + 1\n    return value\n",
        );
        let hir = agilang_compiler::hir(&source).unwrap();
        let c_code = generate(&hir);

        assert!(c_code.contains("while ((value < 3)) {"));
        assert!(c_code.contains("value = (value + 1);"));
    }

    #[test]
    fn test_codegen_compound_assignment_lowering() {
        let source = SourceFile::new(
            "compound.agi",
            "fn main() -> i32:\n    let value = 1\n    value += 2\n    value *= 3\n    return value\n",
        );
        let hir = agilang_compiler::hir(&source).unwrap();
        let c_code = generate(&hir);

        assert!(c_code.contains("value = (value + 2);"));
        assert!(c_code.contains("value = (value * 3);"));
    }

    #[test]
    fn test_codegen_elif_lowering() {
        let source = SourceFile::new(
            "elif.agi",
            "fn main() -> i32:\n    let value = 1\n    if value == 0:\n        return 0\n    elif value == 1:\n        return 1\n    else:\n        return 2\n",
        );
        let hir = agilang_compiler::hir(&source).unwrap();
        let c_code = generate(&hir);

        assert!(c_code.contains("if ((value == 0)) {"));
        assert!(c_code.contains("else {\n        if ((value == 1)) {"));
    }

    #[test]
    fn test_codegen_break_and_continue_lowering() {
        let source = SourceFile::new(
            "loop_control.agi",
            "fn main() -> i32:\n    let value = 0\n    while value < 5:\n        value += 1\n        if value == 2:\n            continue\n        if value == 4:\n            break\n    return value\n",
        );
        let hir = agilang_compiler::hir(&source).unwrap();
        let c_code = generate(&hir);

        assert!(c_code.contains("continue;"));
        assert!(c_code.contains("break;"));
    }

    #[test]
    fn test_codegen_exec_native_stats_binary() {
        if !cfg!(windows) {
            return;
        }

        let where_out = Command::new("cmd")
            .args(["/C", "where cl.exe"])
            .output()
            .expect("failed to probe cl.exe");
        if !where_out.status.success() {
            return;
        }

        let source = SourceFile::new(
            "exec_stats.agi",
            "fn main() -> f64:\n    let m = mean([1.0, 2.0, 3.0, 4.0])\n    let d = median([1.0, 2.0, 3.0, 4.0])\n    let q = quantile([1.0, 2.0, 3.0, 4.0], 0.5)\n    let p = percentile([1.0, 2.0, 3.0, 4.0], 50.0)\n    let v = variance([1.0, 2.0, 3.0, 4.0])\n    let s = stddev([1.0, 2.0, 3.0, 4.0])\n    return m + d + q + p + v + s\n",
        );
        let hir = agilang_compiler::hir(&source).unwrap();
        let c_code = generate(&hir);

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "agilang_codegen_exec_{}_{}",
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&dir).unwrap();

        let c_path = dir.join("app.c");
        let exe_path = dir.join("app.exe");
        fs::write(&c_path, c_code).unwrap();

        let compile = Command::new("cmd")
            .args([
                "/C",
                &format!(
                    "cl.exe /nologo /O2 /Fe:\"{}\" \"{}\"",
                    exe_path.display(),
                    c_path.display()
                ),
            ])
            .output()
            .unwrap();

        if !compile.status.success() {
            panic!(
                "failed to compile generated C. stdout: {} stderr: {}",
                String::from_utf8_lossy(&compile.stdout),
                String::from_utf8_lossy(&compile.stderr)
            );
        }

        let run = Command::new(&exe_path).output().unwrap();
        assert_eq!(run.status.code(), Some(12));

        fs::remove_dir_all(&dir).ok();
    }

    fn compile_and_run_with_runtime_stub(source_name: &str, source_text: &str) -> Option<(i32, String)> {
        if !cfg!(windows) {
            return None;
        }

        let where_out = Command::new("cmd")
            .args(["/C", "where cl.exe"])
            .output()
            .ok()?;
        if !where_out.status.success() {
            return None;
        }

        let source = SourceFile::new(source_name, source_text);
        let hir = agilang_compiler::hir(&source).ok()?;
        let c_code = generate(&hir);

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "agilang_codegen_entry_{}_{}",
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&dir).unwrap();

        let c_path = dir.join("app.c");
        let runtime_stub_path = dir.join("runtime_stub.c");
        let exe_path = dir.join("app.exe");
        fs::write(&c_path, c_code).unwrap();
        fs::write(
            &runtime_stub_path,
            "void agi_print(const char* msg) { (void)msg; }\n\
             const char* agi_http_get(const char* url) { (void)url; return \"\"; }\n\
             const char* agi_http_post(const char* url, const char* body) { (void)url; (void)body; return \"\"; }\n\
             const char* agi_http_get_json(const char* url) { (void)url; return \"{}\"; }\n\
             const char* agi_http_post_json(const char* url, const char* body) { (void)url; (void)body; return \"{}\"; }\n\
             const char* agi_http_request_json(const char* request_json) { (void)request_json; return \"{}\"; }\n",
        )
        .unwrap();

        let compile = Command::new("cmd")
            .args([
                "/C",
                &format!(
                    "cl.exe /nologo /O2 /Fe:\"{}\" \"{}\" \"{}\"",
                    exe_path.display(),
                    c_path.display(),
                    runtime_stub_path.display()
                ),
            ])
            .output()
            .unwrap();

        if !compile.status.success() {
            panic!(
                "failed to compile generated C. stdout: {} stderr: {}",
                String::from_utf8_lossy(&compile.stdout),
                String::from_utf8_lossy(&compile.stderr)
            );
        }

        let run = Command::new(&exe_path).output().unwrap();
        let result = (
            run.status.code().unwrap_or_default(),
            String::from_utf8_lossy(&run.stdout).to_string(),
        );
        fs::remove_dir_all(&dir).ok();
        Some(result)
    }

    #[test]
    fn test_codegen_exec_void_main_binary() {
        let Some((code, _stdout)) =
            compile_and_run_with_runtime_stub("void_exec.agi", "fn main() -> void:\n    print(\"hello\")\n")
        else {
            return;
        };
        assert_eq!(code, 0);
    }

    #[test]
    fn test_codegen_exec_inferred_void_main_binary() {
        let Some((code, _stdout)) =
            compile_and_run_with_runtime_stub("inferred_void_exec.agi", "fn main():\n    print(\"hello\")\n")
        else {
            return;
        };
        assert_eq!(code, 0);
    }

    #[test]
    fn test_codegen_exec_void_helper_main_binary() {
        let Some((code, _stdout)) = compile_and_run_with_runtime_stub(
            "void_helper_exec.agi",
            "fn helper() -> void:\n    print(\"helper\")\n\nfn main() -> void:\n    helper()\n",
        ) else {
            return;
        };
        assert_eq!(code, 0);
    }

    #[test]
    fn test_codegen_exec_i32_main_binary_preserves_exit_code() {
        let Some((code, _stdout)) = compile_and_run_with_runtime_stub(
            "i32_exec.agi",
            "fn main() -> i32:\n    return 7\n",
        ) else {
            return;
        };
        assert_eq!(code, 7);
    }

    #[test]
    fn test_codegen_exec_while_loop_binary() {
        let Some((code, _stdout)) = compile_and_run_with_runtime_stub(
            "while_exec.agi",
            "fn main() -> i32:\n    let value = 0\n    while value < 4:\n        value = value + 1\n    return value\n",
        ) else {
            return;
        };
        assert_eq!(code, 4);
    }

    #[test]
    fn test_codegen_exec_compound_assignment_binary() {
        let Some((code, _stdout)) = compile_and_run_with_runtime_stub(
            "compound_exec.agi",
            "fn main() -> i32:\n    let value = 1\n    value += 2\n    value *= 3\n    return value\n",
        ) else {
            return;
        };
        assert_eq!(code, 9);
    }

    #[test]
    fn test_codegen_exec_elif_binary() {
        let Some((code, _stdout)) = compile_and_run_with_runtime_stub(
            "elif_exec.agi",
            "fn main() -> i32:\n    let value = 1\n    if value == 0:\n        return 0\n    elif value == 1:\n        return 7\n    else:\n        return 2\n",
        ) else {
            return;
        };
        assert_eq!(code, 7);
    }

    #[test]
    fn test_codegen_exec_break_and_continue_binary() {
        let Some((code, _stdout)) = compile_and_run_with_runtime_stub(
            "loop_control_exec.agi",
            "fn main() -> i32:\n    let value = 0\n    while value < 5:\n        value += 1\n        if value == 2:\n            continue\n        if value == 4:\n            break\n    return value\n",
        ) else {
            return;
        };
        assert_eq!(code, 4);
    }

    #[test]
    fn generated_runtime_contains_deterministic_list_ownership_helpers() {
        let program = HirProgram { functions: vec![] };
        let c_code = generate(&program);
        assert!(c_code.contains("static void agi_list_free_f64"));
        assert!(c_code.contains("static bool agi_list_reserve_f64"));
        assert!(c_code.contains("static bool agi_list_insert_f64"));
        assert!(c_code.contains("static bool agi_list_remove_f64"));
        assert!(c_code.contains("static void agi_list_free_list_f64"));
    }
}
