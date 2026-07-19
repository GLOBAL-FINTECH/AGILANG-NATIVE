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
    let ast = parse(source)?;
    let analyser = agilang_semantic::Analyser::new();
    let _ = analyser.analyse(&ast)?;
    Ok(ast)
}

pub fn symbols(source: &SourceFile) -> Result<Vec<agilang_symbols::SymbolTable>, Vec<Diagnostic>> {
    let ast = parse(source)?;
    let analyser = agilang_semantic::Analyser::new();
    let (_, scopes) = analyser.analyse(&ast)?;
    Ok(scopes)
}

pub fn hir(source: &SourceFile) -> Result<agilang_ir::HirProgram, Vec<Diagnostic>> {
    let ast = parse(source)?;
    let analyser = agilang_semantic::Analyser::new();
    let (hir_prog, _) = analyser.analyse(&ast)?;
    Ok(hir_prog)
}
