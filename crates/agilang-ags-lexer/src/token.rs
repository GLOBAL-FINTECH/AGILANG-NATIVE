//! Token definitions for AGS (Agilang View Language)

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Directives
    PageDirective,
    FetchDirective,
    LiveDirective,
    LoadingDirective,
    ErrorDirective,
    StaleDirective,

    // Keywords
    Title,
    From,
    Every,
    Timeout,
    Retry,
    Exponential,
    Initial,
    Server,

    // Literals
    Identifier(String),
    String(String),
    Number(String),

    // Operators & Punctuation
    Assign,       // =
    Dot,          // .
    Colon,        // :
    Semicolon,    // ;
    Comma,        // ,
    LeftBrace,    // {
    RightBrace,   // }
    LeftBracket,  // [
    RightBracket, // ]
    LeftParen,    // (
    RightParen,   // )
    Forward,      // /
    At,           // @

    // Template expressions
    ExpressionStart, // {{
    ExpressionEnd,   // }}

    // HTML tags
    HtmlTagStart,  // <
    HtmlTagEnd,    // >
    HtmlTagClose,  // </
    HtmlSelfClose, // />

    // HTML content
    HtmlText(String),
    HtmlAttribute(String),

    // Comments
    Comment(String),

    // Special
    Eof,
}

#[derive(Debug, Clone)]
pub struct TokenWithLocation {
    pub token: Token,
    pub line: usize,
    pub column: usize,
}

impl TokenWithLocation {
    pub fn new(token: Token, line: usize, column: usize) -> Self {
        TokenWithLocation {
            token,
            line,
            column,
        }
    }
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::PageDirective => write!(f, "@page"),
            Token::FetchDirective => write!(f, "@fetch"),
            Token::LiveDirective => write!(f, "@live"),
            Token::Identifier(s) => write!(f, "{}", s),
            Token::String(s) => write!(f, "\"{}\"", s),
            Token::Number(n) => write!(f, "{}", n),
            Token::HtmlText(t) => write!(f, "text({})", t),
            Token::ExpressionStart => write!(f, "{{{{"),
            Token::ExpressionEnd => write!(f, "}}}}"),
            _ => write!(f, "{:?}", self),
        }
    }
}
