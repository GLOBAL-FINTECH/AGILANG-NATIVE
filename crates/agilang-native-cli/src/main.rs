use agilang_compiler::{check, parse, tokenize, SourceFile};
use anyhow::{bail, Context, Result};
use std::{env, path::PathBuf};

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".into());
    match command.as_str() {
        "--version" | "version" => println!(
            "AGILANG Native Compiler Frontend {}",
            env!("CARGO_PKG_VERSION")
        ),
        "tokens" => {
            let source = load(args.next())?;
            match tokenize(&source) {
                Ok(tokens) => {
                    for t in tokens {
                        println!("{:?} {}..{}", t.kind, t.span.start, t.span.end)
                    }
                }
                Err(e) => report(&source, e)?,
            }
        }
        "ast" => {
            let source = load(args.next())?;
            match parse(&source) {
                Ok(ast) => println!("{ast:#?}"),
                Err(e) => report(&source, e)?,
            }
        }
        "check" => {
            let source = load(args.next())?;
            match check(&source) {
                Ok(ast) => println!("AGILANG check passed: {} function(s)", ast.functions.len()),
                Err(e) => report(&source, e)?,
            }
        }
        "help" | "--help" | "-h" => print_help(),
        other => bail!("unknown command `{other}`; expected tokens, ast, check, or version"),
    };
    Ok(())
}
fn load(path: Option<String>) -> Result<SourceFile> {
    let path = PathBuf::from(path.context("source file path is required")?);
    SourceFile::load(&path).with_context(|| format!("failed to load {}", path.display()))
}
fn report(source: &SourceFile, errors: Vec<agilang_compiler::Diagnostic>) -> Result<()> {
    for e in &errors {
        eprintln!("{}", e.render(source));
    }
    bail!("compilation failed with {} error(s)", errors.len())
}
fn print_help() {
    println!("AGILANG Native Compiler Frontend\n\nUsage:\n  agilang-native tokens <file.agi>\n  agilang-native ast <file.agi>\n  agilang-native check <file.agi>\n  agilang-native version");
}
