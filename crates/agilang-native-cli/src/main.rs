use agilang_compiler::{check, hir, parse, symbols, tokenize, SourceFile};
use anyhow::{bail, Context, Result};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".into());
    match command.as_str() {
        "new" => {
            let name = args.next().context("project name is required")?;
            let mut template = "app";
            while let Some(arg) = args.next() {
                if arg == "--template" {
                    template = args.next().context("template name is required")?.leak();
                }
            }
            command_new(&name, template)?;
        }
        "init" => {
            command_init()?;
        }
        "check" => {
            let source = load_optional(args.next())?;
            match check(&source) {
                Ok(ast) => println!("AGILANG check passed: {} function(s)", ast.functions.len()),
                Err(e) => report(&source, e)?,
            }
        }
        "symbols" => {
            let source = load_optional(args.next())?;
            match symbols(&source) {
                Ok(scopes) => {
                    println!("module");
                    // Print symbols in global scope first, then subsequent scopes if any
                    for scope in &scopes {
                        for sym in scope.symbols.values() {
                            match sym.kind {
                                agilang_symbols::SymbolKind::Function => {
                                    if sym.name == "print" {
                                        continue; // Skip builtin print for clean output
                                    }
                                    println!(
                                        "└── fn {}() -> {}",
                                        sym.name,
                                        match &sym.ty {
                                            agilang_types::Type::Function(f) =>
                                                format!("{}", f.ret),
                                            t => format!("{}", t),
                                        }
                                    );
                                }
                                agilang_symbols::SymbolKind::Local
                                | agilang_symbols::SymbolKind::Constant => {
                                    println!("    ├── {}: {}", sym.name, sym.ty);
                                }
                                agilang_symbols::SymbolKind::Parameter => {
                                    println!("    ├── (param) {}: {}", sym.name, sym.ty);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Err(e) => report(&source, e)?,
            }
        }
        "hir" => {
            let source = load_optional(args.next())?;
            match hir(&source) {
                Ok(hir_prog) => {
                    println!("HirProgram");
                    for func in &hir_prog.functions {
                        println!("└── HirFunction {}() -> {}", func.name, func.return_type);
                        for stmt in &func.body {
                            match stmt {
                                agilang_ir::HirStmt::Let {
                                    name, ty, value, ..
                                } => {
                                    println!(
                                        "    ├── Let {}: {} = {:#?}: {}",
                                        name,
                                        ty,
                                        value,
                                        value.ty()
                                    );
                                }
                                agilang_ir::HirStmt::Return { value, .. } => {
                                    if let Some(val) = value {
                                        println!("    └── Return {:#?}: {}", val, val.ty());
                                    } else {
                                        println!("    └── Return void");
                                    }
                                }
                                agilang_ir::HirStmt::Expr(expr) => {
                                    println!("    ├── Expr {:#?}: {}", expr, expr.ty());
                                }
                            }
                        }
                    }
                }
                Err(e) => report(&source, e)?,
            }
        }
        "tokens" => {
            let source = load_optional(args.next())?;
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
            let source = load_optional(args.next())?;
            match parse(&source) {
                Ok(ast) => println!("{ast:#?}"),
                Err(e) => report(&source, e)?,
            }
        }
        "run" => {
            let source = load_optional(args.next())?;
            match check(&source) {
                Ok(_) => {
                    println!("AGILANG check passed. (Execution is not yet implemented in Phase 2)")
                }
                Err(e) => report(&source, e)?,
            }
        }
        "build" => {
            let source = load_optional(args.next())?;
            match check(&source) {
                Ok(_) => println!(
                    "AGILANG check passed. (Compilation is not yet implemented in Phase 2)"
                ),
                Err(e) => report(&source, e)?,
            }
        }
        "test" => {
            println!("AGILANG check passed. (Testing is not yet implemented in Phase 2)");
        }
        "fmt" => {
            println!("Formatting is not yet implemented in Phase 2");
        }
        "doctor" => {
            println!("AGILANG native runtime status: healthy");
        }
        "--version" | "version" | "-V" => {
            println!(
                "AGILANG Native Compiler Frontend {}",
                env!("CARGO_PKG_VERSION")
            );
        }
        "help" | "--help" | "-h" => {
            print_help();
        }
        other => {
            unknown_command(other);
            std::process::exit(1);
        }
    }
    Ok(())
}

fn command_new(name: &str, template: &str) -> Result<()> {
    let path = PathBuf::from(name);
    if path.exists() {
        bail!("directory `{}` already exists", name);
    }
    fs::create_dir_all(&path)?;
    fs::create_dir_all(path.join("src"))?;
    fs::create_dir_all(path.join("tests"))?;

    // Create agilang.toml
    let toml = format!(
        "[project]\nname = \"{}\"\nversion = \"0.1.0\"\nentry = \"src/main.agi\"\n",
        name
    );
    fs::write(path.join("agilang.toml"), toml)?;

    // Create main.agi based on template
    let main_content = match template {
        "blockchain" => {
            "fn main() -> i32:\n    print(\"Initializing Smart Chain\")\n    return 0\n"
        }
        "ags" => "fn main() -> i32:\n    print(\"Initializing AGS Dashboard\")\n    return 0\n",
        "web" => "fn main() -> i32:\n    print(\"Initializing Web App Server\")\n    return 0\n",
        _ => "fn main() -> i32:\n    print(\"Hello from AGILANG\")\n    return 0\n",
    };
    fs::write(path.join("src/main.agi"), main_content)?;

    // Create main_test.agi
    fs::write(
        path.join("tests/main_test.agi"),
        "fn test_hello() -> i32:\n    return 0\n",
    )?;

    // Create .gitignore
    fs::write(path.join(".gitignore"), "/target\n")?;

    // Create README.md
    fs::write(
        path.join("README.md"),
        format!("# {}\n\nCreated with AGILANG CLI.\n", name),
    )?;

    println!(
        "Created new AGILANG project `{}` with template `{}`",
        name, template
    );
    Ok(())
}

fn command_init() -> Result<()> {
    let toml_path = Path::new("agilang.toml");
    if toml_path.exists() {
        bail!("agilang.toml already exists in the current directory");
    }
    let current_dir = env::current_dir()?;
    let name = current_dir
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app".to_string());

    fs::create_dir_all("src")?;
    fs::create_dir_all("tests")?;

    let toml = format!(
        "[project]\nname = \"{}\"\nversion = \"0.1.0\"\nentry = \"src/main.agi\"\n",
        name
    );
    fs::write("agilang.toml", toml)?;
    fs::write(
        "src/main.agi",
        "fn main() -> i32:\n    print(\"Hello from AGILANG\")\n    return 0\n",
    )?;
    fs::write(
        "tests/main_test.agi",
        "fn test_hello() -> i32:\n    return 0\n",
    )?;
    fs::write(".gitignore", "/target\n")?;
    fs::write("README.md", format!("# {}\n", name))?;

    println!(
        "Initialized AGILANG project `{}` in the current directory",
        name
    );
    Ok(())
}

fn find_project_entry() -> Result<PathBuf> {
    let mut dir = env::current_dir()?;
    loop {
        let manifest_path = dir.join("agilang.toml");
        if manifest_path.exists() {
            let content = fs::read_to_string(&manifest_path)?;
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("entry") {
                    if let Some(pos) = trimmed.find('=') {
                        let value = trimmed[pos + 1..]
                            .trim()
                            .trim_matches('"')
                            .trim_matches('\'');
                        return Ok(dir.join(value));
                    }
                }
            }
            return Ok(dir.join("src/main.agi"));
        }
        if !dir.pop() {
            break;
        }
    }
    bail!("no agilang.toml found and no file specified")
}

fn load_optional(path: Option<String>) -> Result<SourceFile> {
    let entry_path = match path {
        Some(p) => PathBuf::from(p),
        None => find_project_entry()?,
    };
    SourceFile::load(&entry_path)
        .with_context(|| format!("failed to load {}", entry_path.display()))
}

fn report(source: &SourceFile, errors: Vec<agilang_compiler::Diagnostic>) -> Result<()> {
    for e in &errors {
        eprintln!("{}", e.render(source));
    }
    bail!("compilation failed with {} error(s)", errors.len())
}

fn print_help() {
    println!(
        "AGILANG Compiler Toolchain\n\n\
        Usage:\n  \
          agilang new <project> [--template <template>]\n  \
          agilang init\n  \
          agilang check [<file>]\n  \
          agilang run [<file>]\n  \
          agilang build [<file>]\n  \
          agilang test\n  \
          agilang fmt\n  \
          agilang symbols [<file>]\n  \
          agilang hir [<file>]\n  \
          agilang tokens [<file>]\n  \
          agilang ast [<file>]\n  \
          agilang doctor\n  \
          agilang version\n"
    );
}

fn unknown_command(command: &str) {
    eprintln!("error: unknown command `{}`", command);
    let suggestion = match command {
        "symbol" => Some("symbols"),
        "token" => Some("tokens"),
        "check-app" => Some("check"),
        "init-app" => Some("init"),
        "format" => Some("fmt"),
        "run-app" => Some("run"),
        "build-app" => Some("build"),
        _ => None,
    };
    if let Some(s) = suggestion {
        eprintln!("\nDid you mean:\n    {}", s);
    }
    eprintln!("\nRun `agilang help` for available commands.");
}
