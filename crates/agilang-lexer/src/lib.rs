//! Lexer for AGILANG source code.
//!
//! Tokenizes AGILANG source into a stream of tokens for parsing.

use agilang_diagnostics::{Diagnostic, Label, Severity};
use agilang_source::{SourceFile, Span};
use std::iter::Peekable;
use std::str::Chars;

/// Token types in AGILANG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenType {
    // Keywords
    Fn,
    Let,
    Const,
    Return,
    If,
    Else,
    While,
    True,
    False,

    // Literals
    Identifier,
    Integer,
    Float,
    String,

    // Operators and punctuation
    Colon,          // :
    Comma,          // ,
    LParen,         // (
    RParen,         // )
    Arrow,          // ->
    Equals,         // =
    Plus,           // +
    Minus,          // -
    Star,           // *
    Slash,          // /

    // Special
    Newline,
    Indent,
    Dedent,
    EOF,
}

impl TokenType {
    /// Check if this is a keyword token.
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            TokenType::Fn
                | TokenType::Let
                | TokenType::Const
                | TokenType::Return
                | TokenType::If
                | TokenType::Else
                | TokenType::While
                | TokenType::True
                | TokenType::False
        )
    }
}

/// A token produced by the lexer.
#[derive(Debug, Clone)]
pub struct Token {
    /// The type of token
    pub kind: TokenType,
    /// The span in source code
    pub span: Span,
    /// The literal text (for identifiers, literals, etc.)
    pub text: String,
}

impl Token {
    /// Create a new token.
    pub fn new(kind: TokenType, span: Span, text: String) -> Self {
        Self { kind, span, text }
    }

    /// Create a token with empty text.
    pub fn with_kind(kind: TokenType, span: Span) -> Self {
        Self {
            kind,
            span,
            text: String::new(),
        }
    }
}

