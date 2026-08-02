use agilang_compiler::{check, hir, SourceFile};
use agilang_ir::{HirExpr, HirStmt};
use agilang_types::Type;

fn source(text: &str) -> SourceFile {
    SourceFile::new("arrays.agi", text)
}

#[test]
fn infers_numeric_array_element_type() {
    let program = hir(&source(
        "fn main() -> i32:\n    let values: array = [1.0, 2.0, 3.0]\n    let first: f64 = values[0]\n    return 0\n",
    ))
    .expect("array program should type-check");

    let main = program
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main function");

    let array_ty = main.body.iter().find_map(|stmt| match stmt {
        HirStmt::Let { name, ty, .. } if name == "values" => Some(ty.clone()),
        _ => None,
    });

    assert_eq!(array_ty, Some(Type::List(Box::new(Type::F64))));
}

#[test]
fn supports_array_index_update_append_len_and_iteration() {
    check(&source(
        "fn total(values: array) -> f64:\n    let result: f64 = 0.0\n    for value in values:\n        result += value\n    return result\n\nfn main() -> i32:\n    let values: array = [1.0, 2.0, 3.0]\n    append(values, 4.0)\n    values[0] = 10.0\n    let count: i64 = len(values)\n    let first: f64 = values[0]\n    let sum: f64 = total(values)\n    if count == 4 and first == 10.0 and sum == 19.0:\n        return 0\n    return 1\n",
    ))
    .expect("complete array operations should type-check");
}

#[test]
fn rejects_mixed_element_types() {
    let diagnostics = check(&source(
        "fn main() -> i32:\n    let values: array = [1.0, true]\n    return 0\n",
    ))
    .expect_err("mixed array elements must fail");

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "E2001" && diagnostic.message.contains("element type mismatch")
    }));
}

#[test]
fn rejects_non_integer_index() {
    let diagnostics = check(&source(
        "fn main() -> i32:\n    let values: array = [1.0, 2.0]\n    let value: f64 = values[1.5]\n    return 0\n",
    ))
    .expect_err("floating-point array index must fail");

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "E2001" && diagnostic.message.contains("index must be integer")
    }));
}

#[test]
fn index_expression_has_element_type() {
    let program = hir(&source(
        "fn main() -> i32:\n    let values: array = [1, 2, 3]\n    let value: i64 = values[1]\n    return 0\n",
    ))
    .expect("array index should type-check");

    let main = program
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main function");

    let index_ty = main.body.iter().find_map(|stmt| match stmt {
        HirStmt::Let {
            name,
            value: HirExpr::Index { ty, .. },
            ..
        } if name == "value" => Some(ty.clone()),
        _ => None,
    });

    assert_eq!(index_ty, Some(Type::I64));
}
