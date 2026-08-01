//! AGS Parser - Converts token stream to AST

use agilang_ags_ast::*;
use agilang_ags_lexer::{Token, TokenWithLocation};

pub struct Parser {
    tokens: Vec<TokenWithLocation>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<TokenWithLocation>) -> Self {
        Parser { tokens, pos: 0 }
    }

    pub fn parse(mut self) -> Result<Document, String> {
        let mut directives = Vec::new();

        // Parse all directives first
        while self.is_directive() {
            directives.push(self.parse_directive()?);
        }

        // Parse the root HTML element
        let root = self.parse_element()?;

        Ok(Document::new(directives, root))
    }

    fn parse_directive(&mut self) -> Result<Directive, String> {
        match &self.peek().token {
            Token::PageDirective => self.parse_page_directive(),
            Token::FetchDirective => self.parse_fetch_directive(),
            Token::LiveDirective => self.parse_live_directive(),
            Token::LoadingDirective => self.parse_loading_directive(),
            Token::ErrorDirective => self.parse_error_directive(),
            Token::StaleDirective => self.parse_stale_directive(),
            _ => Err(format!("Unexpected directive: {:?}", self.peek())),
        }
    }

    fn parse_page_directive(&mut self) -> Result<Directive, String> {
        self.expect_token(Token::PageDirective)?;

        let mut title = String::new();
        let mut seo_description = None;
        let mut robots = None;

        // Parse attributes: title="..." seo_description="..."
        while !self.is_directive() && !self.is_element_start() && self.peek().token != Token::Eof {
            match &self.peek().token {
                Token::Title => {
                    self.advance();
                    self.expect_token(Token::Assign)?;
                    title = self.parse_string_value()?;
                }
                Token::Identifier(name) if name == "seo_description" => {
                    self.advance();
                    self.expect_token(Token::Assign)?;
                    seo_description = Some(self.parse_string_value()?);
                }
                Token::Identifier(name) if name == "robots" => {
                    self.advance();
                    self.expect_token(Token::Assign)?;
                    robots = Some(self.parse_string_value()?);
                }
                _ => {
                    self.advance();
                }
            }
        }

        Ok(Directive::Page {
            title,
            seo_description,
            robots,
        })
    }

    fn parse_fetch_directive(&mut self) -> Result<Directive, String> {
        self.expect_token(Token::FetchDirective)?;
        let name = self.parse_identifier()?;
        let response_type = if self.peek().token == Token::Colon {
            self.advance();
            Some(self.parse_identifier()?)
        } else {
            None
        };
        self.expect_token(Token::From)?;
        let endpoint = self.parse_string_value()?;
        Ok(Directive::Fetch {
            name,
            response_type,
            endpoint,
        })
    }

    fn parse_live_directive(&mut self) -> Result<Directive, String> {
        self.expect_token(Token::LiveDirective)?;

        // @live name from "endpoint" every 1000 [timeout 5000] [retry exponential] [initial server]
        let name = match &self.peek().token {
            Token::Identifier(n) => {
                let n = n.clone();
                self.advance();
                n
            }
            _ => return Err("Expected identifier after @live".to_string()),
        };

        let response_type = if self.peek().token == Token::Colon {
            self.advance();
            Some(self.parse_identifier()?)
        } else {
            None
        };

        self.expect_token(Token::From)?;
        let endpoint = self.parse_string_value()?;

        self.expect_token(Token::Every)?;
        let interval_ms = self.parse_number_value()?;

        let mut timeout_ms = None;
        let mut retry_strategy = None;
        let mut initial_server = false;

        // Parse optional clauses
        while !self.is_directive() && !self.is_element_start() && self.peek().token != Token::Eof {
            match &self.peek().token {
                Token::Timeout => {
                    self.advance();
                    timeout_ms = Some(self.parse_number_value()?);
                }
                Token::Retry => {
                    self.advance();
                    retry_strategy = Some(match &self.peek().token {
                        Token::Exponential => {
                            self.advance();
                            "exponential".to_string()
                        }
                        _ => return Err("Expected retry strategy".to_string()),
                    });
                }
                Token::Initial => {
                    self.advance();
                    self.expect_token(Token::Server)?;
                    initial_server = true;
                }
                _ => break,
            }
        }

        Ok(Directive::Live {
            name,
            response_type,
            endpoint,
            interval_ms,
            timeout_ms,
            retry_strategy,
            initial_server,
        })
    }

    #[allow(dead_code)]
    fn parse_loading_directive(&mut self) -> Result<Directive, String> {
        self.expect_token(Token::LoadingDirective)?;
        let binding_name = self.parse_identifier()?;
        Ok(Directive::Loading { binding_name })
    }

    #[allow(dead_code)]
    fn parse_error_directive(&mut self) -> Result<Directive, String> {
        self.expect_token(Token::ErrorDirective)?;
        let binding_name = self.parse_identifier()?;

        let error_var = if self.match_keyword("as") {
            Some(self.parse_identifier()?)
        } else {
            None
        };

        Ok(Directive::Error {
            binding_name,
            error_var,
        })
    }

    #[allow(dead_code)]
    fn parse_stale_directive(&mut self) -> Result<Directive, String> {
        self.expect_token(Token::StaleDirective)?;
        let binding_name = self.parse_identifier()?;
        Ok(Directive::Stale { binding_name })
    }

    fn parse_element(&mut self) -> Result<ViewNode, String> {
        self.expect_token(Token::HtmlTagStart)?;

        let tag = self.parse_identifier()?;
        let line = self.peek().line;
        let column = self.peek().column;

        let mut node = ViewNode::new(&tag, line, column);

        // Parse attributes
        while self.peek().token != Token::HtmlTagEnd {
            if let Token::Identifier(attr_name) = &self.peek().token {
                let attr_name = attr_name.clone();
                self.advance();

                let attr_value = if self.peek().token == Token::Assign {
                    self.advance();
                    self.parse_string_value()?
                } else {
                    String::new()
                };

                node = node.with_attribute(attr_name, attr_value);
            } else {
                self.advance();
            }
        }

        self.expect_token(Token::HtmlTagEnd)?;

        // Parse content and children
        let mut text_content = Vec::new();
        loop {
            match &self.peek().token {
                Token::HtmlTagStart => {
                    // Check if it's a closing tag
                    let next_pos = self.pos + 1;
                    if next_pos < self.tokens.len() {
                        if matches!(self.tokens[next_pos].token, Token::Identifier(_)) {
                            // It's an opening tag or closing tag, we'll check
                            let saved_pos = self.pos;
                            self.advance(); // consume HtmlTagStart
                            if matches!(&self.peek().token, Token::Identifier(_)) {
                                // It might be a child element or closing tag
                                // Look ahead to see if there's another tag
                                let tag_name = if let Token::Identifier(n) = &self.peek().token {
                                    n.clone()
                                } else {
                                    String::new()
                                };

                                if tag_name == tag {
                                    // This is closing tag
                                    self.pos = saved_pos;
                                    break;
                                } else {
                                    // This is a child element
                                    self.pos = saved_pos;
                                    let child = self.parse_element()?;
                                    node.children.push(child);
                                }
                            } else {
                                self.pos = saved_pos;
                                break;
                            }
                        }
                    } else {
                        break;
                    }
                }
                Token::ExpressionStart => {
                    let binding = self.parse_binding()?;
                    text_content.push(TextOrBinding::Binding(binding));
                }
                Token::HtmlText(text) => {
                    if !text.trim().is_empty() {
                        text_content.push(TextOrBinding::Text(text.clone()));
                    }
                    self.advance();
                }
                Token::Eof => break,
                _ => {
                    self.advance();
                }
            }
        }

        if !text_content.is_empty() {
            node = node.set_text_content(text_content);
        }

        // Parse closing tag
        if self.peek().token == Token::HtmlTagClose {
            self.advance();
            let closing_tag = self.parse_identifier()?;
            if closing_tag != tag {
                return Err(format!(
                    "Mismatched closing tag: expected </{}>, got </{}>",
                    tag, closing_tag
                ));
            }
            self.expect_token(Token::HtmlTagEnd)?;
        }

        Ok(node)
    }

    fn parse_binding(&mut self) -> Result<Binding, String> {
        let line = self.peek().line;
        let column = self.peek().column;

        self.expect_token(Token::ExpressionStart)?;

        let mut expression = String::new();

        // Parse the expression: source.field.path
        while self.peek().token != Token::ExpressionEnd {
            match &self.peek().token {
                Token::Identifier(name) => {
                    expression.push_str(name);
                    self.advance();
                }
                Token::Dot => {
                    expression.push('.');
                    self.advance();
                }
                Token::Eof => return Err("Unterminated binding expression".to_string()),
                _ => {
                    return Err(format!("Unexpected token in binding: {:?}", self.peek()));
                }
            }
        }

        self.expect_token(Token::ExpressionEnd)?;

        Ok(Binding {
            expression,
            line,
            column,
        })
    }

    fn parse_string_value(&mut self) -> Result<String, String> {
        match &self.peek().token {
            Token::String(s) => {
                let s = s.clone();
                self.advance();
                Ok(s)
            }
            _ => Err("Expected string value".to_string()),
        }
    }

    fn parse_number_value(&mut self) -> Result<u64, String> {
        match &self.peek().token {
            Token::Number(n) => {
                let n = n.parse::<u64>().map_err(|_| "Invalid number".to_string())?;
                self.advance();
                Ok(n)
            }
            _ => Err("Expected number value".to_string()),
        }
    }

    fn parse_identifier(&mut self) -> Result<String, String> {
        match &self.peek().token {
            Token::Identifier(n) => {
                let n = n.clone();
                self.advance();
                Ok(n)
            }
            _ => Err("Expected identifier".to_string()),
        }
    }

    #[allow(dead_code)]
    fn expect_keyword(&mut self, keyword: &str) -> Result<(), String> {
        match &self.peek().token {
            Token::Identifier(n) if n == keyword => {
                self.advance();
                Ok(())
            }
            _ => Err(format!("Expected keyword '{}'", keyword)),
        }
    }

    #[allow(dead_code)]
    fn match_keyword(&mut self, keyword: &str) -> bool {
        if let Token::Identifier(n) = &self.peek().token {
            if n == keyword {
                self.advance();
                return true;
            }
        }
        false
    }

    fn expect_token(&mut self, expected: Token) -> Result<(), String> {
        let current = self.peek().token.clone();
        if std::mem::discriminant(&current) == std::mem::discriminant(&expected) {
            self.advance();
            Ok(())
        } else {
            Err(format!("Expected {:?}, got {:?}", expected, current))
        }
    }

    fn peek(&self) -> &TokenWithLocation {
        self.tokens.get(self.pos).unwrap_or(&TokenWithLocation {
            token: Token::Eof,
            line: 0,
            column: 0,
        })
    }

    fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn is_directive(&self) -> bool {
        matches!(
            self.peek().token,
            Token::PageDirective
                | Token::FetchDirective
                | Token::LiveDirective
                | Token::LoadingDirective
                | Token::ErrorDirective
                | Token::StaleDirective
        )
    }

    fn is_element_start(&self) -> bool {
        matches!(self.peek().token, Token::HtmlTagStart)
    }
}

