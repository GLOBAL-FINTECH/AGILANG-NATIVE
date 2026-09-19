use agilang_compiler::check;
use agilang_source::SourceFile;
use anyhow::Result;
use std::io::{self, Write};

fn main() -> Result<()> {
    println!("AGILANG Native REPL 0.8.0");
    println!("Enter a function or expression. :help, :check, :quit");
    let mut buffer = String::new();
    loop {
        print!("agi> ");
        io::stdout().flush()?;
        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 { break; }
        let command = line.trim();
        match command {
            ":quit" | ":q" => break,
            ":help" => println!(":check validates the current buffer; :clear resets it; :quit exits."),
            ":clear" => { buffer.clear(); println!("buffer cleared"); }
            ":check" => {
                let source = SourceFile::new("<repl>", &buffer);
                match check(&source) {
                    Ok(program) => println!("ok: {} function(s)", program.functions.len()),
                    Err(errors) => for error in errors { println!("{}", error.render(&source)); },
                }
            }
            _ if command.is_empty() => {}
            _ => {
                buffer.push_str(&line);
                let source = SourceFile::new("<repl>", &buffer);
                match check(&source) {
                    Ok(_) => println!("ok"),
                    Err(_) => println!("buffered; use :check for diagnostics"),
                }
            }
        }
    }
    Ok(())
}
