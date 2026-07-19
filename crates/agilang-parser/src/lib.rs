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
            match self.function() {
                Some(f) => functions.push(f),
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
    fn function(&mut self) -> Option<Function> {
        let start = self
            .expect(&TokenKind::Fn, "E100", "expected `fn`")?
            .span
            .start;
        let (name, _) = self.identifier("expected function name")?;
        self.expect(&TokenKind::LParen, "E101", "expected `(`")?;
        let mut params = vec![];
        if !self.at(&TokenKind::RParen) {
            loop {
                let (n, s) = self.identifier("expected parameter name")?;
                let ty = if self.eat(&TokenKind::Colon).is_some() {
                    Some(self.type_ref()?)
                } else {
                    None
                };
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
        self.line_end();
        Some(Stmt::Expr(expr))
    }
    fn expression(&mut self) -> Option<Expr> {
        self.binary(0)
    }
    fn binary(&mut self, min: u8) -> Option<Expr> {
        let mut left = self.primary()?;
        loop {
            let (op, p) = match &self.current().kind {
                TokenKind::Plus => (BinaryOp::Add, 1),
                TokenKind::Minus => (BinaryOp::Subtract, 1),
                TokenKind::Star => (BinaryOp::Multiply, 2),
                TokenKind::Slash => (BinaryOp::Divide, 2),
                _ => break,
            };
            if p < min {
                break;
            }
            self.advance();
            let right = self.binary(p + 1)?;
            let span = left.span().join(right.span());
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
                span,
            };
        }
        Some(left)
    }
    fn primary(&mut self) -> Option<Expr> {
        let t = self.advance().clone();
        let mut e = match t.kind {
            TokenKind::Identifier(n) => Expr::Identifier(n, t.span),
            TokenKind::Integer(v) => Expr::Integer(v, t.span),
            TokenKind::Float(v) => Expr::Float(v, t.span),
            TokenKind::String(v) => Expr::String(v, t.span),
            TokenKind::True => Expr::Bool(true, t.span),
            TokenKind::False => Expr::Bool(false, t.span),
            TokenKind::LParen => {
                let x = self.expression()?;
                self.expect(&TokenKind::RParen, "E121", "expected `)`")?;
                x
            }
            _ => {
                self.errors
                    .push(Diagnostic::error("E120", "expected expression", t.span));
                return None;
            }
        };
        while self.eat(&TokenKind::LParen).is_some() {
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
            let span = Span::new(e.span().start, end);
            e = Expr::Call {
                callee: Box::new(e),
                args,
                span,
            };
        }
        Some(e)
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
        while !self.at(&TokenKind::Eof) && !self.at(&TokenKind::Fn) {
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
}
