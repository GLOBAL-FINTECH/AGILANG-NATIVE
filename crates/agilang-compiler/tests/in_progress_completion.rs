use agilang_compiler::{check, hir, SourceFile};
use agilang_ir::{HirExpr, HirStmt};
use agilang_types::Type;

fn source(text: &str) -> SourceFile {
    SourceFile::new("completion.agi", text)
}

#[test]
fn functions_returns_and_i32_are_semantically_complete() {
    let src = source(
        "fn add(left: i32, right: i32) -> i32:\n    return left + right\n\nfn main() -> i32:\n    return add(20, 22)\n",
    );
    check(&src).expect("functions, returns, and i32 must pass semantic analysis");
    let program = hir(&src).expect("HIR generation must succeed");
    assert_eq!(program.functions.len(), 2);
    assert_eq!(program.functions[0].return_type, Type::I32);
    assert_eq!(program.functions[1].return_type, Type::I32);
}

#[test]
fn missing_i32_return_value_is_rejected() {
    let src = source("fn main() -> i32:\n    return\n");
    let errors = check(&src).expect_err("missing i32 return value must fail");
    assert!(errors.iter().any(|error| error.code == "E2004"));
}

#[test]
fn wrong_i32_return_type_is_rejected() {
    let src = source("fn main() -> i32:\n    return \"wrong\"\n");
    let errors = check(&src).expect_err("wrong return type must fail");
    assert!(errors.iter().any(|error| error.code == "E2006"));
}

#[test]
fn inferred_array_iteration_preserves_numeric_operations() {
    let src = source(
        "fn total(values: array) -> f64:\n    let sum: f64 = 0.0\n    for value in values:\n        sum += value\n    return sum\n\nfn main() -> i32:\n    let values: array = [1.0, 2.0, 3.0]\n    let result: f64 = total(values)\n    if result == 6.0:\n        return 0\n    return 1\n",
    );
    check(&src).expect("numeric array iteration must type-check");
}

#[test]
fn nested_numeric_arrays_reach_hir_with_nested_types() {
    let src = source(
        "fn main() -> i32:\n    let matrix: array = [[1.0, 2.0], [3.0, 4.0]]\n    let row: array = matrix[1]\n    let value: f64 = row[0]\n    if value == 3.0:\n        return 0\n    return 1\n",
    );
    let program = hir(&src).expect("nested numeric arrays must lower to HIR");
    let main = program.functions.iter().find(|function| function.name == "main").unwrap();
    let matrix_type = main.body.iter().find_map(|statement| match statement {
        HirStmt::Let { name, ty, value: HirExpr::ListLiteral(_, _, _), .. } if name == "matrix" => Some(ty.clone()),
        _ => None,
    }).expect("matrix declaration must exist");
    assert!(matches!(matrix_type, Type::List(inner) if matches!(inner.as_ref(), Type::List(_))));
}

#[test]
fn mixed_nested_array_shapes_are_rejected() {
    let src = source("fn main() -> i32:\n    let values: array = [[1.0], 2.0]\n    return 0\n");
    let errors = check(&src).expect_err("mixed nested and scalar array elements must fail");
    assert!(errors.iter().any(|error| error.code == "E2001"));
}
