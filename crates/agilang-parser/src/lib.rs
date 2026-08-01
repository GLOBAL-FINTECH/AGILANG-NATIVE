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
        let mut functions = vec![];
        self.skip_newlines();
        while !self.at(&TokenKind::Eof) {
            match self.item() {
                Some(mut parsed) => functions.append(&mut parsed),
                None => self.synchronize(),
            }
            self.skip_newlines();
        }
        if self.errors.is_empty() {
            Ok(Program { functions })
        } else {
            Err(self.errors)
        }
    }
    fn item(&mut self) -> Option<Vec<Function>> {
        if self.at(&TokenKind::Module) {
            self.skip_until_newline();
            self.eat(&TokenKind::Newline);
            return Some(vec![]);
        }
        if self.at(&TokenKind::Use) {
            self.skip_until_newline();
            self.eat(&TokenKind::Newline);
            return Some(vec![]);
        }
        if self.at(&TokenKind::Class) {
            return self.class_item();
        }
        self.function().map(|f| vec![f])
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
        if self.at(&TokenKind::Else) {
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
        | Stmt::If { span, .. }
        | Stmt::ForIn { span, .. } => span.end,
        Stmt::Expr(expr) => expr.span().end,
    }
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
        assert_eq!(p.functions[0].name, "main");
        assert_eq!(p.functions[0].body.len(), 2);
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
    fn parses_object_literal() {
        let s = SourceFile::new(
            "x.agi",
            "fn main() -> i32:\n    let request = {\"method\": \"GET\", \"timeout_ms\": 1000}\n    return 0\n",
        );
        let p = parse(&lex(&s).unwrap()).unwrap();
        assert_eq!(p.functions[0].body.len(), 2);
    }
}
