//! Recursive-descent parser for native AGILANG bootstrap syntax.
use agilang_ast::*;
use agilang_diagnostics::Diagnostic;
use agilang_lexer::{Token, TokenKind};
use agilang_source::Span;

pub fn parse(tokens: &[Token]) -> Result<Program, Vec<Diagnostic>> {
    Parser {
        tokens,
        cursor: 0,
        errors: vec![],
    }
    .program()
}
struct Parser<'a> {
    tokens: &'a [Token],
    cursor: usize,
    errors: Vec<Diagnostic>,
}
impl<'a> Parser<'a> {
    fn program(mut self) -> Result<Program, Vec<Diagnostic>> {
        let mut module_name = None;
        let mut imports = vec![];
        let mut structs = vec![];
        let mut enums = vec![];
        let mut functions = vec![];
        self.skip_newlines();
        while !self.at(&TokenKind::Eof) {
            match self.item() {
                Some(Item::Module(module_decl)) => {
                    if module_name.is_some() {
                        self.errors.push(Diagnostic::error(
                            "E157",
                            "duplicate module declaration",
                            module_decl.span,
                        ));
                    } else {
                        module_name = Some(module_decl);
                    }
                }
                Some(Item::Import(import_decl)) => imports.push(import_decl),
                Some(Item::Struct(struct_decl)) => structs.push(struct_decl),
                Some(Item::Enum(enum_decl)) => enums.push(enum_decl),
                Some(Item::Functions(mut parsed)) => functions.append(&mut parsed),
                None => self.synchronize(),
            }
            self.skip_newlines();
        }
        if self.errors.is_empty() {
            Ok(Program {
                module_name,
                imports,
                structs,
                enums,
                functions,
            })
        } else {
            Err(self.errors)
        }
    }
    fn item(&mut self) -> Option<Item> {
        if self.at(&TokenKind::Module) {
            return self.module_item();
        }
        if self.at(&TokenKind::Import) || self.at(&TokenKind::Use) {
            return self.import_item();
        }
        if self.at(&TokenKind::Struct) {
            return self.struct_item();
        }
        if self.at(&TokenKind::Enum) {
            return self.enum_item();
        }
        if self.at(&TokenKind::Class) {
            return self.class_item().map(Item::Functions);
        }
        self.function().map(|f| Item::Functions(vec![f]))
    }
    fn module_item(&mut self) -> Option<Item> {
        let start = self.expect(&TokenKind::Module, "E158", "expected `module`")?.span.start;
        let (path, end) = self.module_path("expected module path")?;
        self.line_end();
        Some(Item::Module(ModuleDecl {
            path,
            span: Span::new(start, end),
        }))
    }
    fn import_item(&mut self) -> Option<Item> {
        let start = self.advance().span.start;
        let (path, end) = self.module_path("expected import path")?;
        self.line_end();
        Some(Item::Import(ImportDecl {
            path,
            span: Span::new(start, end),
        }))
    }
    fn struct_item(&mut self) -> Option<Item> {
        let start = self.expect(&TokenKind::Struct, "E159", "expected `struct`")?.span.start;
        let (name, _) = self.identifier("expected struct name")?;
        self.expect(&TokenKind::Colon, "E160", "expected `:`")?;
        self.expect(&TokenKind::Newline, "E161", "expected newline")?;
        self.expect(&TokenKind::Indent, "E162", "expected indented struct body")?;
        let mut fields = vec![];
        while !self.at(&TokenKind::Dedent) && !self.at(&TokenKind::Eof) {
            if self.eat(&TokenKind::Newline).is_some() {
                continue;
            }
            let (field_name, field_span) = self.identifier("expected field name")?;
            self.expect(&TokenKind::Colon, "E163", "expected `:` after field name")?;
            let ty = self.type_ref()?;
            self.line_end();
            fields.push(StructField {
                name: field_name,
                ty,
                span: field_span,
            });
        }
        let end = self
            .expect(&TokenKind::Dedent, "E164", "expected end of struct body")
            .map(|t| t.span.end)
            .unwrap_or(start);
        Some(Item::Struct(StructDecl {
            name,
            fields,
            span: Span::new(start, end),
        }))
    }
    fn enum_item(&mut self) -> Option<Item> {
        let start = self.expect(&TokenKind::Enum, "E165", "expected `enum`")?.span.start;
        let (name, _) = self.identifier("expected enum name")?;
        self.expect(&TokenKind::Colon, "E166", "expected `:`")?;
        self.expect(&TokenKind::Newline, "E167", "expected newline")?;
        self.expect(&TokenKind::Indent, "E168", "expected indented enum body")?;
        let mut variants = vec![];
        while !self.at(&TokenKind::Dedent) && !self.at(&TokenKind::Eof) {
            if self.eat(&TokenKind::Newline).is_some() {
                continue;
            }
            let (variant_name, variant_span) = self.identifier("expected enum variant")?;
            self.line_end();
            variants.push(EnumVariant {
                name: variant_name,
                span: variant_span,
            });
        }
        let end = self
            .expect(&TokenKind::Dedent, "E169", "expected end of enum body")
            .map(|t| t.span.end)
            .unwrap_or(start);
        Some(Item::Enum(EnumDecl {
            name,
            variants,
            span: Span::new(start, end),
        }))
    }
    fn class_item(&mut self) -> Option<Vec<Function>> {
        self.expect(&TokenKind::Class, "E128", "expected `class`")?;
        let (class_name, class_span) = self.identifier("expected class name")?;
        if self.eat(&TokenKind::Extends).is_some() {
            self.identifier("expected base class name")?;
        }
        self.expect(&TokenKind::Colon, "E129", "expected `:`")?;
        self.expect(&TokenKind::Newline, "E130", "expected newline")?;
        self.expect(&TokenKind::Indent, "E131", "expected indented class body")?;
        let mut functions = vec![];
        while !self.at(&TokenKind::Dedent) && !self.at(&TokenKind::Eof) {
            if self.eat(&TokenKind::Newline).is_some() {
                continue;
            }
            if self.at(&TokenKind::Let) || self.at(&TokenKind::Const) {
                self.skip_until_newline();
                self.eat(&TokenKind::Newline);
                continue;
            }
            if self.at(&TokenKind::Fn) {
                if let Some(function) = self.class_function(&class_name, class_span) {
                    functions.push(function);
                } else {
                    self.synchronize_line();
                }
                continue;
            }
            self.synchronize_line();
        }
        self.expect(&TokenKind::Dedent, "E132", "expected end of class body")?;
        Some(functions)
    }
    fn function(&mut self) -> Option<Function> {
        self.function_with_implicit_self(None)
    }
    fn class_function(&mut self, class_name: &str, class_span: Span) -> Option<Function> {
        self.function_with_implicit_self(Some((class_name.to_string(), class_span)))
    }
    fn function_with_implicit_self(
        &mut self,
        implicit_self: Option<(String, Span)>,
    ) -> Option<Function> {
        let start = self
            .expect(&TokenKind::Fn, "E100", "expected `fn`")?
            .span
            .start;
        let (name, _) = self.identifier("expected function name")?;
        self.expect(&TokenKind::LParen, "E101", "expected `(`")?;
        let mut params = vec![];
        if let Some((class_name, class_span)) = implicit_self.clone() {
            params.push(Parameter {
                name: "self".to_string(),
                ty: Some(TypeRef {
                    name: class_name,
                    span: class_span,
                }),
                span: class_span,
            });
        }
        if !self.at(&TokenKind::RParen) {
            loop {
                let (n, s) = self.identifier("expected parameter name")?;
                let ty = if self.eat(&TokenKind::Colon).is_some() {
                    Some(self.type_ref()?)
                } else {
                    None
                };
                if self.eat(&TokenKind::Equal).is_some() {
                    self.expression()?;
                }
                params.push(Parameter {
                    name: n,
                    ty,
                    span: s,
                });
                if self.eat(&TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RParen, "E102", "expected `)`")?;
        let return_type = if self.eat(&TokenKind::Arrow).is_some() {
            Some(self.type_ref()?)
        } else {
            None
        };
        self.expect(&TokenKind::Colon, "E103", "expected `:`")?;
        self.expect(&TokenKind::Newline, "E104", "expected newline")?;
        self.expect(
            &TokenKind::Indent,
            "E105",
            "expected indented function body",
        )?;
        let mut body = vec![];
        while !self.at(&TokenKind::Dedent) && !self.at(&TokenKind::Eof) {
            if self.eat(&TokenKind::Newline).is_some() {
                continue;
            }
            if let Some(s) = self.statement() {
                body.push(s)
            } else {
                self.synchronize_line();
            }
        }
        let end = self
            .expect(&TokenKind::Dedent, "E106", "expected end of function body")
            .map(|t| t.span.end)
            .unwrap_or(start);
        Some(Function {
            name,
            params,
            return_type,
            body,
            span: Span::new(start, end),
        })
    }
    fn statement(&mut self) -> Option<Stmt> {
        if self.at(&TokenKind::If) {
            return self.if_statement();
        }
        if self.at(&TokenKind::While) {
            return self.while_statement();
        }
        if self.at(&TokenKind::For) {
            return self.for_in_statement();
        }
        if self.at(&TokenKind::Throw) {
            let start = self.advance().span.start;
            let value = self.expression()?;
            let span = Span::new(start, value.span().end);
            self.line_end();
            return Some(Stmt::Expr(Expr::Call {
                callee: Box::new(Expr::Identifier("throw".to_string(), Span::new(start, start + 5))),
                args: vec![value],
                span,
            }));
        }
        if self.at(&TokenKind::Let) || self.at(&TokenKind::Const) {
            let mutable = self.at(&TokenKind::Let);
            let start = self.advance().span.start;
            let (name, _) = self.identifier("expected variable name")?;
            let ty = if self.eat(&TokenKind::Colon).is_some() {
                Some(self.type_ref()?)
            } else {
                None
            };
            self.expect(&TokenKind::Equal, "E110", "expected `=`")?;
            let value = self.expression()?;
            let end = value.span().end;
            self.line_end();
            return Some(Stmt::Let {
                name,
                ty,
                value,
                mutable,
                span: Span::new(start, end),
            });
        }
        if self.at(&TokenKind::Return) {
            let start = self.advance().span.start;
            let value = if self.at(&TokenKind::Newline) {
                None
            } else {
                Some(self.expression()?)
            };
            let end = value
                .as_ref()
                .map(Expr::span)
                .map(|s| s.end)
                .unwrap_or(start + 6);
            self.line_end();
            return Some(Stmt::Return {
                value,
                span: Span::new(start, end),
            });
        }
        if self.at(&TokenKind::Break) {
            let token = self.advance().clone();
            self.line_end();
            return Some(Stmt::Break { span: token.span });
        }
        if self.at(&TokenKind::Continue) {
            let token = self.advance().clone();
            self.line_end();
            return Some(Stmt::Continue { span: token.span });
        }
        let expr = self.expression()?;
        if self.eat(&TokenKind::Equal).is_some() {
            let value = self.expression()?;
            let span = expr.span().join(value.span());
            self.line_end();
            return Some(Stmt::Assign {
                target: expr,
                value,
                span,
            });
        }
        if let Some(op) = self.compound_assignment_op() {
            let value = self.expression()?;
            let span = expr.span().join(value.span());
            self.line_end();
            return Some(Stmt::Assign {
                target: expr.clone(),
                value: Expr::Binary {
                    left: Box::new(expr),
                    op,
                    right: Box::new(value),
                    span,
                },
                span,
            });
        }
        self.line_end();
        Some(Stmt::Expr(expr))
    }
    fn if_statement(&mut self) -> Option<Stmt> {
        let start = self.expect(&TokenKind::If, "E133", "expected `if`")?.span.start;
        let condition = self.expression()?;
        self.expect(&TokenKind::Colon, "E134", "expected `:`")?;
        self.expect(&TokenKind::Newline, "E135", "expected newline")?;
        self.expect(&TokenKind::Indent, "E136", "expected indented if body")?;
        let then_body = self.block_statements()?;
        let mut else_body = vec![];
        let mut end = condition.span().end;
        if self.at(&TokenKind::Elif) {
            let elif_stmt = self.elif_clause()?;
            end = stmt_span_end(&elif_stmt);
            else_body.push(elif_stmt);
        } else if self.at(&TokenKind::Else) {
            self.advance();
            self.expect(&TokenKind::Colon, "E137", "expected `:`")?;
            self.expect(&TokenKind::Newline, "E138", "expected newline")?;
            self.expect(&TokenKind::Indent, "E139", "expected indented else body")?;
            else_body = self.block_statements()?;
            end = else_body
                .last()
                .map(stmt_span_end)
                .unwrap_or(end);
        } else {
            end = then_body.last().map(stmt_span_end).unwrap_or(end);
        }
        Some(Stmt::If {
            condition,
            then_body,
            else_body,
            span: Span::new(start, end),
        })
    }
    fn for_in_statement(&mut self) -> Option<Stmt> {
        let start = self.expect(&TokenKind::For, "E140", "expected `for`")?.span.start;
        let (first_name, _) = self.identifier("expected loop variable")?;
        let mut second_name = None;
        if self.eat(&TokenKind::Comma).is_some() {
            let (value_name, _) = self.identifier("expected loop value variable")?;
            second_name = Some(value_name);
        }
        self.expect(&TokenKind::In, "E141", "expected `in`")?;
        let iterable = self.expression()?;
        self.expect(&TokenKind::Colon, "E142", "expected `:`")?;
        self.expect(&TokenKind::Newline, "E143", "expected newline")?;
        self.expect(&TokenKind::Indent, "E144", "expected indented loop body")?;
        let body = self.block_statements()?;
        let end = body.last().map(stmt_span_end).unwrap_or(iterable.span().end);
        Some(Stmt::ForIn {
            key_name: first_name,
            value_name: second_name,
            iterable,
            body,
            span: Span::new(start, end),
        })
    }
    fn elif_clause(&mut self) -> Option<Stmt> {
        let start = self.expect(&TokenKind::Elif, "E150", "expected `elif`")?.span.start;
        let condition = self.expression()?;
        self.expect(&TokenKind::Colon, "E151", "expected `:`")?;
        self.expect(&TokenKind::Newline, "E152", "expected newline")?;
        self.expect(&TokenKind::Indent, "E153", "expected indented elif body")?;
        let then_body = self.block_statements()?;
        let mut else_body = vec![];
        let mut end = then_body.last().map(stmt_span_end).unwrap_or(condition.span().end);
        if self.at(&TokenKind::Elif) {
            let nested = self.elif_clause()?;
            end = stmt_span_end(&nested);
            else_body.push(nested);
        } else if self.at(&TokenKind::Else) {
            self.advance();
            self.expect(&TokenKind::Colon, "E154", "expected `:`")?;
            self.expect(&TokenKind::Newline, "E155", "expected newline")?;
            self.expect(&TokenKind::Indent, "E156", "expected indented else body")?;
            else_body = self.block_statements()?;
            end = else_body.last().map(stmt_span_end).unwrap_or(end);
        }
        Some(Stmt::If {
            condition,
            then_body,
            else_body,
            span: Span::new(start, end),
        })
    }
    fn while_statement(&mut self) -> Option<Stmt> {
        let start = self
            .expect(&TokenKind::While, "E146", "expected `while`")?
            .span
            .start;
        let condition = self.expression()?;
        self.expect(&TokenKind::Colon, "E147", "expected `:`")?;
        self.expect(&TokenKind::Newline, "E148", "expected newline")?;
        self.expect(&TokenKind::Indent, "E149", "expected indented loop body")?;
        let body = self.block_statements()?;
        let end = body.last().map(stmt_span_end).unwrap_or(condition.span().end);
        Some(Stmt::While {
            condition,
            body,
            span: Span::new(start, end),
        })
    }
    fn block_statements(&mut self) -> Option<Vec<Stmt>> {
        let mut body = vec![];
        while !self.at(&TokenKind::Dedent) && !self.at(&TokenKind::Eof) {
            if self.eat(&TokenKind::Newline).is_some() {
                continue;
            }
            if let Some(stmt) = self.statement() {
                body.push(stmt);
            } else {
                self.synchronize_line();
            }
        }
        self.expect(&TokenKind::Dedent, "E145", "expected end of block")?;
        Some(body)
    }
    fn expression(&mut self) -> Option<Expr> {
        self.logical_or()
    }

    fn logical_or(&mut self) -> Option<Expr> {
        let mut left = self.logical_and()?;
        while self.eat(&TokenKind::Or).is_some() {
            let right = self.logical_and()?;
            let span = left.span().join(right.span());
            left = Expr::Binary {
                left: Box::new(left),
                op: BinaryOp::Or,
                right: Box::new(right),
                span,
            };
        }
        Some(left)
    }

    fn logical_and(&mut self) -> Option<Expr> {
        let mut left = self.comparison()?;
        while self.eat(&TokenKind::And).is_some() {
            let right = self.comparison()?;
            let span = left.span().join(right.span());
            left = Expr::Binary {
                left: Box::new(left),
                op: BinaryOp::And,
                right: Box::new(right),
                span,
            };
        }
        Some(left)
    }

    fn comparison(&mut self) -> Option<Expr> {
        let mut left = self.additive()?;
        loop {
            let op = if self.eat(&TokenKind::EqualEqual).is_some() {
                Some(BinaryOp::Equal)
            } else if self.eat(&TokenKind::BangEqual).is_some() {
                Some(BinaryOp::NotEqual)
            } else if self.eat(&TokenKind::LessEqual).is_some() {
                Some(BinaryOp::LessEqual)
            } else if self.eat(&TokenKind::GreaterEqual).is_some() {
                Some(BinaryOp::GreaterEqual)
            } else if self.eat(&TokenKind::Less).is_some() {
                Some(BinaryOp::Less)
            } else if self.eat(&TokenKind::Greater).is_some() {
                Some(BinaryOp::Greater)
            } else {
                None
            };

            if let Some(op) = op {
                let right = self.additive()?;
                let span = left.span().join(right.span());
                left = Expr::Binary {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                    span,
                };
            } else {
                break;
            }
        }
        Some(left)
    }

    fn additive(&mut self) -> Option<Expr> {
        let mut left = self.multiplicative()?;
        loop {
            let op = if self.eat(&TokenKind::Plus).is_some() {
                Some(BinaryOp::Add)
            } else if self.eat(&TokenKind::Minus).is_some() {
                Some(BinaryOp::Subtract)
            } else {
                None
            };
            if let Some(op) = op {
                let right = self.multiplicative()?;
                let span = left.span().join(right.span());
                left = Expr::Binary {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                    span,
                };
            } else {
                break;
            }
        }
        Some(left)
    }

    fn multiplicative(&mut self) -> Option<Expr> {
        let mut left = self.postfix()?;
        loop {
            let op = if self.eat(&TokenKind::Star).is_some() {
                Some(BinaryOp::Multiply)
            } else if self.eat(&TokenKind::Slash).is_some() {
                Some(BinaryOp::Divide)
            } else {
                None
            };
            if let Some(op) = op {
                let right = self.postfix()?;
                let span = left.span().join(right.span());
                left = Expr::Binary {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                    span,
                };
            } else {
                break;
            }
        }
        Some(left)
    }

    fn postfix(&mut self) -> Option<Expr> {
        let mut expr = self.primary()?;
        loop {
            if self.eat(&TokenKind::Dot).is_some() {
                let (member, member_span) = self.identifier("expected member name after `.`")?;
                let span = expr.span().join(member_span);
                expr = Expr::MemberAccess {
                    object: Box::new(expr),
                    member,
                    span,
                };
                continue;
            }

            if self.eat(&TokenKind::LBracket).is_some() {
                let index = self.expression()?;
                let end = self
                    .expect(&TokenKind::RBracket, "E123", "expected `]`")?
                    .span
                    .end;
                let span = Span::new(expr.span().start, end);
                expr = Expr::Index {
                    object: Box::new(expr),
                    index: Box::new(index),
                    span,
                };
                continue;
            }

            if self.eat(&TokenKind::LParen).is_some() {
                let mut args = vec![];
                if !self.at(&TokenKind::RParen) {
                    loop {
                        args.push(self.expression()?);
                        if self.eat(&TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                }
                let end = self
                    .expect(&TokenKind::RParen, "E122", "expected `)`")?
                    .span
                    .end;
                let span = Span::new(expr.span().start, end);
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                    span,
                };
                continue;
            }

            break;
        }
        Some(expr)
    }

    fn primary(&mut self) -> Option<Expr> {
        let t = self.advance().clone();
        match t.kind {
            TokenKind::Identifier(n) => Some(Expr::Identifier(n, t.span)),
            TokenKind::Integer(v) => Some(Expr::Integer(v, t.span)),
            TokenKind::Float(v) => Some(Expr::Float(v, t.span)),
            TokenKind::String(v) => Some(Expr::String(v, t.span)),
            TokenKind::True => Some(Expr::Bool(true, t.span)),
            TokenKind::False => Some(Expr::Bool(false, t.span)),
            TokenKind::LBracket => {
                let mut items = vec![];
                self.skip_container_layout();
                if !self.at(&TokenKind::RBracket) {
                    loop {
                        items.push(self.expression()?);
                        self.skip_container_layout();
                        if self.eat(&TokenKind::Comma).is_none() {
                            break;
                        }
                        self.skip_container_layout();
                    }
                }
                self.skip_container_layout();
                let end = self
                    .expect(&TokenKind::RBracket, "E124", "expected `]`")?
                    .span
                    .end;
                Some(Expr::ListLiteral(items, Span::new(t.span.start, end)))
            }
            TokenKind::LBrace => {
                let mut items = vec![];
                self.skip_container_layout();
                if !self.at(&TokenKind::RBrace) {
                    loop {
                        self.skip_container_layout();
                        let key_token = self.advance().clone();
                        let key = match key_token.kind {
                            TokenKind::String(value) | TokenKind::Identifier(value) => value,
                            _ => {
                                self.errors.push(Diagnostic::error(
                                    "E125",
                                    "expected object key",
                                    key_token.span,
                                ));
                                return None;
                            }
                        };
                        self.expect(&TokenKind::Colon, "E126", "expected `:` after object key")?;
                        let value = self.expression()?;
                        items.push((key, value));
                        self.skip_container_layout();
                        if self.eat(&TokenKind::Comma).is_none() {
                            break;
                        }
                        self.skip_container_layout();
                    }
                }
                self.skip_container_layout();
                let end = self
                    .expect(&TokenKind::RBrace, "E127", "expected `}`")?
                    .span
                    .end;
                Some(Expr::ObjectLiteral(items, Span::new(t.span.start, end)))
            }
            TokenKind::LParen => {
                let x = self.expression()?;
                self.expect(&TokenKind::RParen, "E121", "expected `)`")?;
                Some(x)
            }
            _ => {
                self.errors
                    .push(Diagnostic::error("E120", "expected expression", t.span));
                None
            }
        }
    }
    fn type_ref(&mut self) -> Option<TypeRef> {
        let (n, s) = self.identifier("expected type name")?;
        Some(TypeRef { name: n, span: s })
    }
    fn identifier(&mut self, msg: &str) -> Option<(String, Span)> {
        let t = self.advance().clone();
        if let TokenKind::Identifier(n) = t.kind {
            Some((n, t.span))
        } else {
            self.errors.push(Diagnostic::error("E107", msg, t.span));
            None
        }
    }
    fn module_path(&mut self, msg: &str) -> Option<(String, usize)> {
        let (mut path, first_span) = self.identifier(msg)?;
        let mut end = first_span.end;
        while self.eat(&TokenKind::Dot).is_some() {
            let (segment, span) = self.identifier("expected module path segment")?;
            path.push('.');
            path.push_str(&segment);
            end = span.end;
        }
        Some((path, end))
    }
    fn line_end(&mut self) {
        if self.eat(&TokenKind::Newline).is_none()
            && !self.at(&TokenKind::Dedent)
            && !self.at(&TokenKind::Eof)
        {
            let s = self.current().span;
            self.errors
                .push(Diagnostic::error("E108", "expected end of line", s));
            self.synchronize_line();
        }
    }
    fn synchronize(&mut self) {
        while !self.at(&TokenKind::Eof) && !self.at(&TokenKind::Fn) && !self.at(&TokenKind::Class)
        {
            self.advance();
        }
    }
    fn synchronize_line(&mut self) {
        while !self.at(&TokenKind::Eof)
            && !self.at(&TokenKind::Newline)
            && !self.at(&TokenKind::Dedent)
        {
            self.advance();
        }
        self.eat(&TokenKind::Newline);
    }
    fn skip_newlines(&mut self) {
        while self.eat(&TokenKind::Newline).is_some() {}
    }
    fn skip_until_newline(&mut self) {
        while !self.at(&TokenKind::Eof) && !self.at(&TokenKind::Newline) {
            self.advance();
        }
    }
    fn compound_assignment_op(&mut self) -> Option<BinaryOp> {
        if self.eat(&TokenKind::PlusEqual).is_some() {
            Some(BinaryOp::Add)
        } else if self.eat(&TokenKind::MinusEqual).is_some() {
            Some(BinaryOp::Subtract)
        } else if self.eat(&TokenKind::StarEqual).is_some() {
            Some(BinaryOp::Multiply)
        } else if self.eat(&TokenKind::SlashEqual).is_some() {
            Some(BinaryOp::Divide)
        } else {
            None
        }
    }
    fn skip_container_layout(&mut self) {
        loop {
            let consumed = self.eat(&TokenKind::Newline).is_some()
                || self.eat(&TokenKind::Indent).is_some()
                || self.eat(&TokenKind::Dedent).is_some();
            if !consumed {
                break;
            }
        }
    }
    fn expect(&mut self, k: &TokenKind, code: &'static str, msg: &str) -> Option<&Token> {
        if self.at(k) {
            Some(self.advance())
        } else {
            let s = self.current().span;
            self.errors.push(Diagnostic::error(code, msg, s));
            None
        }
    }
    fn eat(&mut self, k: &TokenKind) -> Option<&Token> {
        if self.at(k) {
            Some(self.advance())
        } else {
            None
        }
    }
    fn at(&self, k: &TokenKind) -> bool {
        std::mem::discriminant(&self.current().kind) == std::mem::discriminant(k)
    }
    fn current(&self) -> &Token {
        &self.tokens[self.cursor.min(self.tokens.len() - 1)]
    }
    fn advance(&mut self) -> &Token {
        let i = self.cursor;
        self.cursor = (self.cursor + 1).min(self.tokens.len());
        &self.tokens[i.min(self.tokens.len() - 1)]
    }
}

fn stmt_span_end(stmt: &Stmt) -> usize {
    match stmt {
        Stmt::Let { span, .. }
        | Stmt::Assign { span, .. }
        | Stmt::Return { span, .. }
        | Stmt::Break { span, .. }
        | Stmt::Continue { span, .. }
        | Stmt::If { span, .. }
        | Stmt::While { span, .. }
        | Stmt::ForIn { span, .. } => span.end,
        Stmt::Expr(expr) => expr.span().end,
    }
}

enum Item {
    Module(ModuleDecl),
    Import(ImportDecl),
    Struct(StructDecl),
    Enum(EnumDecl),
    Functions(Vec<Function>),
}
#[cfg(test)]
mod tests {
    use super::*;
    use agilang_lexer::lex;
    use agilang_source::SourceFile;
    #[test]
    fn parses_main() {
        let s = SourceFile::new(
            "x.agi",
            "fn main() -> i32:\n    let chain_id: i64 = 1990\n    return 0\n",
        );
        let p = parse(&lex(&s).unwrap()).unwrap();
        assert!(p.module_name.is_none());
        assert!(p.imports.is_empty());
        assert!(p.structs.is_empty());
        assert!(p.enums.is_empty());
        assert_eq!(p.functions[0].name, "main");
        assert_eq!(p.functions[0].body.len(), 2);
    }

    #[test]
    fn parses_module_and_imports() {
        let s = SourceFile::new(
            "x.agi",
            "module App.Controllers.InvoiceController\nimport App.Services.TaxService\nimport App.Models.Invoice\n\nfn main() -> i32:\n    return 0\n",
        );
        let p = parse(&lex(&s).unwrap()).unwrap();
        assert_eq!(p.module_name.as_ref().unwrap().path, "App.Controllers.InvoiceController");
        assert_eq!(p.imports.len(), 2);
        assert_eq!(p.imports[0].path, "App.Services.TaxService");
        assert_eq!(p.imports[1].path, "App.Models.Invoice");
    }

    #[test]
    fn parses_struct_declaration() {
        let s = SourceFile::new(
            "x.agi",
            "struct Point:\n    x: i32\n    y: i32\n\nfn main() -> i32:\n    return 0\n",
        );
        let p = parse(&lex(&s).unwrap()).unwrap();
        assert_eq!(p.structs.len(), 1);
        assert_eq!(p.structs[0].name, "Point");
        assert_eq!(p.structs[0].fields.len(), 2);
        assert_eq!(p.structs[0].fields[0].name, "x");
        assert_eq!(p.structs[0].fields[1].name, "y");
    }

    #[test]
    fn parses_enum_declaration() {
        let s = SourceFile::new(
            "x.agi",
            "enum Status:\n    Pending\n    Active\n    Suspended\n\nfn main() -> i32:\n    return 0\n",
        );
        let p = parse(&lex(&s).unwrap()).unwrap();
        assert_eq!(p.enums.len(), 1);
        assert_eq!(p.enums[0].name, "Status");
        assert_eq!(p.enums[0].variants.len(), 3);
        assert_eq!(p.enums[0].variants[1].name, "Active");
    }

    #[test]
    fn parses_postfix_member_index_and_call() {
        let s = SourceFile::new(
            "x.agi",
            "fn main() -> i32:\n    print(result.final_patient.history[0].to_string())\n    return 0\n",
        );
        let p = parse(&lex(&s).unwrap()).unwrap();
        assert_eq!(p.functions[0].body.len(), 2);
    }

    #[test]
    fn parses_list_literal_and_comparisons() {
        let s = SourceFile::new(
            "x.agi",
            "fn main() -> i32:\n    let values = [1.0, 2.0, 3.0]\n    print(values[0] == 1.0 and values[1] > 1.5)\n    return 0\n",
        );
        let p = parse(&lex(&s).unwrap()).unwrap();
        assert_eq!(p.functions[0].body.len(), 3);
    }

    #[test]
    fn parses_assignment_statement() {
        let s = SourceFile::new(
            "x.agi",
            "fn main() -> i32:\n    let values = [1.0, 2.0, 3.0]\n    values[0] = 9.0\n    return 0\n",
        );
        let p = parse(&lex(&s).unwrap()).unwrap();
        assert_eq!(p.functions[0].body.len(), 3);
        assert!(matches!(p.functions[0].body[1], Stmt::Assign { .. }));
    }

    #[test]
    fn parses_while_statement() {
        let s = SourceFile::new(
            "x.agi",
            "fn main() -> i32:\n    let value = 0\n    while value < 3:\n        print(value)\n        value = value + 1\n    return 0\n",
        );
        let p = parse(&lex(&s).unwrap()).unwrap();
        assert_eq!(p.functions[0].body.len(), 3);
        assert!(matches!(p.functions[0].body[1], Stmt::While { .. }));
    }

    #[test]
    fn parses_elif_statement() {
        let s = SourceFile::new(
            "x.agi",
            "fn main() -> i32:\n    let value = 1\n    if value == 0:\n        return 0\n    elif value == 1:\n        return 1\n    else:\n        return 2\n",
        );
        let p = parse(&lex(&s).unwrap()).unwrap();
        assert_eq!(p.functions[0].body.len(), 2);
        match &p.functions[0].body[1] {
            Stmt::If { else_body, .. } => {
                assert!(matches!(else_body.first(), Some(Stmt::If { .. })));
            }
            other => panic!("expected if statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_compound_assignment_statement() {
        let s = SourceFile::new(
            "x.agi",
            "fn main() -> i32:\n    let value = 1\n    value += 2\n    return value\n",
        );
        let p = parse(&lex(&s).unwrap()).unwrap();
        assert_eq!(p.functions[0].body.len(), 3);
        assert!(matches!(p.functions[0].body[1], Stmt::Assign { .. }));
    }

    #[test]
    fn parses_break_and_continue_statements() {
        let s = SourceFile::new(
            "x.agi",
            "fn main() -> i32:\n    while true:\n        continue\n        break\n    return 0\n",
        );
        let p = parse(&lex(&s).unwrap()).unwrap();
        match &p.functions[0].body[0] {
            Stmt::While { body, .. } => {
                assert!(matches!(body[0], Stmt::Continue { .. }));
                assert!(matches!(body[1], Stmt::Break { .. }));
            }
            other => panic!("expected while statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_object_literal() {
        let s = SourceFile::new(
            "x.agi",
            "fn main() -> i32:\n    let request = {\"method\": \"GET\", \"timeout_ms\": 1000}\n    return 0\n",
        );
        let p = parse(&lex(&s).unwrap()).unwrap();
        assert_eq!(p.functions[0].body.len(), 2);
    }
}
