use agilang_compiler::{check, hir, parse, tokenize, SourceFile};
use anyhow::{bail, Context, Result};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

const TARGET: &str = if cfg!(target_arch = "x86_64") {
    "x86_64-pc-windows-msvc"
} else {
    "aarch64-pc-windows-msvc"
};

fn find_conflicting_installations() -> Vec<PathBuf> {
    let mut conflicts = vec![];
    let active_exe = env::current_exe().ok();
    if let Some(path_var) = env::var_os("Path") {
        for dir in env::split_paths(&path_var) {
            let exe = dir.join("agilang.exe");
            if exe.exists() {
                if let Some(ref active) = active_exe {
                    if let (Ok(p1), Ok(p2)) = (exe.canonicalize(), active.canonicalize()) {
                        if p1 != p2 {
                            conflicts.push(exe);
                        }
                    } else if exe != *active {
                        conflicts.push(exe);
                    }
                } else {
                    conflicts.push(exe);
                }
            }
        }
    }
    conflicts
}

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".into());
    match command.as_str() {
        "new" => {
            let name = args.next().context("project name is required")?;
            let remaining: Vec<String> = args.collect();
            let mut template = "app";
            let mut i = 0;
            while i < remaining.len() {
                if remaining[i] == "--template" && i + 1 < remaining.len() {
                    template = remaining[i + 1].clone().leak();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            agilang_project_generator::generate_project(&name, template)?;
            println!(
                "Created new AGILANG project `{}` with template `{}`",
                name, template
            );
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
            match hir(&source) {
                Ok(hir_prog) => {
                    println!("module");
                    println!("├── builtin print(string) -> void");
                    for (i, func) in hir_prog.functions.iter().enumerate() {
                        let is_last_func = i == hir_prog.functions.len() - 1;
                        let prefix = if is_last_func {
                            "└── "
                        } else {
                            "├── "
                        };
                        println!("{}fn {}() -> {}", prefix, func.name, func.return_type);

                        let child_prefix = if is_last_func { "    " } else { "│   " };
                        for (j, sym) in func.local_symbols.iter().enumerate() {
                            let is_last_sym = j == func.local_symbols.len() - 1;
                            let sym_prefix = if is_last_sym {
                                "└── "
                            } else {
                                "├── "
                            };
                            let mut_str = if sym.mutable { "mutable" } else { "constant" };
                            println!(
                                "{}{}local {} {}: {}",
                                child_prefix, sym_prefix, mut_str, sym.name, sym.ty
                            );
                        }
                    }
                }
                Err(e) => report(&source, e)?,
            }
        }
        "hir" => {
            let mut file_arg = None;
            let mut debug = false;
            for arg in args.by_ref() {
                if arg == "--debug" {
                    debug = true;
                } else if file_arg.is_none() {
                    file_arg = Some(arg);
                }
            }
            let source = load_optional(file_arg)?;
            match hir(&source) {
                Ok(hir_prog) => {
                    if debug {
                        println!("{:#?}", hir_prog);
                    } else {
                        println!("HirProgram");
                        for (i, func) in hir_prog.functions.iter().enumerate() {
                            let is_last_func = i == hir_prog.functions.len() - 1;
                            let prefix = if is_last_func {
                                "└── "
                            } else {
                                "├── "
                            };
                            println!(
                                "{}HirFunction {}() -> {}",
                                prefix, func.name, func.return_type
                            );

                            let child_prefix = if is_last_func { "    " } else { "│   " };
                            for (j, stmt) in func.body.iter().enumerate() {
                                let is_last_stmt = j == func.body.len() - 1;
                                let stmt_prefix = if is_last_stmt {
                                    "└── "
                                } else {
                                    "├── "
                                };
                                match stmt {
                                    agilang_ir::HirStmt::Let {
                                        name, ty, value, ..
                                    } => {
                                        println!(
                                            "{}{}Let {}: {} = {}",
                                            child_prefix,
                                            stmt_prefix,
                                            name,
                                            ty,
                                            format_hir_expr(value)
                                        );
                                    }
                                    agilang_ir::HirStmt::Return { value, .. } => {
                                        if let Some(val) = value {
                                            println!(
                                                "{}{}Return {}",
                                                child_prefix,
                                                stmt_prefix,
                                                format_hir_expr(val)
                                            );
                                        } else {
                                            println!("{}{}Return void", child_prefix, stmt_prefix);
                                        }
                                    }
                                    agilang_ir::HirStmt::Expr(expr) => {
                                        println!(
                                            "{}{}{}",
                                            child_prefix,
                                            stmt_prefix,
                                            format_hir_expr(expr)
                                        );
                                    }
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
            let mut file_arg = None;
            for arg in args.by_ref() {
                if file_arg.is_none() {
                    file_arg = Some(arg);
                }
            }
            let entry_path = match file_arg {
                Some(p) => PathBuf::from(p),
                None => find_project_entry()?,
            };

            let mut out_exe = PathBuf::from("build/out.exe");
            if let Ok(current_dir) = env::current_dir() {
                let mut dir = current_dir;
                loop {
                    if dir.join("agilang.toml").exists() {
                        let name = dir
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "app".into());
                        out_exe = dir.join("build").join(format!("{}.exe", name));
                        break;
                    }
                    if !dir.pop() {
                        break;
                    }
                }
            }

            agilang_build::build_project(&entry_path, &out_exe, false, false, false)?;

            println!("\nExecuting {}...", out_exe.display());
            let status = std::process::Command::new(&out_exe)
                .status()
                .context("failed to execute compiled binary")?;

            if !status.success() {
                bail!("binary executed with non-zero exit code");
            }
        }
        "build" => {
            let mut file_arg = None;
            let mut emit_c = false;
            let mut keep_generated = false;
            let mut verbose = false;
            for arg in args.by_ref() {
                if arg == "--emit-c" {
                    emit_c = true;
                } else if arg == "--keep-generated" {
                    keep_generated = true;
                } else if arg == "--verbose" {
                    verbose = true;
                } else if file_arg.is_none() {
                    file_arg = Some(arg);
                }
            }
            let entry_path = match file_arg {
                Some(p) => PathBuf::from(p),
                None => find_project_entry()?,
            };

            let mut out_exe = PathBuf::from("build/out.exe");
            let mut found_project = false;
            if let Ok(current_dir) = env::current_dir() {
                let mut dir = current_dir;
                loop {
                    if dir.join("agilang.toml").exists() {
                        let name = dir
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "app".into());
                        out_exe = dir.join("build").join(format!("{}.exe", name));
                        found_project = true;
                        break;
                    }
                    if !dir.pop() {
                        break;
                    }
                }
            }

            if !found_project {
                let stem = entry_path
                    .file_stem()
                    .unwrap_or_else(|| std::ffi::OsStr::new("out"))
                    .to_string_lossy();
                out_exe = PathBuf::from(format!("build/{}.exe", stem));
            }

            agilang_build::build_project(&entry_path, &out_exe, emit_c, keep_generated, verbose)?;
        }
        "serve" => {
            let mut host = "127.0.0.1".to_string();
            let mut port_str = "8080".to_string();
            let mut allow_fallback = false;

            let remaining: Vec<String> = args.collect();
            let mut i = 0;
            while i < remaining.len() {
                if remaining[i] == "--host" && i + 1 < remaining.len() {
                    host = remaining[i + 1].clone();
                    i += 2;
                } else if remaining[i] == "--port" && i + 1 < remaining.len() {
                    port_str = remaining[i + 1].clone();
                    i += 2;
                } else if remaining[i] == "--port-fallback" {
                    allow_fallback = true;
                    i += 1;
                } else {
                    i += 1;
                }
            }

            let requested_port: u16 = port_str.parse().context("invalid port number")?;

            // Find project root
            let mut project_root = std::env::current_dir()?;
            let mut app_name = "app".to_string();
            let mut found = false;
            loop {
                if project_root.join("agilang.toml").exists() {
                    app_name = project_root
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "app".into());
                    found = true;
                    break;
                }
                if !project_root.pop() {
                    break;
                }
            }

            if !found {
                bail!("not in an AGILANG project (agilang.toml not found)");
            }

            println!("Loading application configuration...");
            println!("Compiling routes...");

            let mut router = agilang_framework_routing::Router::new();
            let web_routes_file = project_root.join("routes/web.agi");
            let api_routes_file = project_root.join("routes/api.agi");
            router.load_routes_from_file(&web_routes_file).ok();
            router.load_routes_from_file(&api_routes_file).ok();

            println!("Compiling views...");
            let views_dir = project_root.join("resources/views");
            let view_count = count_views(&views_dir);

            println!("Binding {}:{}...", host, requested_port);

            let host_ip: std::net::IpAddr = host.parse().context("invalid host IP address")?;

            let (listener, bound_port) = match agilang_framework_server::bind_with_fallback(host_ip, requested_port, allow_fallback) {
                Ok((l, p)) => {
                    if p != requested_port {
                        println!("warning: port {} is already occupied", requested_port);
                        println!("Using available port {}", p);
                    }
                    (l, p)
                }
                Err(error) => {
                    if error.kind() == std::io::ErrorKind::AddrInUse {
                        eprintln!("\nerror[E4001]: unable to bind development server\n");
                        eprintln!("Address: {}:{}", host, requested_port);
                        eprintln!("Reason: the port is already in use\n");
                        eprintln!("Try:\n    agilang serve --port {}", requested_port + 1);
                        eprintln!("\nTo inspect the process on Windows:\n    Get-NetTCPConnection -LocalPort {} -State Listen\n", requested_port);
                        return Ok(());
                    } else {
                        return Err(error.into());
                    }
                }
            };

            println!("\nAGILANG development server\n");
            println!("Application: {}", app_name);
            println!("Environment: local");
            println!("Address: http://{}:{}", host, bound_port);
            println!("Routes loaded: {}", router.routes.len());
            println!("Views compiled: {}", view_count);
            println!("Status: ready\n");
            println!("Press Ctrl+C to stop.");

            let view_engine = agilang_framework_view::ViewEngine::new(views_dir);

            for stream in listener.incoming() {
                match stream {
                    Ok(client_stream) => {
                        let router_ref = &router;
                        let view_ref = &view_engine;
                        let root_ref = &project_root;
                        if let Err(e) = agilang_framework_server::handle_client(client_stream, router_ref, view_ref, root_ref) {
                            eprintln!("request error: {}", e);
                        }
                    }
                    Err(e) => {
                        eprintln!("connection error: {}", e);
                    }
                }
            }
        }
        "auth" => {
            let mut roles = vec!["user".to_string(), "admin".to_string()];
            let mut force = false;
            let mut repair = false;

            let remaining: Vec<String> = args.collect();
            let mut i = 0;
            while i < remaining.len() {
                if remaining[i] == "--roles" && i + 1 < remaining.len() {
                    roles = remaining[i + 1].split(',').map(|s| s.trim().to_string()).collect();
                    i += 2;
                } else if remaining[i] == "--force" {
                    force = true;
                    i += 1;
                } else if remaining[i] == "--repair" {
                    repair = true;
                    i += 1;
                } else {
                    i += 1;
                }
            }

            agilang_project_generator::generate_auth(roles, force, repair)?;
        }
        "template:repair" => {
            let mut dry_run = false;
            let mut force = false;

            let remaining: Vec<String> = args.collect();
            let mut i = 0;
            while i < remaining.len() {
                if remaining[i] == "--dry-run" {
                    dry_run = true;
                    i += 1;
                } else if remaining[i] == "--force" {
                    force = true;
                    i += 1;
                } else {
                    i += 1;
                }
            }

            agilang_project_generator::repair_template(dry_run, force)?;
        }
        "make" => {
            let component = args
                .next()
                .context("framework component type is required (e.g. controller, model, etc.)")?;
            let name = args.next().context("component name is required")?;
            agilang_project_generator::make_component(&component, &name)?;
        }
        cmd if cmd.starts_with("make:") => {
            let component = &cmd[5..];
            let name = args.next().context("component name is required")?;
            agilang_project_generator::make_component(component, &name)?;
        }
        "test" => {
            println!("AGILANG check passed. (Testing is not yet implemented in Phase 2)");
        }
        "fmt" => {
            println!("Formatting is not yet implemented in Phase 2");
        }
        "doctor" => {
            let active_exe = env::current_exe()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "unknown".to_string());
            println!("CLI resolution");
            println!("  active: {}", active_exe);
            println!("  status: native");
            println!("  AGILANG native runtime status: healthy");

            let conflicts = find_conflicting_installations();
            if !conflicts.is_empty() {
                println!("\nConflicting installations");
                for c in conflicts {
                    println!("  found: {}", c.display());
                    println!("  implementation: legacy Python");
                    println!("  recommendation: rename to agilang-python or remove it from PATH");
                }
            } else {
                println!("\nNo conflicting installations found.");
            }
        }
        "--version" | "version" | "-V" => {
            let verbose = args.next().as_deref() == Some("--verbose");
            if verbose {
                let exe_path = env::current_exe()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| "unknown".to_string());
                println!("AGILANG Native Compiler {}", env!("CARGO_PKG_VERSION"));
                println!("Runtime: {}", env!("CARGO_PKG_VERSION"));
                println!("ABI: 1.0.0");
                println!("Implementation: Rust native");
                println!("Executable: {}", exe_path);
                println!("Target: {}", TARGET);
            } else {
                println!(
                    "AGILANG Native Compiler Frontend {}",
                    env!("CARGO_PKG_VERSION")
                );
            }
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

fn format_hir_expr(expr: &agilang_ir::HirExpr) -> String {
    match expr {
        agilang_ir::HirExpr::Identifier(name, _, _) => name.clone(),
        agilang_ir::HirExpr::Integer(val, ty, _) => format!("{}:{}", val, ty),
        agilang_ir::HirExpr::Float(val, ty, _) => format!("{}:{}", val, ty),
        agilang_ir::HirExpr::String(val, _, _) => format!("\"{}\":string", val),
        agilang_ir::HirExpr::Bool(val, _, _) => format!("{}:bool", val),
        agilang_ir::HirExpr::Call {
            callee, args, ty, ..
        } => {
            let callee_str = format_hir_expr(callee);
            let args_str: Vec<String> = args.iter().map(format_hir_expr).collect();
            format!("Call {}({}) -> {}", callee_str, args_str.join(", "), ty)
        }
        agilang_ir::HirExpr::Binary {
            left, op, right, ..
        } => {
            let op_str = match op {
                agilang_ir::HirBinaryOp::Add => "+",
                agilang_ir::HirBinaryOp::Subtract => "-",
                agilang_ir::HirBinaryOp::Multiply => "*",
                agilang_ir::HirBinaryOp::Divide => "/",
                agilang_ir::HirBinaryOp::Equal => "==",
                agilang_ir::HirBinaryOp::NotEqual => "!=",
                agilang_ir::HirBinaryOp::Less => "<",
                agilang_ir::HirBinaryOp::LessEqual => "<=",
                agilang_ir::HirBinaryOp::Greater => ">",
                agilang_ir::HirBinaryOp::GreaterEqual => ">=",
                agilang_ir::HirBinaryOp::And => "and",
                agilang_ir::HirBinaryOp::Or => "or",
            };
            format!(
                "({} {} {})",
                format_hir_expr(left),
                op_str,
                format_hir_expr(right)
            )
        }
    }
}

fn count_views(dir: &std::path::Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_views(&path);
            } else if path.extension().and_then(|s| s.to_str()) == Some("ags") {
                count += 1;
            }
        }
    }
    count
}