pub fn parse(tokens: Vec<TokenWithLocation>) -> Result<Document, String> {
    Parser::new(tokens).parse()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agilang_ags_lexer::tokenize;

    #[test]
    fn test_parse_page_directive() {
        let input = r#"@page title="Test Dashboard" seo_description="Test page"
<main></main>"#;
        let tokens = tokenize(input).unwrap();
        let doc = parse(tokens).unwrap();

        assert!(matches!(
            doc.get_page_directive(),
            Some(Directive::Page { .. })
        ));
    }

    #[test]
    fn test_parse_live_directive() {
        let input = r#"@live chain from "/api/status" every 1000 timeout 5000 retry exponential initial server
<main></main>"#;
        let tokens = tokenize(input).unwrap();
        let doc = parse(tokens).unwrap();

        let live_dirs = doc.get_live_directives();
        assert_eq!(live_dirs.len(), 1);

        if let Directive::Live {
            name,
            endpoint,
            interval_ms,
            ..
        } = live_dirs[0]
        {
            assert_eq!(name, "chain");
            assert_eq!(endpoint, "/api/status");
            assert_eq!(*interval_ms, 1000);
        }
    }

    #[test]
    fn test_parse_fetch_and_page_robots() {
        let input = r#"@page title="Native" robots="index,follow"
@fetch runtime from "/api/runtime/status"
<main></main>"#;
        let document = parse(tokenize(input).unwrap()).unwrap();
        assert!(matches!(
            &document.directives[0],
            Directive::Page {
                robots: Some(value),
                ..
            } if value == "index,follow"
        ));
        assert!(matches!(
            &document.directives[1],
            Directive::Fetch { name, endpoint, .. }
                if name == "runtime" && endpoint == "/api/runtime/status"
        ));
    }

    #[test]
    fn test_parse_simple_element() {
        let input = r#"@page title="Test"
<main></main>"#;
        let tokens = tokenize(input).unwrap();
        let doc = parse(tokens).unwrap();

        assert_eq!(doc.root.tag, "main");
    }

    #[test]
    fn test_parse_element_with_attributes() {
        let input = r#"@page title="Test"
<div class="container" id="main"></div>"#;
        let tokens = tokenize(input).unwrap();
        let doc = parse(tokens).unwrap();

        assert_eq!(doc.root.tag, "div");
        assert_eq!(
            doc.root.attributes.get("class"),
            Some(&"container".to_string())
        );
        assert_eq!(doc.root.attributes.get("id"), Some(&"main".to_string()));
    }
}
