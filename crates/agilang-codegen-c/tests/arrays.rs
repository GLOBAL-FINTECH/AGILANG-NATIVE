use agilang_compiler::{hir, SourceFile};

fn generate(source: &str) -> String {
    let source = SourceFile::new("arrays.agi", source);
    let program = hir(&source).expect("array source should produce HIR");
    agilang_codegen_c::generate(&program)
}

#[test]
fn lowers_numeric_array_literal_and_indexing() {
    let generated = generate(
        "fn main() -> i32:\n    let values: array = [1.0, 2.0, 3.0]\n    let first: f64 = values[0]\n    if first == 1.0:\n        return 0\n    return 1\n",
    );

    assert!(generated.contains("agi_list_from_array_f64"));
    assert!(generated.contains("agi_list_get_f64"));
    assert!(!generated.contains("unsupported list literal"));
}

#[test]
fn lowers_append_len_and_array_iteration() {
    let generated = generate(
        "fn total(values: array) -> f64:\n    let result: f64 = 0.0\n    for value in values:\n        result += value\n    return result\n\nfn main() -> i32:\n    let values: array = [1.0, 2.0]\n    append(values, 3.0)\n    let count: i64 = len(values)\n    let result: f64 = total(values)\n    if count == 3 and result == 6.0:\n        return 0\n    return 1\n",
    );

    assert!(generated.contains("agi_list_push_f64"));
    assert!(generated.contains(".length"));
    assert!(generated.contains("for ("));
}

#[test]
fn lowers_nested_numeric_arrays() {
    let generated = generate(
        "fn main() -> i32:\n    let matrix: array = [[1.0, 2.0], [3.0, 4.0]]\n    let row: array = matrix[1]\n    let value: f64 = row[0]\n    if value == 3.0:\n        return 0\n    return 1\n",
    );

    assert!(generated.contains("agi_list_from_array_list_f64"));
    assert!(generated.contains("agi_list_get_list_f64"));
}
