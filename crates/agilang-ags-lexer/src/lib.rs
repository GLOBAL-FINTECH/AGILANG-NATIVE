//! AGS Lexer - Tokenizes AGS source code
//!
//! Handles:
//! - @page, @fetch, and @live directives
//! - HTML templates with binding expressions {{ }}
//! - String literals, identifiers, numbers
//! - Line and column tracking

use std::iter::Peekable;
use std::str::Chars;

pub use crate::token::{Token, TokenWithLocation};

pub mod token;

pub struct Lexer<'a> {
    input: Peekable<Chars<'a>>,
    line: usize,
    column: usize,
    tokens: Vec<TokenWithLocation>,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Lexer {
            input: input.chars().peekable(),
            line: 1,
            column: 1,
            tokens: Vec::new(),
        }
    }

    pub fn tokenize(mut self) -> Result<Vec<TokenWithLocation>, String> {
        while self.peek().is_some() {
            self.skip_whitespace();

            if self.peek().is_none() {
                break;
            }

            match self.peek() {
                Some('@') => self.lex_directive()?,
                Some('<') => self.lex_html_tag()?,
                Some('>') => {
                    self.advance();
                    self.add_token(Token::HtmlTagEnd);
                }
                Some('{') => self.lex_expression_or_brace()?,
                Some('}') => self.lex_close_brace()?,
                Some('"') => self.lex_string()?,
                Some('/') => self.lex_forward_slash()?,
                Some('=') => {
                    self.advance();
                    self.add_token(Token::Assign);
                }
                Some('.') => {
                    self.advance();
                    self.add_token(Token::Dot);
                }
                Some(':') => {
                    self.advance();
                    self.add_token(Token::Colon);
                }
                Some(';') => {
                    self.advance();
                    self.add_token(Token::Semicolon);
                }
                Some(',') => {
                    self.advance();
                    self.add_token(Token::Comma);
                }
                Some('[') => {
                    self.advance();
                    self.add_token(Token::LeftBracket);
                }
                Some(']') => {
                    self.advance();
                    self.add_token(Token::RightBracket);
                }
                Some('(') => {
                    self.advance();
                    self.add_token(Token::LeftParen);
                }
                Some(')') => {
                    self.advance();
                    self.add_token(Token::RightParen);
                }
                Some(c) if c.is_ascii_digit() => self.lex_number()?,
                Some(c) if c.is_ascii_alphabetic() || c == '_' => self.lex_identifier()?,
                Some(c) => {
                    return Err(format!(
                        "Unexpected character '{}' at {}:{}",
                        c, self.line, self.column
                    ))
                }
                None => break,
            }
        }

        self.add_token(Token::Eof);
        Ok(self.tokens)
    }

    fn lex_directive(&mut self) -> Result<(), String> {
        self.advance(); // consume @

        let directive = self.lex_identifier_string();

        let token = match directive.as_str() {
            "page" => Token::PageDirective,
            "fetch" => Token::FetchDirective,
            "live" => Token::LiveDirective,
            "loading" => Token::LoadingDirective,
            "error" => Token::ErrorDirective,
            "stale" => Token::StaleDirective,
            _ => return Err(format!("Unknown directive: @{}", directive)),
        };

        self.add_token(token);
        Ok(())
    }

    fn lex_html_tag(&mut self) -> Result<(), String> {
        if self.peek() == Some('<') {
            self.advance();

            // Check for closing tag </
            if self.peek() == Some('/') {
                self.advance();
                self.add_token(Token::HtmlTagClose);
            } else {
                self.add_token(Token::HtmlTagStart);
            }
        }

        Ok(())
    }

    fn lex_expression_or_brace(&mut self) -> Result<(), String> {
        if self.peek() == Some('{') {
            self.advance();
            if self.peek() == Some('{') {
                self.advance();
                self.add_token(Token::ExpressionStart);
            } else {
                self.add_token(Token::LeftBrace);
            }
        }
        Ok(())
    }

    fn lex_close_brace(&mut self) -> Result<(), String> {
        if self.peek() == Some('}') {
            self.advance();
            if self.peek() == Some('}') {
                self.advance();
                self.add_token(Token::ExpressionEnd);
            } else {
                self.add_token(Token::RightBrace);
            }
        }
        Ok(())
    }

    fn lex_string(&mut self) -> Result<(), String> {
        self.advance(); // consume opening quote

        let mut value = String::new();
        while self.peek().is_some() && self.peek() != Some('"') {
            if self.peek() == Some('\\') {
                self.advance();
                match self.peek() {
                    Some('n') => value.push('\n'),
                    Some('t') => value.push('\t'),
                    Some('r') => value.push('\r'),
                    Some('"') => value.push('"'),
                    Some('\\') => value.push('\\'),
                    Some(c) => value.push(c),
                    None => return Err("Unterminated string".to_string()),
                }
                self.advance();
            } else {
                if let Some(c) = self.peek() {
                    value.push(c);
                }
                self.advance();
            }
        }

        if self.peek() != Some('"') {
            return Err("Unterminated string".to_string());
        }

        self.advance(); // consume closing quote
        self.add_token(Token::String(value));
        Ok(())
    }

    fn lex_forward_slash(&mut self) -> Result<(), String> {
        self.advance();
        if self.peek() == Some('>') {
            self.advance();
            self.add_token(Token::HtmlSelfClose);
        } else {
            self.add_token(Token::Forward);
        }
        Ok(())
    }

    fn lex_number(&mut self) -> Result<(), String> {
        let mut number = String::new();

        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                number.push(c);
                self.advance();
            } else {
                break;
            }
        }

        self.add_token(Token::Number(number));
        Ok(())
    }

    fn lex_identifier(&mut self) -> Result<(), String> {
        let ident = self.lex_identifier_string();

        let token = match ident.as_str() {
            "title" => Token::Title,
            "from" => Token::From,
            "every" => Token::Every,
            "timeout" => Token::Timeout,
            "retry" => Token::Retry,
            "exponential" => Token::Exponential,
            "initial" => Token::Initial,
            "server" => Token::Server,
            _ => Token::Identifier(ident),
        };

        self.add_token(token);
        Ok(())
    }

    fn lex_identifier_string(&mut self) -> String {
        let mut ident = String::new();

        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                ident.push(c);
                self.advance();
            } else {
                break;
            }
        }

        ident
    }

    fn peek(&mut self) -> Option<char> {
        self.input.peek().copied()
    }

    fn advance(&mut self) {
        if let Some(c) = self.input.next() {
            if c == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
    }

    fn add_token(&mut self, token: Token) {
        self.tokens
            .push(TokenWithLocation::new(token, self.line, self.column));
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }
}