/// Lexer for AGILANG source code.
pub struct Lexer<'a> {
    _source: &'a SourceFile,
    chars: Peekable<Chars<'a>>,
    offset: usize,
    indent_stack: Vec<usize>,
    diagnostics: Vec<Diagnostic>,
    buffered_tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer for a source file.
    pub fn new(source: &'a SourceFile) -> Self {
        let chars = source.as_str().chars().peekable();
        Self {
            _source: source,
            chars,
            offset: 0,
            indent_stack: vec![0],
            diagnostics: Vec::new(),
            buffered_tokens: Vec::new(),
        }
    }

    /// Get the current offset.
    pub fn current_offset(&self) -> usize {
        self.offset
    }

    /// Peek at the next character.
    fn peek(&mut self) -> Option<&char> {
        self.chars.peek()
    }

    /// Consume the next character.
    fn next(&mut self) -> Option<char> {
        let c = self.chars.next();
        if c.is_some() {
            self.offset += c.unwrap().len_utf8();
        }
        c
    }

    /// Check if the next character matches.
    fn matches(&mut self, expected: char) -> bool {
        if let Some(&c) = self.peek() {
            if c == expected {
                self.next();
                return true;
            }
        }
        false
    }

    /// Skip whitespace (but not newlines).
    fn skip_whitespace(&mut self) {
        while let Some(&c) = self.peek() {
            if c == ' ' || c == '\t' {
                self.next();
            } else {
                break;
            }
        }
    }



    /// Lex a string literal.
    fn lex_string(&mut self, start: usize) -> Result<Token, ()> {
        let mut text = String::new();
        // Opening quote already consumed by next_token

        while let Some(&c) = self.peek() {
            if c == '"' {
                self.next(); // Skip closing quote
                let end = self.offset;
                return Ok(Token::new(TokenType::String, Span::new(start, end), text));
            } else if c == '\\' {
                self.next(); // Skip backslash
                if let Some(escaped) = self.next() {
                    match escaped {
                        'n' => text.push('\n'),
                        't' => text.push('\t'),
                        'r' => text.push('\r'),
                        '\\' => text.push('\\'),
                        '"' => text.push('"'),
                        _ => {
                            text.push(escaped);
                        }
                    }
                }
            } else {
                text.push(c);
                self.next();
            }
        }

        // Unterminated string
        let span = Span::new(start, self.offset);
        self.diagnostics.push(
            Diagnostic::error("unterminated string literal")
                .with_label(Label::new(span))
                .with_code("L0001"),
        );
        Err(())
    }

    /// Lex a single-line comment.
    fn lex_comment(&mut self) {
        while let Some(&c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.next();
        }
    }

    /// Handle indentation changes.
    fn handle_indentation(&mut self, start: usize) -> Vec<Token> {
        let mut indent = 0;
        while let Some(&c) = self.peek() {
            if c == ' ' {
                indent += 1;
                self.next();
            } else if c == '\t' {
                indent += 4; // Assume 4-space tab
                self.next();
            } else {
                break;
            }
        }

        let current_indent = *self.indent_stack.last().unwrap_or(&0);
        let mut tokens = Vec::new();

        if indent > current_indent {
            self.indent_stack.push(indent);
            tokens.push(Token::with_kind(TokenType::Indent, Span::new(start, start)));
        } else if indent < current_indent {
            while let Some(&top) = self.indent_stack.last() {
                if top > indent {
                    self.indent_stack.pop();
                    tokens.push(Token::with_kind(TokenType::Dedent, Span::new(start, start)));
                } else {
                    break;
                }
            }
        }

        tokens
    }

    /// Lex the next token.
    pub fn next_token(&mut self) -> Token {
        // Return buffered tokens first
        if !self.buffered_tokens.is_empty() {
            return self.buffered_tokens.remove(0);
        }

        self.skip_whitespace();

        let start = self.offset;

        match self.next() {
            Some(c) => match c {
                '\n' => {
                    let newline_token = Token::with_kind(TokenType::Newline, Span::new(start, self.offset));
                    let indent_tokens = self.handle_indentation(self.offset);
                    if !indent_tokens.is_empty() {
                        // Buffer the indent tokens, return newline first
                        self.buffered_tokens.extend(indent_tokens);
                    }
                    newline_token
                }
                '\r' => {
                    // Skip CR in CRLF
                    if self.matches('\n') {
                        let newline_token = Token::with_kind(TokenType::Newline, Span::new(start, self.offset));
                        let indent_tokens = self.handle_indentation(self.offset);
                        if !indent_tokens.is_empty() {
                            self.buffered_tokens.extend(indent_tokens);
                        }
                        newline_token
                    } else {
                        Token::with_kind(TokenType::Newline, Span::new(start, self.offset))
                    }
                }
                ':' => Token::with_kind(TokenType::Colon, Span::new(start, self.offset)),
                ',' => Token::with_kind(TokenType::Comma, Span::new(start, self.offset)),
                '(' => Token::with_kind(TokenType::LParen, Span::new(start, self.offset)),
                ')' => Token::with_kind(TokenType::RParen, Span::new(start, self.offset)),
                '=' => Token::with_kind(TokenType::Equals, Span::new(start, self.offset)),
                '+' => Token::with_kind(TokenType::Plus, Span::new(start, self.offset)),
                '-' => {
                    if self.matches('>') {
                        Token::with_kind(TokenType::Arrow, Span::new(start, self.offset))
                    } else {
                        Token::with_kind(TokenType::Minus, Span::new(start, self.offset))
                    }
                }
                '*' => Token::with_kind(TokenType::Star, Span::new(start, self.offset)),
                '/' => {
                    if self.matches('/') {
                        self.lex_comment();
                        // Recursively get the next token after the comment
                        self.next_token()
                    } else {
                        Token::with_kind(TokenType::Slash, Span::new(start, self.offset))
                    }
                }
                '"' => {
                    // Don't call self.next() here - lex_string handles the quote
                    match self.lex_string(start) {
                        Ok(token) => token,
                        Err(_) => Token::with_kind(TokenType::String, Span::new(start, self.offset)),
                    }
                }
                c if c.is_ascii_digit() => {
                    // We already consumed the digit, need to include it in the number
                    let mut text = c.to_string();
                    let mut has_dot = false;

                    while let Some(&next_c) = self.peek() {
                        if next_c.is_ascii_digit() {
                            text.push(next_c);
                            self.next();
                        } else if next_c == '.' && !has_dot {
                            text.push(next_c);
                            has_dot = true;
                            self.next();
                        } else {
                            break;
                        }
                    }

                    let end = self.offset;
                    let span = Span::new(start, end);
                    let kind = if has_dot {
                        TokenType::Float
                    } else {
                        TokenType::Integer
                    };

                    Token::new(kind, span, text)
                }
                c if c.is_alphabetic() || c == '_' => {
                    // We already consumed the first character
                    let mut text = c.to_string();

                    while let Some(&next_c) = self.peek() {
                        if next_c.is_alphanumeric() || next_c == '_' {
                            text.push(next_c);
                            self.next();
                        } else {
                            break;
                        }
                    }

                    let end = self.offset;
                    let span = Span::new(start, end);

                    let kind = match text.as_str() {
                        "fn" => TokenType::Fn,
                        "let" => TokenType::Let,
                        "const" => TokenType::Const,
                        "return" => TokenType::Return,
                        "if" => TokenType::If,
                        "else" => TokenType::Else,
                        "while" => TokenType::While,
                        "true" => TokenType::True,
                        "false" => TokenType::False,
                        _ => TokenType::Identifier,
                    };

                    Token::new(kind, span, text)
                }
                _ => {
                    // Unknown character
                    let span = Span::new(start, self.offset);
                    self.diagnostics.push(
                        Diagnostic::error(format!("unexpected character '{}'", c))
                            .with_label(Label::new(span))
                            .with_code("L0002"),
                    );
                    Token::with_kind(TokenType::Identifier, span)
                }
            },
            None => {
                // Flush remaining dedent tokens before EOF
                if self.indent_stack.len() > 1 {
                    self.indent_stack.pop();
                    return Token::with_kind(TokenType::Dedent, Span::new(start, start));
                }
                Token::with_kind(TokenType::EOF, Span::new(start, start))
            }
        }
    }

    /// Get all diagnostics.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Check if there are any errors.
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == Severity::Error)
    }
}

