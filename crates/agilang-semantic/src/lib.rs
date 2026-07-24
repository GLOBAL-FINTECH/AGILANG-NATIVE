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
    current_local_symbols: Vec<Symbol>,
}

impl Analyser {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mut global_scope = SymbolTable::new();
        // Register http global symbol.
        global_scope
            .insert(Symbol {
                name: "http".to_string(),
                kind: SymbolKind::Local,
                ty: Type::Unknown,
                span: Span::default(),
                mutable: false,
            })
            .ok();

        // Register builtins.
        for (name, params, ret) in [
            ("print", vec![Type::Unknown], Type::Void),
            ("abs", vec![Type::F64], Type::F64),
            ("sign", vec![Type::F64], Type::F64),
            ("sqrt", vec![Type::F64], Type::F64),
            ("cbrt", vec![Type::F64], Type::F64),
            ("exp", vec![Type::F64], Type::F64),
            ("exp2", vec![Type::F64], Type::F64),
            ("ln", vec![Type::F64], Type::F64),
            ("log2", vec![Type::F64], Type::F64),
            ("log10", vec![Type::F64], Type::F64),
            ("sin", vec![Type::F64], Type::F64),
            ("cos", vec![Type::F64], Type::F64),
            ("tan", vec![Type::F64], Type::F64),
            ("asin", vec![Type::F64], Type::F64),
            ("acos", vec![Type::F64], Type::F64),
            ("atan", vec![Type::F64], Type::F64),
            ("floor", vec![Type::F64], Type::F64),
            ("ceil", vec![Type::F64], Type::F64),
            ("round", vec![Type::F64], Type::F64),
            ("trunc", vec![Type::F64], Type::F64),
            ("fract", vec![Type::F64], Type::F64),
            ("is_nan", vec![Type::F64], Type::Bool),
            ("is_finite", vec![Type::F64], Type::Bool),
            ("is_infinite", vec![Type::F64], Type::Bool),
            ("min", vec![Type::F64, Type::F64], Type::F64),
            ("max", vec![Type::F64, Type::F64], Type::F64),
            ("clamp", vec![Type::F64, Type::F64, Type::F64], Type::F64),
            ("pow", vec![Type::F64, Type::F64], Type::F64),
            ("hypot", vec![Type::F64, Type::F64], Type::F64),
            ("log", vec![Type::F64, Type::F64], Type::F64),
            ("atan2", vec![Type::F64, Type::F64], Type::F64),
            ("sum", vec![Type::F64], Type::F64),
            ("product", vec![Type::F64], Type::F64),
            ("mean", vec![Type::F64], Type::F64),
            ("variance", vec![Type::F64], Type::F64),
            ("stddev", vec![Type::F64], Type::F64),
            ("median", vec![Type::F64], Type::F64),
            ("mode", vec![Type::F64], Type::F64),
            ("percentile", vec![Type::F64, Type::F64], Type::F64),
            ("quantile", vec![Type::F64, Type::F64], Type::F64),
            (
                "append",
                vec![Type::List(Box::new(Type::F64)), Type::F64],
                Type::Void,
            ),
            ("len", vec![Type::List(Box::new(Type::Unknown))], Type::I64),
            ("get", vec![Type::Unknown, Type::String], Type::String),
            (
                "post",
                vec![Type::Unknown, Type::String, Type::String],
                Type::String,
            ),
            ("get_json", vec![Type::Unknown, Type::String], Type::String),
            (
                "post_json",
                vec![Type::Unknown, Type::String, Type::String],
                Type::String,
            ),
        ] {
            global_scope
                .insert(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Builtin,
                    ty: Type::Function(FunctionType {
                        params,
                        ret: Box::new(ret),
                    }),
                    span: Span::default(),
                    mutable: false,
                })
                .ok();
        }

        Self {
            scopes: vec![global_scope],
            errors: vec![],
            current_return_type: None,
            current_local_symbols: vec![],
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
        self.current_local_symbols = vec![];

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
        let local_syms = std::mem::take(&mut self.current_local_symbols);

        Some(HirFunction {
            name: func.name.clone(),
            params: hir_params,
            return_type: ret_type,
            body: hir_body,
            local_symbols: local_syms,
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
                    None => Type::Unknown,
                };

                let val_expr = if declared_ty != Type::Unknown {
                    self.analyse_expr(value, Some(&declared_ty))?
                } else {
                    self.analyse_expr(value, None)?
                };

                let final_ty = if declared_ty != Type::Unknown {
                    declared_ty
                } else {
                    val_expr.ty().clone()
                };

                if final_ty != Type::Unknown && !final_ty.is_compatible(val_expr.ty()) {
                    self.errors.push(Diagnostic::error(
                        "E2001",
                        format!(
                            "type mismatch: expected `{}`, found `{}`",
                            final_ty,
                            val_expr.ty()
                        ),
                        value.span(),
                    ));
                }

                let local_sym = Symbol {
                    name: name.clone(),
                    kind: if *mutable {
                        SymbolKind::Local
                    } else {
                        SymbolKind::Constant
                    },
                    ty: final_ty.clone(),
                    span: *span,
                    mutable: *mutable,
                };

                self.declare(local_sym.clone());
                self.current_local_symbols.push(local_sym);

                Some(HirStmt::Let {
                    name: name.clone(),
                    ty: final_ty,
                    value: val_expr,
                    mutable: *mutable,
                    span: *span,
                })
            }
            Stmt::Assign {
                target,
                value,
                span,
            } => {
                let target_expr = self.analyse_expr(target, None)?;
                let expected_ty = target_expr.ty().clone();
                let value_expr = self.analyse_expr(value, Some(&expected_ty))?;

                if !expected_ty.is_compatible(value_expr.ty()) {
                    self.errors.push(Diagnostic::error(
                        "E2001",
                        format!(
                            "type mismatch: expected `{}`, found `{}`",
                            expected_ty,
                            value_expr.ty()
                        ),
                        value.span(),
                    ));
                }

                match &target_expr {
                    HirExpr::Identifier(name, _, target_span) => {
                        if let Some(sym) = self.lookup(name) {
                            if !sym.mutable {
                                self.errors.push(Diagnostic::error(
                                    "E2010",
                                    format!("cannot assign to immutable symbol `{}`", name),
                                    *target_span,
                                ));
                            }
                        }
                    }
                    HirExpr::Index { object, .. } => {
                        if let HirExpr::Identifier(name, _, target_span) = object.as_ref() {
                            if let Some(sym) = self.lookup(name) {
                                if !sym.mutable {
                                    self.errors.push(Diagnostic::error(
                                        "E2010",
                                        format!(
                                            "cannot assign through immutable list symbol `{}`",
                                            name
                                        ),
                                        *target_span,
                                    ));
                                }
                            }
                        }
                    }
                    _ => {
                        self.errors.push(Diagnostic::error(
                            "E2011",
                            "invalid assignment target",
                            target.span(),
                        ));
                    }
                }

                Some(HirStmt::Assign {
                    target: target_expr,
                    value: value_expr,
                    span: *span,
                })
            }
            Stmt::Return { value, span } => {
                let hir_val = match value {
                    Some(expr) => {
                        let ret_ty = self.current_return_type.clone();
                        let val_expr = self.analyse_expr(expr, ret_ty.as_ref())?;
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
                let val_expr = self.analyse_expr(expr, None)?;
                Some(HirStmt::Expr(val_expr))
            }
        }
    }

    fn analyse_expr(&mut self, expr: &Expr, expected_ty: Option<&Type>) -> Option<HirExpr> {
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
            Expr::Integer(val, span) => {
                let ty = match expected_ty {
                    Some(t) if t.is_integer() => t.clone(),
                    _ => Type::I64,
                };
                Some(HirExpr::Integer(*val, ty, *span))
            }
            Expr::Float(val, span) => {
                let ty = match expected_ty {
                    Some(t) if t.is_float() => t.clone(),
                    _ => Type::F64,
                };
                Some(HirExpr::Float(*val, ty, *span))
            }
            Expr::String(val, span) => Some(HirExpr::String(val.clone(), Type::String, *span)),
            Expr::Bool(val, span) => Some(HirExpr::Bool(*val, Type::Bool, *span)),
            Expr::ListLiteral(items, span) => {
                let mut hir_items = vec![];
                let mut inferred: Option<Type> = None;
                for item in items {
                    let hir_item = self.analyse_expr(item, inferred.as_ref())?;
                    if let Some(existing) = &inferred {
                        if !existing.is_compatible(hir_item.ty()) {
                            self.errors.push(Diagnostic::error(
                                "E2001",
                                format!(
                                    "list literal element type mismatch: expected `{}`, found `{}`",
                                    existing,
                                    hir_item.ty()
                                ),
                                hir_item.span(),
                            ));
                        }
                    } else {
                        inferred = Some(hir_item.ty().clone());
                    }
                    hir_items.push(hir_item);
                }
                let elem_ty = inferred.unwrap_or(Type::Unknown);
                Some(HirExpr::ListLiteral(
                    hir_items,
                    Type::List(Box::new(elem_ty)),
                    *span,
                ))
            }
            Expr::MemberAccess {
                object,
                member,
                span,
            } => {
                let hir_object = self.analyse_expr(object, None)?;
                let result_ty = match hir_object.ty() {
                    Type::Error => Type::Error,
                    _ => Type::Unknown,
                };
                Some(HirExpr::MemberAccess {
                    object: Box::new(hir_object),
                    member: member.clone(),
                    ty: result_ty,
                    span: *span,
                })
            }
            Expr::Index {
                object,
                index,
                span,
            } => {
                let hir_object = self.analyse_expr(object, None)?;
                let hir_index = self.analyse_expr(index, Some(&Type::I64))?;

                if !hir_index.ty().is_integer() && hir_index.ty() != &Type::Error {
                    self.errors.push(Diagnostic::error(
                        "E2001",
                        format!("list index must be integer, found `{}`", hir_index.ty()),
                        hir_index.span(),
                    ));
                }

                let result_ty = match hir_object.ty() {
                    Type::List(inner) => (*inner.clone()).clone(),
                    Type::Error => Type::Error,
                    other => {
                        self.errors.push(Diagnostic::error(
                            "E2001",
                            format!("cannot index non-list value of type `{}`", other),
                            hir_object.span(),
                        ));
                        Type::Error
                    }
                };

                Some(HirExpr::Index {
                    object: Box::new(hir_object),
                    index: Box::new(hir_index),
                    ty: result_ty,
                    span: *span,
                })
            }
            Expr::Call { callee, args, span } => {
                // Method-call desugaring support: `obj.method(x)` -> `method(obj, x)`.
                if let Expr::MemberAccess { object, member, .. } = callee.as_ref() {
                    let hir_object = self.analyse_expr(object, None)?;
                    let mut hir_args = vec![hir_object];
                    for arg in args {
                        if let Some(hir_arg) = self.analyse_expr(arg, None) {
                            hir_args.push(hir_arg);
                        }
                    }

                    let (ret_ty, callee_ty) = match member.as_str() {
                        "to_string" => (
                            Type::String,
                            Type::Function(FunctionType {
                                params: vec![Type::Unknown],
                                ret: Box::new(Type::String),
                            }),
                        ),
                        "append" => (
                            Type::Void,
                            Type::Function(FunctionType {
                                params: vec![Type::Unknown, Type::Unknown],
                                ret: Box::new(Type::Void),
                            }),
                        ),
                        "get" | "get_json" => (
                            Type::String,
                            Type::Function(FunctionType {
                                params: vec![Type::Unknown, Type::String],
                                ret: Box::new(Type::String),
                            }),
                        ),
                        "post" | "post_json" => (
                            Type::String,
                            Type::Function(FunctionType {
                                params: vec![Type::Unknown, Type::String, Type::String],
                                ret: Box::new(Type::String),
                            }),
                        ),
                        _ => (
                            Type::Unknown,
                            Type::Function(FunctionType {
                                params: vec![Type::Unknown; hir_args.len()],
                                ret: Box::new(Type::Unknown),
                            }),
                        ),
                    };

                    return Some(HirExpr::Call {
                        callee: Box::new(HirExpr::Identifier(member.clone(), callee_ty, *span)),
                        args: hir_args,
                        ty: ret_ty,
                        span: *span,
                    });
                }

                let hir_callee = self.analyse_expr(callee, None)?;
                let mut hir_args = vec![];

                if let HirExpr::Identifier(name, _, _) = &hir_callee {
                    if let Some(intrinsic_call) =
                        self.analyse_builtin_call(name, &hir_callee, args, *span)
                    {
                        return Some(intrinsic_call);
                    }
                }

                let callee_ty = hir_callee.ty().clone();
                match callee_ty {
                    Type::Function(ft) => {
                        if ft.params.len() != args.len() {
                            self.errors.push(Diagnostic::error(
                                "E2002",
                                format!(
                                    "argument count mismatch: function expects {} arguments, but {} were provided",
                                    ft.params.len(),
                                    args.len()
                                ),
                                *span,
                            ));
                            for arg in args {
                                if let Some(hir_arg) = self.analyse_expr(arg, None) {
                                    hir_args.push(hir_arg);
                                }
                            }
                        } else {
                            for (expected, arg) in ft.params.iter().zip(args.iter()) {
                                if let Some(hir_arg) = self.analyse_expr(arg, Some(expected)) {
                                    if !expected.is_compatible(hir_arg.ty()) {
                                        self.errors.push(Diagnostic::error(
                                            "E2003",
                                            format!(
                                                "argument type mismatch: expected `{}`, found `{}`",
                                                expected,
                                                hir_arg.ty()
                                            ),
                                            hir_arg.span(),
                                        ));
                                    }
                                    hir_args.push(hir_arg);
                                }
                            }
                        }
                        Some(HirExpr::Call {
                            callee: Box::new(hir_callee),
                            args: hir_args,
                            ty: *ft.ret,
                            span: *span,
                        })
                    }
                    Type::Error => {
                        for arg in args {
                            if let Some(hir_arg) = self.analyse_expr(arg, None) {
                                hir_args.push(hir_arg);
                            }
                        }
                        Some(HirExpr::Call {
                            callee: Box::new(hir_callee),
                            args: hir_args,
                            ty: Type::Error,
                            span: *span,
                        })
                    }
                    _ => {
                        self.errors.push(Diagnostic::error(
                            "E2009",
                            "cannot call non-function symbol",
                            callee.span(),
                        ));
                        for arg in args {
                            if let Some(hir_arg) = self.analyse_expr(arg, None) {
                                hir_args.push(hir_arg);
                            }
                        }
                        Some(HirExpr::Call {
                            callee: Box::new(hir_callee),
                            args: hir_args,
                            ty: Type::Error,
                            span: *span,
                        })
                    }
                }
            }
            Expr::Binary {
                left,
                op,
                right,
                span,
            } => {
                let hir_left = self.analyse_expr(left, expected_ty)?;
                let expected_right_ty = if hir_left.ty() != &Type::Error {
                    Some(hir_left.ty())
                } else {
                    expected_ty
                };
                let hir_right = self.analyse_expr(right, expected_right_ty)?;

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
                    BinaryOp::Equal | BinaryOp::NotEqual => {
                        let ty = if hir_left.ty().is_compatible(hir_right.ty())
                            || hir_left.ty() == &Type::Unknown
                            || hir_right.ty() == &Type::Unknown
                        {
                            Type::Bool
                        } else {
                            self.errors.push(Diagnostic::error(
                                "E2001",
                                format!(
                                    "type mismatch: cannot compare `{}` and `{}`",
                                    hir_left.ty(),
                                    hir_right.ty()
                                ),
                                *span,
                            ));
                            Type::Error
                        };
                        (
                            if matches!(op, BinaryOp::Equal) {
                                HirBinaryOp::Equal
                            } else {
                                HirBinaryOp::NotEqual
                            },
                            ty,
                        )
                    }
                    BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual => {
                        let ty = if hir_left.ty().is_numeric()
                            && hir_right.ty().is_numeric()
                            && hir_left.ty().is_compatible(hir_right.ty())
                        {
                            Type::Bool
                        } else {
                            self.errors.push(Diagnostic::error(
                                "E2001",
                                format!(
                                    "type mismatch: cannot order `{}` and `{}`",
                                    hir_left.ty(),
                                    hir_right.ty()
                                ),
                                *span,
                            ));
                            Type::Error
                        };
                        let op = match op {
                            BinaryOp::Less => HirBinaryOp::Less,
                            BinaryOp::LessEqual => HirBinaryOp::LessEqual,
                            BinaryOp::Greater => HirBinaryOp::Greater,
                            BinaryOp::GreaterEqual => HirBinaryOp::GreaterEqual,
                            _ => unreachable!(),
                        };
                        (op, ty)
                    }
                    BinaryOp::And | BinaryOp::Or => {
                        let ty = if hir_left.ty() == &Type::Bool && hir_right.ty() == &Type::Bool {
                            Type::Bool
                        } else {
                            self.errors.push(Diagnostic::error(
                                "E2001",
                                format!(
                                    "type mismatch: logical operator requires bool operands, found `{}` and `{}`",
                                    hir_left.ty(),
                                    hir_right.ty()
                                ),
                                *span,
                            ));
                            Type::Error
                        };
                        (
                            if matches!(op, BinaryOp::And) {
                                HirBinaryOp::And
                            } else {
                                HirBinaryOp::Or
                            },
                            ty,
                        )
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

    fn analyse_builtin_call(
        &mut self,
        name: &str,
        hir_callee: &HirExpr,
        args: &[Expr],
        span: Span,
    ) -> Option<HirExpr> {
        let sym = self.lookup(name)?;
        if sym.kind != SymbolKind::Builtin {
            return None;
        }

        let mut hir_args = vec![];
        let mut expect_numeric = false;
        let mut min_args = 0usize;
        let mut exact_args: Option<usize> = None;
        let mut allow_numeric_list_overload = false;
        let mut q_last_numeric = false;

        match name {
            "print" => {
                min_args = 1;
            }
            "sum" | "product" | "mean" | "variance" | "stddev" | "median" | "mode" => {
                min_args = 1;
                expect_numeric = true;
                allow_numeric_list_overload = true;
            }
            "append" => {
                exact_args = Some(2);
            }
            "len" => {
                exact_args = Some(1);
            }
            "get" | "get_json" => {
                exact_args = Some(2);
            }
            "post" | "post_json" => {
                exact_args = Some(3);
            }
            "percentile" | "quantile" => {
                min_args = 2;
                expect_numeric = true;
                q_last_numeric = true;
                allow_numeric_list_overload = true;
            }
            "min" | "max" | "pow" | "hypot" | "log" | "atan2" => {
                exact_args = Some(2);
                expect_numeric = true;
            }
            "clamp" => {
                exact_args = Some(3);
                expect_numeric = true;
            }
            "abs" | "sign" | "sqrt" | "cbrt" | "exp" | "exp2" | "ln" | "log2" | "log10" | "sin"
            | "cos" | "tan" | "asin" | "acos" | "atan" | "floor" | "ceil" | "round" | "trunc"
            | "fract" | "is_nan" | "is_finite" | "is_infinite" => {
                exact_args = Some(1);
                expect_numeric = true;
            }
            _ => return None,
        }

        if let Some(count) = exact_args {
            if args.len() != count {
                self.errors.push(Diagnostic::error(
                    "E2002",
                    format!(
                        "argument count mismatch: function expects {} arguments, but {} were provided",
                        count,
                        args.len()
                    ),
                    span,
                ));
            }
        } else if args.len() < min_args {
            self.errors.push(Diagnostic::error(
                "E2002",
                format!(
                    "argument count mismatch: function expects at least {} argument{}, but {} were provided",
                    min_args,
                    if min_args == 1 { "" } else { "s" },
                    args.len()
                ),
                span,
            ));
        }

        for (idx, arg) in args.iter().enumerate() {
            let expected = if expect_numeric {
                if allow_numeric_list_overload {
                    if q_last_numeric {
                        if idx == 0 {
                            if args.len() == 2 {
                                None
                            } else {
                                Some(&Type::F64)
                            }
                        } else {
                            Some(&Type::F64)
                        }
                    } else if args.len() == 1 {
                        None
                    } else {
                        Some(&Type::F64)
                    }
                } else {
                    Some(&Type::F64)
                }
            } else {
                None
            };
            if let Some(hir_arg) = self.analyse_expr(arg, expected) {
                if expect_numeric
                    && !hir_arg.ty().is_numeric()
                    && hir_arg.ty() != &Type::Error
                    && !(allow_numeric_list_overload
                        && idx == 0
                        && Self::is_numeric_list_type(hir_arg.ty()))
                {
                    self.errors.push(Diagnostic::error(
                        "E2003",
                        format!(
                            "argument type mismatch: function `{}` expects numeric arguments, found `{}`",
                            name,
                            hir_arg.ty()
                        ),
                        hir_arg.span(),
                    ));
                }
                hir_args.push(hir_arg);
            }
        }

        if allow_numeric_list_overload {
            if q_last_numeric {
                if args.len() == 2 && !hir_args.is_empty() {
                    let first_ok = hir_args[0].ty().is_numeric()
                        || Self::is_numeric_list_type(hir_args[0].ty());
                    if !first_ok {
                        self.errors.push(Diagnostic::error(
                            "E2003",
                            format!(
                                "argument type mismatch: function `{}` expects first argument as a numeric list or number",
                                name
                            ),
                            hir_args[0].span(),
                        ));
                    }
                }
            } else if args.len() == 1 && !hir_args.is_empty() {
                let only_ok =
                    hir_args[0].ty().is_numeric() || Self::is_numeric_list_type(hir_args[0].ty());
                if !only_ok {
                    self.errors.push(Diagnostic::error(
                        "E2003",
                        format!(
                            "argument type mismatch: function `{}` expects a numeric value or numeric list",
                            name
                        ),
                        hir_args[0].span(),
                    ));
                }
            }
        }

        if name == "append" && hir_args.len() == 2 {
            if !Self::is_numeric_list_type(hir_args[0].ty()) {
                self.errors.push(Diagnostic::error(
                    "E2003",
                    "argument type mismatch: function `append` expects first argument as numeric list",
                    hir_args[0].span(),
                ));
            }
            if !hir_args[1].ty().is_numeric() && hir_args[1].ty() != &Type::Error {
                self.errors.push(Diagnostic::error(
                    "E2003",
                    format!(
                        "argument type mismatch: function `append` expects numeric value, found `{}`",
                        hir_args[1].ty()
                    ),
                    hir_args[1].span(),
                ));
            }
        }

        if name == "len" && hir_args.len() == 1 && !matches!(hir_args[0].ty(), Type::List(_)) {
            self.errors.push(Diagnostic::error(
                "E2003",
                format!(
                    "argument type mismatch: function `len` expects a list value, found `{}`",
                    hir_args[0].ty()
                ),
                hir_args[0].span(),
            ));
        }

        if (name == "get" || name == "get_json")
            && hir_args.len() == 2
            && hir_args[1].ty() != &Type::String
            && hir_args[1].ty() != &Type::Error
        {
            self.errors.push(Diagnostic::error(
                "E2003",
                format!(
                    "argument type mismatch: function `{}` expects second argument as string, found `{}`",
                    name, hir_args[1].ty()
                ),
                hir_args[1].span(),
            ));
        }
        if (name == "post" || name == "post_json") && hir_args.len() == 3 {
            if hir_args[1].ty() != &Type::String && hir_args[1].ty() != &Type::Error {
                self.errors.push(Diagnostic::error(
                    "E2003",
                    format!(
                        "argument type mismatch: function `{}` expects second argument as string, found `{}`",
                        name, hir_args[1].ty()
                    ),
                    hir_args[1].span(),
                ));
            }
            if hir_args[2].ty() != &Type::String && hir_args[2].ty() != &Type::Error {
                self.errors.push(Diagnostic::error(
                    "E2003",
                    format!(
                        "argument type mismatch: function `{}` expects third argument as string, found `{}`",
                        name, hir_args[2].ty()
                    ),
                    hir_args[2].span(),
                ));
            }
        }

        let ret_ty = match name {
            "print" | "append" => Type::Void,
            "len" => Type::I64,
            "is_nan" | "is_finite" | "is_infinite" => Type::Bool,
            "get" | "post" | "get_json" | "post_json" => Type::String,
            // `mode` currently lowers to NaN when no mode exists.
            "mode" => Type::F64,
            _ => Type::F64,
        };

        Some(HirExpr::Call {
            callee: Box::new(hir_callee.clone()),
            args: hir_args,
            ty: ret_ty,
            span,
        })
    }

    fn is_numeric_list_type(ty: &Type) -> bool {
        match ty {
            Type::List(inner) => inner.is_numeric() || inner.as_ref() == &Type::Unknown,
            _ => false,
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
    fn test_numeric_builtins_valid() {
        let src = "fn main() -> i32:\n    let x = sqrt(9.0)\n    let y = pow(2.0, 8.0)\n    let z = mean(1.0, 2.0, 3.0, 4.0)\n    print(x, y, z)\n    return 0\n";
        assert!(check_source(src).is_ok());
    }

    #[test]
    fn test_numeric_builtin_type_mismatch() {
        let src = "fn main() -> i32:\n    let x = sqrt(\"nope\")\n    return 0\n";
        let errs = check_source(src).unwrap_err();
        assert!(errs.iter().any(|e| e.code == "E2003"));
    }

    #[test]
    fn test_stats_builtin_list_overload() {
        let src = "fn main() -> i32:\n    let m = mean([1.0, 2.0, 3.0, 4.0])\n    let q = quantile([1.0, 2.0, 3.0, 4.0], 0.5)\n    let p = percentile([1.0, 2.0, 3.0, 4.0], 50.0)\n    print(m, q, p)\n    return 0\n";
        assert!(check_source(src).is_ok());
    }

    #[test]
    fn test_stats_builtin_named_list_overload() {
        let src = "fn main() -> i32:\n    let xs = [1.0, 2.0, 3.0, 4.0]\n    let m = mean(xs)\n    let q = quantile(xs, 0.5)\n    let p = percentile(xs, 50.0)\n    print(m, q, p)\n    return 0\n";
        assert!(check_source(src).is_ok());
    }

    #[test]
    fn test_stats_builtin_list_type_mismatch() {
        let src =
            "fn main() -> i32:\n    let xs = [\"a\", \"b\"]\n    let m = mean(xs)\n    return 0\n";
        let errs = check_source(src).unwrap_err();
        assert!(errs.iter().any(|e| e.code == "E2003"));
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
        assert!(check_source(src).is_ok());
    }

    #[test]
    fn test_variadic_print() {
        let src = "fn main() -> i32:\n    print(\"value\", 10, true)\n    return 0\n";
        assert!(check_source(src).is_ok());
    }

    #[test]
    fn test_list_index_and_comparison() {
        let src = "fn main() -> i32:\n    let values = [1.0, 2.0, 3.0]\n    let ok: bool = values[0] < values[1]\n    return 0\n";
        assert!(check_source(src).is_ok());
    }

    #[test]
    fn test_mutable_list_index_assignment() {
        let src = "fn main() -> i32:\n    let values = [1.0, 2.0, 3.0]\n    values[0] = 9.0\n    return 0\n";
        assert!(check_source(src).is_ok());
    }

    #[test]
    fn test_list_append_and_len() {
        let src = "fn main() -> i32:\n    let values = [1.0, 2.0, 3.0]\n    append(values, 4.0)\n    let n: i64 = len(values)\n    print(n)\n    return 0\n";
        assert!(check_source(src).is_ok());
    }

    #[test]
    fn test_immutable_assignment_rejected() {
        let src = "fn main() -> i32:\n    const values = [1.0, 2.0, 3.0]\n    values[0] = 9.0\n    return 0\n";
        let errs = check_source(src).unwrap_err();
        assert!(errs.iter().any(|e| e.code == "E2010"));
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
