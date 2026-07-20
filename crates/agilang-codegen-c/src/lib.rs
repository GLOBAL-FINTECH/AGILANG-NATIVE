use agilang_ir::{HirBinaryOp, HirExpr, HirProgram, HirStmt};
use agilang_types::Type;

pub fn generate(program: &HirProgram) -> String {
    let mut out = String::new();
    out.push_str("#include <stdint.h>\n");
    out.push_str("#include <stdio.h>\n");
    out.push_str("#include <stdbool.h>\n\n");

    // Declare the runtime print function
    out.push_str("// Linked AGILANG runtime ABI\n");
    out.push_str("extern void agi_print(const char* msg);\n\n");

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

    // Generate C main standard entry point
    out.push_str("// Standard C Main Entry Point\n");
    out.push_str("int32_t agilang_main(void) {\n");
    out.push_str("    return (int32_t)main_agi();\n");
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
                format!("{}return {};\n", ind, generate_expr(val))
            } else {
                format!("{}return;\n", ind)
            }
        }
        HirStmt::Expr(expr) => {
            format!("{}{};\n", ind, generate_expr(expr))
        }
    }
}

fn generate_expr(expr: &HirExpr) -> String {
    match expr {
        HirExpr::Identifier(name, _, _) => name.clone(),
        HirExpr::Integer(val, _, _) => val.to_string(),
        HirExpr::Float(val, _, _) => val.to_string(),
        HirExpr::String(val, _, _) => format!("\"{}\"", val.replace("\"", "\\\"")),
        HirExpr::Bool(val, _, _) => val.to_string(),
        HirExpr::Call { callee, args, .. } => {
            let callee_name = match &**callee {
                HirExpr::Identifier(name, _, _) => name.clone(),
                _ => "unknown".to_string(),
            };
            let mut arg_strs = vec![];
            for arg in args {
                arg_strs.push(generate_expr(arg));
            }
            if callee_name == "print" {
                format!("agi_print({})", arg_strs.join(", "))
            } else {
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

#[cfg(test)]
mod tests {
    use super::*;
    use agilang_source::SourceFile;

    #[test]
    fn test_codegen_basic() {
        let source = SourceFile::new(
            "test.agi",
            "fn main() -> i32:\n    print(\"Hello C Codegen\")\n    return 0\n",
        );
        let hir = agilang_compiler::hir(&source).unwrap();
        let c_code = generate(&hir);

        assert!(c_code.contains("int32_t main_agi("));
        assert!(c_code.contains("agi_print(\"Hello C Codegen\");"));
        assert!(c_code.contains("return 0;"));
        assert!(c_code.contains("int main(void)"));
    }
}
