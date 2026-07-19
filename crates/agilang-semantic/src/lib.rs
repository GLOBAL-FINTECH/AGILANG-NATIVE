use agilang_ast::{BinaryOp, Expr, Function, Program, Stmt};
use agilang_diagnostics::Diagnostic;
use agilang_ir::{HirBinaryOp, HirExpr, HirFunction, HirParameter, HirProgram, HirStmt};
use agilang_source::Span;
use agilang_symbols::{Symbol, SymbolKind, SymbolTable};
use agilang_types::{FunctionType, Type};

pub struct Analyser {
    scopes: Vec<SymbolTable>,
    errors: Vec<Diagnostic>,
    current_return_type: Option<Type>,
}

impl Analyser {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mut global_scope = SymbolTable::new();
        // Register builtins
        global_scope
            .insert(Symbol {
                name: "print".to_string(),
                kind: SymbolKind::Builtin,
                ty: Type::Function(FunctionType {
                    params: vec![Type::String],
                    ret: Box::new(Type::Void),
                }),
                span: Span::default(),
                mutable: false,
            })
            .ok();

        Self {
            scopes: vec![global_scope],
            errors: vec![],
            current_return_type: None,
        }
    }

    pub fn analyse(
        mut self,
        program: &Program,
    ) -> Result<(HirProgram, Vec<SymbolTable>), Vec<Diagnostic>> {
        // First pass: declare all functions in global scope
        for func in &program.functions {
            let mut param_types = vec![];
            for param in &func.params {
                let ty = match &param.ty {
                    Some(tr) => Type::from_str(&tr.name),
                    None => Type::Unknown,
                };
                param_types.push(ty);
            }
            let ret_type = match &func.return_type {
                Some(tr) => Type::from_str(&tr.name),
                None => Type::Void,
            };

            let symbol = Symbol {
                name: func.name.clone(),
                kind: SymbolKind::Function,
                ty: Type::Function(FunctionType {
                    params: param_types,
                    ret: Box::new(ret_type),
                }),
                span: func.span,
                mutable: false,
            };

            if let Err(existing) = self.scopes[0].insert(symbol) {
                self.errors.push(
                    Diagnostic::error(
                        "E1002",
                        format!("duplicate symbol `{}`", func.name),
                        func.span,
                    )
                    .with_hint(format!(
                        "first declared at span {}..{}",
                        existing.span.start, existing.span.end
                    )),
                );
            }
        }

        let mut hir_functions = vec![];
        // Second pass: analyse function bodies
        for func in &program.functions {
            if let Some(hir_func) = self.analyse_function(func) {
                hir_functions.push(hir_func);
            }
        }

        if self.errors.is_empty() {
            Ok((
                HirProgram {
                    functions: hir_functions,
                },
                self.scopes,
            ))
        } else {
            Err(self.errors)
        }
    }

    fn enter_scope(&mut self) {
        self.scopes.push(SymbolTable::new());
    }

    fn exit_scope(&mut self) -> SymbolTable {
        self.scopes.pop().expect("empty scope stack")
    }

    fn declare(&mut self, symbol: Symbol) {
        let current = self.scopes.last_mut().expect("empty scope stack");
        if let Err(existing) = current.insert(symbol.clone()) {
            self.errors.push(
                Diagnostic::error(
                    "E1002",
                    format!("duplicate symbol `{}`", symbol.name),
                    symbol.span,
                )
                .with_hint(format!(
                    "first declared at span {}..{}",
                    existing.span.start, existing.span.end
                )),
            );
        }
    }

    fn lookup(&self, name: &str) -> Option<&Symbol> {
        for scope in self.scopes.iter().rev() {
            if let Some(sym) = scope.get(name) {
                return Some(sym);
            }
        }
        None
    }

    fn analyse_function(&mut self, func: &Function) -> Option<HirFunction> {
        self.enter_scope();

        let mut hir_params = vec![];
        let mut param_types = vec![];
        for param in &func.params {
            let ty = match &param.ty {
                Some(tr) => {
                    let t = Type::from_str(&tr.name);
                    if t == Type::Unknown {
                        self.errors.push(Diagnostic::error(
                            "E1004",
                            format!("unknown type `{}`", tr.name),
                            tr.span,
                        ));
                        Type::Error
                    } else {
                        t
                    }
                }
                None => Type::Unknown,
            };

            self.declare(Symbol {
                name: param.name.clone(),
                kind: SymbolKind::Parameter,
                ty: ty.clone(),
                span: param.span,
                mutable: true,
            });

            hir_params.push(HirParameter {
                name: param.name.clone(),
                ty: ty.clone(),
                span: param.span,
            });
            param_types.push(ty);
        }

        let ret_type = match &func.return_type {
            Some(tr) => {
                let t = Type::from_str(&tr.name);
                if t == Type::Unknown {
                    self.errors.push(Diagnostic::error(
                        "E1004",
                        format!("unknown type `{}`", tr.name),
                        tr.span,
                    ));
                    Type::Error
                } else {
                    t
                }
            }
            None => Type::Void,
        };

        self.current_return_type = Some(ret_type.clone());

        let mut hir_body = vec![];
        for stmt in &func.body {
            if let Some(hir_stmt) = self.analyse_stmt(stmt) {
                hir_body.push(hir_stmt);
            }
        }

        // Return analysis: check that a non-void function returns on all control flows
        if ret_type != Type::Void && ret_type != Type::Error {
            let mut returns = false;
            for stmt in &hir_body {
                if matches!(stmt, HirStmt::Return { .. }) {
                    returns = true;
                    break;
                }
            }
            if !returns {
                self.errors.push(Diagnostic::error(
                    "E2008",
                    format!(
                        "missing return statement in function returning `{}`",
                        ret_type
                    ),
                    func.span,
                ));
            }
        }

        self.exit_scope();
        self.current_return_type = None;

        Some(HirFunction {
            name: func.name.clone(),
            params: hir_params,
            return_type: ret_type,
            body: hir_body,
            span: func.span,
        })
    }

    fn analyse_stmt(&mut self, stmt: &Stmt) -> Option<HirStmt> {
        match stmt {
            Stmt::Let {
                name,
                ty: type_ref,
                value,
                mutable,
                span,
            } => {
                let val_expr = self.analyse_expr(value)?;
                let declared_ty = match type_ref {
                    Some(tr) => {
                        let t = Type::from_str(&tr.name);
                        if t == Type::Unknown {
                            self.errors.push(Diagnostic::error(
                                "E1004",
                                format!("unknown type `{}`", tr.name),
                                tr.span,
                            ));
                            Type::Error
                        } else {
                            t
                        }
                    }
                    None => val_expr.ty().clone(),
                };

                if !declared_ty.is_compatible(val_expr.ty()) {
                    self.errors.push(Diagnostic::error(
                        "E2001",
                        format!(
                            "type mismatch: expected `{}`, found `{}`",
                            declared_ty,
                            val_expr.ty()
                        ),
                        value.span(),
                    ));
                }

                self.declare(Symbol {
                    name: name.clone(),
                    kind: if *mutable {
                        SymbolKind::Local
                    } else {
                        SymbolKind::Constant
                    },
                    ty: declared_ty.clone(),
                    span: *span,
                    mutable: *mutable,
                });

                Some(HirStmt::Let {
                    name: name.clone(),
                    ty: declared_ty,
                    value: val_expr,
                    mutable: *mutable,
                    span: *span,
                })
            }
            Stmt::Return { value, span } => {
                let hir_val = match value {
                    Some(expr) => {
                        let val_expr = self.analyse_expr(expr)?;
                        if let Some(expected_ty) = &self.current_return_type {
                            if expected_ty == &Type::Void {
                                self.errors.push(Diagnostic::error(
                                    "E2005",
                                    "extraneous return value in void function",
                                    expr.span(),
                                ));
                            } else if !expected_ty.is_compatible(val_expr.ty()) {
                                self.errors.push(Diagnostic::error(
                                    "E2006",
                                    format!(
                                        "type mismatch: expected return type `{}`, found `{}`",
                                        expected_ty,
                                        val_expr.ty()
                                    ),
                                    expr.span(),
                                ));
                            }
                        }
                        Some(val_expr)
                    }
                    None => {
                        if let Some(expected_ty) = &self.current_return_type {
                            if expected_ty != &Type::Void && expected_ty != &Type::Error {
                                self.errors.push(Diagnostic::error(
                                    "E2004",
                                    format!(
                                        "missing return value in function expecting `{}`",
                                        expected_ty
                                    ),
                                    *span,
                                ));
                            }
                        }
                        None
                    }
                };
                Some(HirStmt::Return {
                    value: hir_val,
                    span: *span,
                })
            }
            Stmt::Expr(expr) => {
                let val_expr = self.analyse_expr(expr)?;
                Some(HirStmt::Expr(val_expr))
            }
        }
    }

    fn analyse_expr(&mut self, expr: &Expr) -> Option<HirExpr> {
        match expr {
            Expr::Identifier(name, span) => match self.lookup(name) {
                Some(sym) => Some(HirExpr::Identifier(name.clone(), sym.ty.clone(), *span)),
                None => {
                    self.errors.push(Diagnostic::error(
                        "E1001",
                        format!("undefined symbol `{}`", name),
                        *span,
                    ));
                    Some(HirExpr::Identifier(name.clone(), Type::Error, *span))
                }
            },
            Expr::Integer(val, span) => Some(HirExpr::Integer(*val, *span)),
            Expr::Float(val, span) => Some(HirExpr::Float(*val, *span)),
            Expr::String(val, span) => Some(HirExpr::String(val.clone(), *span)),
            Expr::Bool(val, span) => Some(HirExpr::Bool(*val, *span)),
            Expr::Call { callee, args, span } => {
                let hir_callee = self.analyse_expr(callee)?;
                let mut hir_args = vec![];
                for arg in args {
                    if let Some(hir_arg) = self.analyse_expr(arg) {
                        hir_args.push(hir_arg);
                    }
                }

                let ret_ty = match hir_callee.ty() {
                    Type::Function(ft) => {
                        if ft.params.len() != hir_args.len() {
                            self.errors.push(Diagnostic::error(
                                "E2002",
                                format!(
                                    "argument count mismatch: function expects {} arguments, but {} were provided",
                                    ft.params.len(),
                                    hir_args.len()
                                ),
                                *span,
                            ));
                        } else {
                            for (i, (expected, arg)) in
                                ft.params.iter().zip(hir_args.iter()).enumerate()
                            {
                                if !expected.is_compatible(arg.ty()) {
                                    self.errors.push(Diagnostic::error(
                                        "E2003",
                                        format!(
                                            "argument type mismatch at position {}: expected `{}`, found `{}`",
                                            i + 1,
                                            expected,
                                            arg.ty()
                                        ),
                                        arg.span(),
                                    ));
                                }
                            }
                        }
                        *ft.ret.clone()
                    }
                    Type::Error => Type::Error,
                    _ => {
                        self.errors.push(Diagnostic::error(
                            "E2009",
                            "cannot call non-function symbol",
                            callee.span(),
                        ));
                        Type::Error
                    }
                };

                Some(HirExpr::Call {
                    callee: Box::new(hir_callee),
                    args: hir_args,
                    ty: ret_ty,
                    span: *span,
                })
            }
            Expr::Binary {
                left,
                op,
                right,
                span,
            } => {
                let hir_left = self.analyse_expr(left)?;
                let hir_right = self.analyse_expr(right)?;

                let (hir_op, ret_ty) = match op {
                    BinaryOp::Add => {
                        let ty =
                            if hir_left.ty() == &Type::String && hir_right.ty() == &Type::String {
                                Type::String
                            } else if hir_left.ty().is_numeric() && hir_right.ty().is_numeric() {
                                if hir_left.ty() == hir_right.ty() {
                                    hir_left.ty().clone()
                                } else {
                                    self.errors.push(Diagnostic::error(
                                        "E2001",
                                        format!(
                                            "type mismatch: cannot add `{}` and `{}`",
                                            hir_left.ty(),
                                            hir_right.ty()
                                        ),
                                        *span,
                                    ));
                                    Type::Error
                                }
                            } else {
                                self.errors.push(Diagnostic::error(
                                    "E2001",
                                    format!(
                                        "type mismatch: cannot add `{}` and `{}`",
                                        hir_left.ty(),
                                        hir_right.ty()
                                    ),
                                    *span,
                                ));
                                Type::Error
                            };
                        (HirBinaryOp::Add, ty)
                    }
                    BinaryOp::Subtract => {
                        let ty = if hir_left.ty().is_numeric()
                            && hir_right.ty().is_numeric()
                            && hir_left.ty() == hir_right.ty()
                        {
                            hir_left.ty().clone()
                        } else {
                            self.errors.push(Diagnostic::error(
                                "E2001",
                                format!(
                                    "type mismatch: cannot subtract `{}` and `{}`",
                                    hir_left.ty(),
                                    hir_right.ty()
                                ),
                                *span,
                            ));
                            Type::Error
                        };
                        (HirBinaryOp::Subtract, ty)
                    }
                    BinaryOp::Multiply => {
                        let ty = if hir_left.ty().is_numeric()
                            && hir_right.ty().is_numeric()
                            && hir_left.ty() == hir_right.ty()
                        {
                            hir_left.ty().clone()
                        } else {
                            self.errors.push(Diagnostic::error(
                                "E2001",
                                format!(
                                    "type mismatch: cannot multiply `{}` and `{}`",
                                    hir_left.ty(),
                                    hir_right.ty()
                                ),
                                *span,
                            ));
                            Type::Error
                        };
                        (HirBinaryOp::Multiply, ty)
                    }
                    BinaryOp::Divide => {
                        let ty = if hir_left.ty().is_numeric()
                            && hir_right.ty().is_numeric()
                            && hir_left.ty() == hir_right.ty()
                        {
                            hir_left.ty().clone()
                        } else {
                            self.errors.push(Diagnostic::error(
                                "E2001",
                                format!(
                                    "type mismatch: cannot divide `{}` and `{}`",
                                    hir_left.ty(),
                                    hir_right.ty()
                                ),
                                *span,
                            ));
                            Type::Error
                        };
                        (HirBinaryOp::Divide, ty)
                    }
                };

                Some(HirExpr::Binary {
                    left: Box::new(hir_left),
                    op: hir_op,
                    right: Box::new(hir_right),
                    ty: ret_ty,
                    span: *span,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agilang_lexer::lex;
    use agilang_parser::parse;
    use agilang_source::SourceFile;

    fn check_source(source_text: &str) -> Result<HirProgram, Vec<Diagnostic>> {
        let source = SourceFile::new("test.agi", source_text);
        let tokens = lex(&source)?;
        let ast = parse(&tokens)?;
        let analyser = Analyser::new();
        let (hir, _) = analyser.analyse(&ast)?;
        Ok(hir)
    }

    #[test]
    fn test_valid_variables() {
        let src =
            "fn main() -> i32:\n    let x: i64 = 10\n    let y: string = \"hello\"\n    return 0\n";
        assert!(check_source(src).is_ok());
    }

    #[test]
    fn test_undefined_symbol() {
        let src = "fn main() -> i32:\n    let x: i32 = y\n    return 0\n";
        let errs = check_source(src).unwrap_err();
        assert!(errs.iter().any(|e| e.code == "E1001"));
    }

    #[test]
    fn test_duplicate_symbol() {
        let src =
            "fn main() -> i32:\n    let x: i32 = 10\n    let x: string = \"hello\"\n    return 0\n";
        let errs = check_source(src).unwrap_err();
        assert!(errs.iter().any(|e| e.code == "E1002"));
    }

    #[test]
    fn test_type_mismatch() {
        let src = "fn main() -> i32:\n    let x: string = 10\n    return 0\n";
        let errs = check_source(src).unwrap_err();
        assert!(errs.iter().any(|e| e.code == "E2001"));
    }

    #[test]
    fn test_argument_count_mismatch() {
        let src = "fn main() -> i32:\n    print()\n    return 0\n";
        let errs = check_source(src).unwrap_err();
        assert!(errs.iter().any(|e| e.code == "E2002"));
    }

    #[test]
    fn test_argument_type_mismatch() {
        let src = "fn main() -> i32:\n    print(10)\n    return 0\n";
        let errs = check_source(src).unwrap_err();
        assert!(errs.iter().any(|e| e.code == "E2003"));
    }

    #[test]
    fn test_return_type_mismatch() {
        let src = "fn main() -> i32:\n    return \"hello\"\n";
        let errs = check_source(src).unwrap_err();
        assert!(errs.iter().any(|e| e.code == "E2006"));
    }

    #[test]
    fn test_missing_return() {
        let src = "fn main() -> i32:\n    let x: i32 = 0\n";
        let errs = check_source(src).unwrap_err();
        assert!(errs.iter().any(|e| e.code == "E2008"));
    }
}