pub fn tokenize(input: &str) -> Result<Vec<TokenWithLocation>, String> {
    Lexer::new(input).tokenize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lex_page_directive() {
        let input = "@page title=\"Dashboard\"";
        let tokens = tokenize(input).unwrap();

        assert!(matches!(tokens[0].token, Token::PageDirective));
        assert!(matches!(tokens[1].token, Token::Title));
        assert!(matches!(tokens[2].token, Token::Assign));
        assert!(matches!(tokens[3].token, Token::String(ref s) if s == "Dashboard"));
    }

    #[test]
    fn test_lex_live_directive() {
        let input = "@live chain from \"/api/status\" every 1000";
        let tokens = tokenize(input).unwrap();

        assert!(matches!(tokens[0].token, Token::LiveDirective));
        assert!(matches!(tokens[1].token, Token::Identifier(ref s) if s == "chain"));
        assert!(matches!(tokens[2].token, Token::From));
        assert!(matches!(tokens[3].token, Token::String(ref s) if s == "/api/status"));
        assert!(matches!(tokens[4].token, Token::Every));
        assert!(matches!(tokens[5].token, Token::Number(ref n) if n == "1000"));
    }

    #[test]
    fn test_lex_fetch_directive() {
        let tokens = tokenize("@fetch runtime from \"/api/runtime/status\"\n<main></main>")
            .expect("@fetch should tokenize");
        assert!(tokens
            .iter()
            .any(|token| token.token == Token::FetchDirective));
    }

    #[test]
    fn test_lex_template_expression() {
        let input = "{{ chain.height }}";
        let tokens = tokenize(input).unwrap();

        assert!(matches!(tokens[0].token, Token::ExpressionStart));
        assert!(matches!(tokens[1].token, Token::Identifier(ref s) if s == "chain"));
        assert!(matches!(tokens[2].token, Token::Dot));
        assert!(matches!(tokens[3].token, Token::Identifier(ref s) if s == "height"));
        assert!(matches!(tokens[4].token, Token::ExpressionEnd));
    }

    #[test]
    fn test_lex_html_tag() {
        let input = "<main><h1>{{ value }}</h1></main>";
        let tokens = tokenize(input).unwrap();

        assert!(matches!(tokens[0].token, Token::HtmlTagStart));
        assert!(matches!(tokens[1].token, Token::Identifier(ref s) if s == "main"));
        assert!(matches!(tokens[2].token, Token::HtmlTagEnd));
        assert!(matches!(tokens[3].token, Token::HtmlTagStart));
        assert!(matches!(tokens[4].token, Token::Identifier(ref s) if s == "h1"));
        assert!(matches!(tokens[5].token, Token::HtmlTagEnd));
    }

    #[test]
    fn test_tokenize_basic_fixture() {
        let input = r#"@page title="Smart Chain Dashboard"
@live chain from "/api/status" every 1000

<main>
    <h1>Chain Height: {{ chain.height }}</h1>
</main>"#;

        let tokens = tokenize(input).unwrap();

        // Should have at least: @page, title, =, string, @live, identifier, from, string, every, number, etc.
        assert!(tokens.len() > 10);

        // Verify key tokens are present
        let has_page = tokens
            .iter()
            .any(|t| matches!(t.token, Token::PageDirective));
        let has_live = tokens
            .iter()
            .any(|t| matches!(t.token, Token::LiveDirective));
        let has_expr_start = tokens
            .iter()
            .any(|t| matches!(t.token, Token::ExpressionStart));

        assert!(has_page);
        assert!(has_live);
        assert!(has_expr_start);
    }
}