/// Tokenize an entire source file.
pub fn tokenize(source: &SourceFile) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut lexer = Lexer::new(source);
    let mut tokens = Vec::new();

    loop {
        let token = lexer.next_token();
        if token.kind == TokenType::EOF {
            tokens.push(token);
            break;
        }
        tokens.push(token);
    }

    (tokens, lexer.diagnostics().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agilang_source::SourceFile;

    #[test]
    fn test_keywords() {
        let source = SourceFile::from_bytes("test.agi".into(), b"fn let const return if else while true false".to_vec()).unwrap();
        let (tokens, diags) = tokenize(&source);

        assert!(diags.is_empty());
        assert_eq!(tokens.len(), 10); // 9 keywords + EOF

        assert_eq!(tokens[0].kind, TokenType::Fn);
        assert_eq!(tokens[1].kind, TokenType::Let);
        assert_eq!(tokens[2].kind, TokenType::Const);
        assert_eq!(tokens[3].kind, TokenType::Return);
        assert_eq!(tokens[4].kind, TokenType::If);
        assert_eq!(tokens[5].kind, TokenType::Else);
        assert_eq!(tokens[6].kind, TokenType::While);
        assert_eq!(tokens[7].kind, TokenType::True);
        assert_eq!(tokens[8].kind, TokenType::False);
        assert_eq!(tokens[9].kind, TokenType::EOF);
    }

    #[test]
    fn test_identifiers() {
        let source = SourceFile::from_bytes("test.agi".into(), b"my_var x123 _private".to_vec()).unwrap();
        let (tokens, diags) = tokenize(&source);

        assert!(diags.is_empty());
        assert_eq!(tokens.len(), 4); // 3 identifiers + EOF

        assert_eq!(tokens[0].kind, TokenType::Identifier);
        assert_eq!(tokens[0].text, "my_var");
        assert_eq!(tokens[1].kind, TokenType::Identifier);
        assert_eq!(tokens[1].text, "x123");
        assert_eq!(tokens[2].kind, TokenType::Identifier);
        assert_eq!(tokens[2].text, "_private");
    }

    #[test]
    fn test_numbers() {
        let source = SourceFile::from_bytes("test.agi".into(), b"123 45.67".to_vec()).unwrap();
        let (tokens, diags) = tokenize(&source);

        assert!(diags.is_empty());
        assert_eq!(tokens.len(), 3); // 2 numbers + EOF

        assert_eq!(tokens[0].kind, TokenType::Integer);
        assert_eq!(tokens[0].text, "123");
        assert_eq!(tokens[1].kind, TokenType::Float);
        assert_eq!(tokens[1].text, "45.67");
    }

    #[test]
    fn test_strings() {
        let source = SourceFile::from_bytes("test.agi".into(), br#""hello" "world\n""#.to_vec()).unwrap();
        let (tokens, diags) = tokenize(&source);

        assert!(diags.is_empty());
        assert_eq!(tokens.len(), 3); // 2 strings + EOF

        assert_eq!(tokens[0].kind, TokenType::String);
        assert_eq!(tokens[0].text, "hello");
        assert_eq!(tokens[1].kind, TokenType::String);
        assert_eq!(tokens[1].text, "world\n");
    }

    #[test]
    fn test_operators() {
        let source = SourceFile::from_bytes("test.agi".into(), b":,()->=+-*/".to_vec()).unwrap();
        let (tokens, diags) = tokenize(&source);

        assert!(diags.is_empty());
        // Should have: :, ,, (, ), ->, =, +, -, *, /, EOF = 11 tokens
        assert_eq!(tokens.len(), 11);

        assert_eq!(tokens[0].kind, TokenType::Colon);
        assert_eq!(tokens[1].kind, TokenType::Comma);
        assert_eq!(tokens[2].kind, TokenType::LParen);
        assert_eq!(tokens[3].kind, TokenType::RParen);
        assert_eq!(tokens[4].kind, TokenType::Arrow);
        assert_eq!(tokens[5].kind, TokenType::Equals);
        assert_eq!(tokens[6].kind, TokenType::Plus);
        assert_eq!(tokens[7].kind, TokenType::Minus);
        assert_eq!(tokens[8].kind, TokenType::Star);
        assert_eq!(tokens[9].kind, TokenType::Slash);
        assert_eq!(tokens[10].kind, TokenType::EOF);
    }

    #[test]
    fn test_comments() {
        let source = SourceFile::from_bytes("test.agi".into(), b"fn main(): // comment\n    return 0".to_vec()).unwrap();
        let (tokens, diags) = tokenize(&source);

        assert!(diags.is_empty());
        // Should have: fn, main, (, ), :, newline, indent, return, 0, dedent, EOF
        assert!(tokens.iter().any(|t| t.kind == TokenType::Fn));
        assert!(tokens.iter().any(|t| t.kind == TokenType::Return));
        assert!(!tokens.iter().any(|t| t.text == "comment"));
    }

    #[test]
    fn test_indentation() {
        let source = SourceFile::from_bytes("test.agi".into(), b"fn main():\n    return 0".to_vec()).unwrap();
        let (tokens, diags) = tokenize(&source);

        assert!(diags.is_empty());
        assert!(tokens.iter().any(|t| t.kind == TokenType::Indent));
        assert!(tokens.iter().any(|t| t.kind == TokenType::Dedent));
    }

    #[test]
    fn test_function_declaration() {
        let source = SourceFile::from_bytes("test.agi".into(), b"fn main() -> i32:\n    return 0".to_vec()).unwrap();
        let (tokens, diags) = tokenize(&source);

        assert!(diags.is_empty());
        
        let expected_kinds = vec![
            TokenType::Fn,
            TokenType::Identifier,
            TokenType::LParen,
            TokenType::RParen,
            TokenType::Arrow,
            TokenType::Identifier,
            TokenType::Colon,
            TokenType::Newline,
            TokenType::Indent,
            TokenType::Return,
            TokenType::Integer,
            TokenType::Dedent,
            TokenType::EOF,
        ];

        for (i, expected) in expected_kinds.iter().enumerate() {
            if i < tokens.len() {
                assert_eq!(tokens[i].kind, *expected, "Token {}: expected {:?}, got {:?}", i, expected, tokens[i].kind);
            }
        }
    }
}
