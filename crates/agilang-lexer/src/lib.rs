//! Native indentation-aware lexer for the initial AGILANG compiler subset.

use agilang_diagnostics::Diagnostic;
use agilang_source::{SourceFile, Span};

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Fn,
    Module,
    Import,
    Use,
    Class,
    Extends,
    Let,
    Const,
    Return,
    If,
    Elif,
    Else,
    While,
    For,
    In,
    Break,
    Continue,
    Throw,
    True,
    False,
    And,
    Or,
    Not,
    Identifier(String),
    Integer(i64),
    Float(f64),
    String(String),
    Colon,
    Comma,
    Dot,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LParen,
    RParen,
    Arrow,
    Equal,
    PlusEqual,
    MinusEqual,
    StarEqual,
    SlashEqual,
    EqualEqual,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Plus,
    Minus,
    Star,
    Slash,
    Newline,
    Indent,
    Dedent,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

pub fn lex(source: &SourceFile) -> Result<Vec<Token>, Vec<Diagnostic>> {
    Lexer::new(source).run()
}

struct Lexer<'a> {
    source: &'a SourceFile,
    bytes: &'a [u8],
    cursor: usize,
    tokens: Vec<Token>,
    errors: Vec<Diagnostic>,
    indents: Vec<usize>,
    at_line_start: bool,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a SourceFile) -> Self {
        Self {
            source,
            bytes: source.text().as_bytes(),
            cursor: 0,
            tokens: Vec::new(),
            errors: Vec::new(),
            indents: vec![0],
            at_line_start: true,
        }
    }

    fn run(mut self) -> Result<Vec<Token>, Vec<Diagnostic>> {
        while self.cursor < self.bytes.len() {
            if self.at_line_start && self.scan_indent() {
                continue;
            }
            let start = self.cursor;
            match self.bytes[self.cursor] {
                b' ' | b'\t' | b'\r' => self.cursor += 1,
                b'\n' => {
                    self.cursor += 1;
                    self.push(TokenKind::Newline, start);
                    self.at_line_start = true;
                }
                b'#' => self.skip_comment(),
                b':' => self.single(TokenKind::Colon),
                b',' => self.single(TokenKind::Comma),
                b'.' => self.single(TokenKind::Dot),
                b'{' => self.single(TokenKind::LBrace),
                b'}' => self.single(TokenKind::RBrace),
                b'[' => self.single(TokenKind::LBracket),
                b']' => self.single(TokenKind::RBracket),
                b'(' => self.single(TokenKind::LParen),
                b')' => self.single(TokenKind::RParen),
                b'=' if self.peek(1) == Some(b'=') => {
                    self.cursor += 2;
                    self.tokens.push(Token {
                        kind: TokenKind::EqualEqual,
                        span: Span::new(start, self.cursor),
                    });
                }
                b'=' => self.single(TokenKind::Equal),
                b'+' if self.peek(1) == Some(b'=') => {
                    self.cursor += 2;
                    self.tokens.push(Token {
                        kind: TokenKind::PlusEqual,
                        span: Span::new(start, self.cursor),
                    });
                }
                b'!' if self.peek(1) == Some(b'=') => {
                    self.cursor += 2;
                    self.tokens.push(Token {
                        kind: TokenKind::BangEqual,
                        span: Span::new(start, self.cursor),
                    });
                }
                b'<' if self.peek(1) == Some(b'=') => {
                    self.cursor += 2;
                    self.tokens.push(Token {
                        kind: TokenKind::LessEqual,
                        span: Span::new(start, self.cursor),
                    });
                }
                b'>' if self.peek(1) == Some(b'=') => {
                    self.cursor += 2;
                    self.tokens.push(Token {
                        kind: TokenKind::GreaterEqual,
                        span: Span::new(start, self.cursor),
                    });
                }
                b'<' => self.single(TokenKind::Less),
                b'>' => self.single(TokenKind::Greater),
                b'+' => self.single(TokenKind::Plus),
                b'*' if self.peek(1) == Some(b'=') => {
                    self.cursor += 2;
                    self.tokens.push(Token {
                        kind: TokenKind::StarEqual,
                        span: Span::new(start, self.cursor),
                    });
                }
                b'*' => self.single(TokenKind::Star),
                b'/' if self.peek(1) == Some(b'=') => {
                    self.cursor += 2;
                    self.tokens.push(Token {
                        kind: TokenKind::SlashEqual,
                        span: Span::new(start, self.cursor),
                    });
                }
                b'/' => self.single(TokenKind::Slash),
                b'-' if self.peek(1) == Some(b'>') => {
                    self.cursor += 2;
                    self.tokens.push(Token {
                        kind: TokenKind::Arrow,
                        span: Span::new(start, self.cursor),
                    });
                }
                b'-' if self.peek(1) == Some(b'=') => {
                    self.cursor += 2;
                    self.tokens.push(Token {
                        kind: TokenKind::MinusEqual,
                        span: Span::new(start, self.cursor),
                    });
                }
                b'-' => self.single(TokenKind::Minus),
                b'"' | b'\'' => self.scan_string(),
                b'0'..=b'9' => self.scan_number(),
                b if is_ident_start(b) => self.scan_identifier(),
                _ => {
                    self.cursor += 1;
                    self.errors.push(Diagnostic::error(
                        "E001",
                        format!(
                            "unexpected character {:?}",
                            self.source
                                .slice(Span::new(start, self.cursor))
                                .unwrap_or("?")
                        ),
                        Span::new(start, self.cursor),
                    ));
                }
            }
        }
        if !matches!(
            self.tokens.last().map(|t| &t.kind),
            Some(TokenKind::Newline)
        ) {
            self.tokens.push(Token {
                kind: TokenKind::Newline,
                span: Span::empty(self.cursor),
            });
        }
        while self.indents.len() > 1 {
            self.indents.pop();
            self.tokens.push(Token {
                kind: TokenKind::Dedent,
                span: Span::empty(self.cursor),
            });
        }
        self.tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span::empty(self.cursor),
        });
        if self.errors.is_empty() {
            Ok(self.tokens)
        } else {
            Err(self.errors)
        }
    }

    fn scan_indent(&mut self) -> bool {
        let start = self.cursor;
        let mut width = 0;
        while let Some(b) = self.bytes.get(self.cursor).copied() {
            match b {
                b' ' => {
                    width += 1;
                    self.cursor += 1;
                }
                b'\t' => {
                    width += 4;
                    self.cursor += 1;
                }
                _ => break,
            }
        }
        if matches!(
            self.bytes.get(self.cursor),
            None | Some(b'\n') | Some(b'\r') | Some(b'#')
        ) {
            self.at_line_start = false;
            return false;
        }
        let current = *self.indents.last().unwrap();
        if width > current {
            self.indents.push(width);
            self.tokens.push(Token {
                kind: TokenKind::Indent,
                span: Span::new(start, self.cursor),
            });
        } else if width < current {
            while width < *self.indents.last().unwrap() {
                self.indents.pop();
                self.tokens.push(Token {
                    kind: TokenKind::Dedent,
                    span: Span::new(start, self.cursor),
                });
            }
            if width != *self.indents.last().unwrap() {
                self.errors.push(Diagnostic::error(
                    "E002",
                    "inconsistent indentation",
                    Span::new(start, self.cursor),
                ));
            }
        }
        self.at_line_start = false;
        false
    }

    fn scan_identifier(&mut self) {
        let start = self.cursor;
        self.cursor += 1;
        while self
            .bytes
            .get(self.cursor)
            .copied()
            .is_some_and(is_ident_continue)
        {
            self.cursor += 1;
        }
        let text = &self.source.text()[start..self.cursor];
        let kind = match text {
            "fn" => TokenKind::Fn,
            "module" => TokenKind::Module,
            "import" => TokenKind::Import,
            "use" => TokenKind::Use,
            "class" => TokenKind::Class,
            "extends" => TokenKind::Extends,
            "let" => TokenKind::Let,
            "const" => TokenKind::Const,
            "return" => TokenKind::Return,
            "if" => TokenKind::If,
            "elif" => TokenKind::Elif,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "throw" => TokenKind::Throw,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "not" => TokenKind::Not,
            _ => TokenKind::Identifier(text.to_owned()),
        };
        self.tokens.push(Token {
            kind,
            span: Span::new(start, self.cursor),
        });
    }

    fn scan_number(&mut self) {
        let start = self.cursor;
        while matches!(self.bytes.get(self.cursor), Some(b'0'..=b'9') | Some(b'_')) {
            self.cursor += 1;
        }
        let mut float = false;
        if self.bytes.get(self.cursor) == Some(&b'.')
            && matches!(self.bytes.get(self.cursor + 1), Some(b'0'..=b'9'))
        {
            float = true;
            self.cursor += 1;
            while matches!(self.bytes.get(self.cursor), Some(b'0'..=b'9') | Some(b'_')) {
                self.cursor += 1;
            }
        }
        let raw = self.source.text()[start..self.cursor].replace('_', "");
        if float {
            match raw.parse::<f64>() {
                Ok(value) => self.tokens.push(Token {
                    kind: TokenKind::Float(value),
                    span: Span::new(start, self.cursor),
                }),
                Err(_) => self.errors.push(Diagnostic::error(
                    "E003",
                    "invalid numeric literal",
                    Span::new(start, self.cursor),
                )),
            }
        } else {
            match raw.parse::<i64>() {
                Ok(value) => self.tokens.push(Token {
                    kind: TokenKind::Integer(value),
                    span: Span::new(start, self.cursor),
                }),
                Err(_) => self.errors.push(Diagnostic::error(
                    "E003",
                    "invalid numeric literal",
                    Span::new(start, self.cursor),
                )),
            }
        }
    }

    fn scan_string(&mut self) {
        let start = self.cursor;
        let quote = self.bytes[self.cursor];
        self.cursor += 1;
        let mut value = String::new();
        while let Some(&b) = self.bytes.get(self.cursor) {
            if b == quote {
                self.cursor += 1;
                self.tokens.push(Token {
                    kind: TokenKind::String(value),
                    span: Span::new(start, self.cursor),
                });
                return;
            }
            if b == b'\n' {
                break;
            }
            if b == b'\\' {
                self.cursor += 1;
                match self.bytes.get(self.cursor).copied() {
                    Some(b'n') => value.push('\n'),
                    Some(b'r') => value.push('\r'),
                    Some(b't') => value.push('\t'),
                    Some(b'\\') => value.push('\\'),
                    Some(b'"') => value.push('"'),
                    Some(b'\'') => value.push('\''),
                    Some(x) => value.push(x as char),
                    None => break,
                }
                self.cursor += 1;
            } else {
                value.push(b as char);
                self.cursor += 1;
            }
        }
        self.errors.push(Diagnostic::error(
            "E004",
            "unterminated string literal",
            Span::new(start, self.cursor),
        ));
    }
    fn skip_comment(&mut self) {
        while self.bytes.get(self.cursor).is_some_and(|b| *b != b'\n') {
            self.cursor += 1;
        }
    }
    fn single(&mut self, kind: TokenKind) {
        let start = self.cursor;
        self.cursor += 1;
        self.tokens.push(Token {
            kind,
            span: Span::new(start, self.cursor),
        });
    }
    fn push(&mut self, kind: TokenKind, start: usize) {
        self.tokens.push(Token {
            kind,
            span: Span::new(start, self.cursor),
        });
    }
    fn peek(&self, n: usize) -> Option<u8> {
        self.bytes.get(self.cursor + n).copied()
    }
}
fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}
fn is_ident_continue(b: u8) -> bool {
    is_ident_start(b) || b.is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn emits_indent_and_dedent() {
        let s = SourceFile::new("x.agi", "fn main():\n    return 0\n");
        let t = lex(&s).unwrap();
        assert!(t.iter().any(|x| x.kind == TokenKind::Indent));
        assert!(t.iter().any(|x| x.kind == TokenKind::Dedent));
    }

    #[test]
    fn lexes_modern_postfix_and_comparisons() {
        let s = SourceFile::new(
            "x.agi",
            "fn main() -> i32:\n    let ok = values[0].score >= 1.0 and values[1] != 0.0\n    return 0\n",
        );
        let t = lex(&s).unwrap();
        assert!(t.iter().any(|x| x.kind == TokenKind::LBracket));
        assert!(t.iter().any(|x| x.kind == TokenKind::RBracket));
        assert!(t.iter().any(|x| x.kind == TokenKind::Dot));
        assert!(t.iter().any(|x| x.kind == TokenKind::GreaterEqual));
        assert!(t.iter().any(|x| x.kind == TokenKind::BangEqual));
        assert!(t.iter().any(|x| x.kind == TokenKind::And));
    }

    #[test]
    fn lexes_elif_and_compound_assignments() {
        let s = SourceFile::new(
            "x.agi",
            "fn main() -> i32:\n    let value = 1\n    if value == 1:\n        value += 1\n    elif value == 2:\n        value -= 1\n    else:\n        value *= 2\n    value /= 2\n    return value\n",
        );
        let t = lex(&s).unwrap();
        assert!(t.iter().any(|x| x.kind == TokenKind::Elif));
        assert!(t.iter().any(|x| x.kind == TokenKind::PlusEqual));
        assert!(t.iter().any(|x| x.kind == TokenKind::MinusEqual));
        assert!(t.iter().any(|x| x.kind == TokenKind::StarEqual));
        assert!(t.iter().any(|x| x.kind == TokenKind::SlashEqual));
    }

    #[test]
    fn lexes_break_and_continue() {
        let s = SourceFile::new(
            "x.agi",
            "fn main() -> i32:\n    while true:\n        continue\n        break\n    return 0\n",
        );
        let t = lex(&s).unwrap();
        assert!(t.iter().any(|x| x.kind == TokenKind::Continue));
        assert!(t.iter().any(|x| x.kind == TokenKind::Break));
    }
}
