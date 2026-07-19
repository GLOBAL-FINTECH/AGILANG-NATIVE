//! Orchestrates the first native AGILANG frontend stages.
pub use agilang_ast::Program;
pub use agilang_diagnostics::Diagnostic;
pub use agilang_lexer::{Token, TokenKind};
pub use agilang_source::SourceFile;

pub fn tokenize(source: &SourceFile) -> Result<Vec<Token>, Vec<Diagnostic>> {
    agilang_lexer::lex(source)
}
pub fn parse(source: &SourceFile) -> Result<Program, Vec<Diagnostic>> {
    let tokens = tokenize(source)?;
    agilang_parser::parse(&tokens)
}
pub fn check(source: &SourceFile) -> Result<Program, Vec<Diagnostic>> {
    parse(source)
}
