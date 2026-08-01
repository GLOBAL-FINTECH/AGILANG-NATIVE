//! Orchestrates the native AGILANG frontend stages, including multi-file module resolution.
pub use agilang_ast::Program;
pub use agilang_diagnostics::Diagnostic;
pub use agilang_lexer::{Token, TokenKind};
pub use agilang_source::SourceFile;

use agilang_ast::{EnumDecl, Function, ImportDecl, ModuleDecl, StructDecl};
use agilang_source::Span;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub fn tokenize(source: &SourceFile) -> Result<Vec<Token>, Vec<Diagnostic>> {
    agilang_lexer::lex(source)
}

pub fn parse(source: &SourceFile) -> Result<Program, Vec<Diagnostic>> {
    let graph = build_module_graph(source)?;
    Ok(graph.program)
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

struct ModuleGraph {
    program: Program,
}

fn build_module_graph(entry: &SourceFile) -> Result<ModuleGraph, Vec<Diagnostic>> {
    let root = find_project_root(entry.path()).unwrap_or_else(|| {
        entry.path()
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    });
    let mut diagnostics = vec![];
    let mut visited = HashSet::new();
    let mut visiting = HashSet::new();
    let mut functions = vec![];
    let mut structs = vec![];
    let mut enums = vec![];
    let mut imports = vec![];
    let mut module_name = None;
    let mut module_order = HashMap::<PathBuf, usize>::new();

    load_module_recursive(
        entry,
        &root,
        true,
        &mut visited,
        &mut visiting,
        &mut module_order,
        &mut module_name,
        &mut imports,
        &mut structs,
        &mut enums,
        &mut functions,
        &mut diagnostics,
    );

    if diagnostics.is_empty() {
        Ok(ModuleGraph {
            program: Program {
                module_name,
                imports,
                structs,
                enums,
                functions,
            },
        })
    } else {
        Err(diagnostics)
    }
}

#[allow(clippy::too_many_arguments)]
fn load_module_recursive(
    source: &SourceFile,
    project_root: &Path,
    is_entry: bool,
    visited: &mut HashSet<PathBuf>,
    visiting: &mut HashSet<PathBuf>,
    module_order: &mut HashMap<PathBuf, usize>,
    entry_module_name: &mut Option<ModuleDecl>,
    collected_imports: &mut Vec<ImportDecl>,
    collected_structs: &mut Vec<StructDecl>,
    collected_enums: &mut Vec<EnumDecl>,
    collected_functions: &mut Vec<Function>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let canonical_path = source
        .path()
        .canonicalize()
        .unwrap_or_else(|_| source.path().to_path_buf());
    if visited.contains(&canonical_path) {
        return;
    }
    if visiting.contains(&canonical_path) {
        diagnostics.push(Diagnostic::error(
            "E3002",
            format!("circular import detected at {}", canonical_path.display()),
            Span::empty(0),
        ));
        return;
    }

    let tokens = match tokenize(source) {
        Ok(tokens) => tokens,
        Err(mut errs) => {
            diagnostics.append(&mut errs);
            return;
        }
    };
    let program = match agilang_parser::parse(&tokens) {
        Ok(program) => program,
        Err(mut errs) => {
            diagnostics.append(&mut errs);
            return;
        }
    };

    visiting.insert(canonical_path.clone());

    if let Some(module_decl) = &program.module_name {
        if let Some(expected_module) = module_name_for_path(project_root, &canonical_path) {
            if module_decl.path != expected_module {
                diagnostics.push(Diagnostic::error(
                    "E3007",
                    format!(
                        "module declaration `{}` does not match source path `{}`",
                        module_decl.path,
                        expected_module
                    ),
                    module_decl.span,
                ));
            }
        }
        if is_entry {
            *entry_module_name = Some(module_decl.clone());
        }
    }

    for import in &program.imports {
        let resolved_path = match resolve_import_path(project_root, &import.path) {
            Some(path) => path,
            None => {
                diagnostics.push(Diagnostic::error(
                    "E3001",
                    format!("module not found: {}", import.path),
                    import.span,
                ));
                continue;
            }
        };
        let imported_source = match SourceFile::load(&resolved_path) {
            Ok(source) => source,
            Err(_) => {
                diagnostics.push(Diagnostic::error(
                    "E3001",
                    format!("module not found: {}", import.path),
                    import.span,
                ));
                continue;
            }
        };
        load_module_recursive(
            &imported_source,
            project_root,
            false,
            visited,
            visiting,
            module_order,
            entry_module_name,
            collected_imports,
            collected_structs,
            collected_enums,
            collected_functions,
            diagnostics,
        );
    }

    visiting.remove(&canonical_path);
    visited.insert(canonical_path.clone());
    module_order.insert(canonical_path, module_order.len());

    if is_entry {
        collected_imports.extend(program.imports.clone());
    }
    collected_structs.extend(program.structs);
    collected_enums.extend(program.enums);
    collected_functions.extend(program.functions);
}

fn find_project_root(path: &Path) -> Option<PathBuf> {
    let mut dir = path.parent()?.to_path_buf();
    loop {
        if dir.join("agilang.toml").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn resolve_import_path(project_root: &Path, module_path: &str) -> Option<PathBuf> {
    let parts: Vec<&str> = module_path.split('.').collect();
    if parts.is_empty() {
        return None;
    }

    let mut candidates = vec![];
    if parts[0] == "App" {
        let mut path = project_root.join("app");
        for segment in &parts[1..parts.len().saturating_sub(1)] {
            path = path.join(segment);
        }
        path = path.join(format!("{}.agi", parts.last()?));
        candidates.push(path);
    }

    let mut direct = project_root.to_path_buf();
    for segment in &parts[..parts.len().saturating_sub(1)] {
        direct = direct.join(segment);
    }
    direct = direct.join(format!("{}.agi", parts.last()?));
    candidates.push(direct);

    candidates.into_iter().find(|candidate| candidate.exists())
}

fn module_name_for_path(project_root: &Path, source_path: &Path) -> Option<String> {
    let relative = source_path.strip_prefix(project_root).ok()?;
    let mut parts: Vec<String> = relative
        .iter()
        .map(|segment| segment.to_string_lossy().into_owned())
        .collect();
    if let Some(last) = parts.last_mut() {
        if let Some(stem) = Path::new(last).file_stem() {
            *last = stem.to_string_lossy().into_owned();
        }
    }
    if parts.first().is_some_and(|part| part.eq_ignore_ascii_case("app")) {
        parts[0] = "App".to_string();
    }
    Some(parts.join("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("agilang-compiler-{name}-{stamp}"))
    }

    #[test]
    fn parses_single_file_without_modules() {
        let source = SourceFile::new("main.agi", "fn main() -> i32:\n    return 0\n");
        let program = parse(&source).unwrap();
        assert!(program.module_name.is_none());
        assert!(program.imports.is_empty());
        assert!(program.structs.is_empty());
        assert!(program.enums.is_empty());
        assert_eq!(program.functions.len(), 1);
    }

    #[test]
    fn resolves_imported_functions_across_files() {
        let root = unique_temp_dir("imports");
        fs::create_dir_all(root.join("app").join("Services")).unwrap();
        fs::write(
            root.join("agilang.toml"),
            "[project]\nname='demo'\nversion='0.1.0'\n\n[application]\nentry='app/Main.agi'\n",
        )
        .unwrap();
        fs::write(
            root.join("app").join("Main.agi"),
            "module App.Main\nimport App.Services.MathService\n\nfn main() -> i32:\n    return multiply(6, 7)\n",
        )
        .unwrap();
        fs::write(
            root.join("app").join("Services").join("MathService.agi"),
            "module App.Services.MathService\n\nfn multiply(a: i32, b: i32) -> i32:\n    return a * b\n",
        )
        .unwrap();

        let source = SourceFile::load(root.join("app").join("Main.agi")).unwrap();
        let hir = hir(&source).unwrap();
        assert_eq!(hir.functions.len(), 2);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn resolves_imported_structs_across_files() {
        let root = unique_temp_dir("structs");
        fs::create_dir_all(root.join("app").join("Models")).unwrap();
        fs::write(
            root.join("agilang.toml"),
            "[project]\nname='demo'\nversion='0.1.0'\n\n[application]\nentry='app/Main.agi'\n",
        )
        .unwrap();
        fs::write(
            root.join("app").join("Main.agi"),
            "module App.Main\nimport App.Models.Point\n\nfn main() -> i32:\n    let point: Point = {x: 2, y: 5}\n    return point.x + point.y\n",
        )
        .unwrap();
        fs::write(
            root.join("app").join("Models").join("Point.agi"),
            "module App.Models.Point\n\nstruct Point:\n    x: i32\n    y: i32\n",
        )
        .unwrap();

        let source = SourceFile::load(root.join("app").join("Main.agi")).unwrap();
        let hir = hir(&source).unwrap();
        assert_eq!(hir.structs.len(), 1);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn resolves_imported_enums_across_files() {
        let root = unique_temp_dir("enums");
        fs::create_dir_all(root.join("app").join("Models")).unwrap();
        fs::write(
            root.join("agilang.toml"),
            "[project]\nname='demo'\nversion='0.1.0'\n\n[application]\nentry='app/Main.agi'\n",
        )
        .unwrap();
        fs::write(
            root.join("app").join("Main.agi"),
            "module App.Main\nimport App.Models.Status\n\nfn main() -> bool:\n    let status: Status = Status.Active\n    return status == Status.Active\n",
        )
        .unwrap();
        fs::write(
            root.join("app").join("Models").join("Status.agi"),
            "module App.Models.Status\n\nenum Status:\n    Pending\n    Active\n    Suspended\n",
        )
        .unwrap();

        let source = SourceFile::load(root.join("app").join("Main.agi")).unwrap();
        let hir = hir(&source).unwrap();
        assert_eq!(hir.enums.len(), 1);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn missing_import_returns_diagnostic() {
        let root = unique_temp_dir("missing-import");
        fs::create_dir_all(root.join("app")).unwrap();
        fs::write(
            root.join("agilang.toml"),
            "[project]\nname='demo'\nversion='0.1.0'\n\n[application]\nentry='app/Main.agi'\n",
        )
        .unwrap();
        fs::write(
            root.join("app").join("Main.agi"),
            "module App.Main\nimport App.Services.Missing\n\nfn main() -> i32:\n    return 0\n",
        )
        .unwrap();

        let source = SourceFile::load(root.join("app").join("Main.agi")).unwrap();
        let errs = parse(&source).unwrap_err();
        assert!(errs.iter().any(|err| err.code == "E3001"));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn cyclic_import_returns_diagnostic() {
        let root = unique_temp_dir("cyclic-import");
        fs::create_dir_all(root.join("app").join("Services")).unwrap();
        fs::write(
            root.join("agilang.toml"),
            "[project]\nname='demo'\nversion='0.1.0'\n\n[application]\nentry='app/Main.agi'\n",
        )
        .unwrap();
        fs::write(
            root.join("app").join("Main.agi"),
            "module App.Main\nimport App.Services.A\n\nfn main() -> i32:\n    return helper()\n",
        )
        .unwrap();
        fs::write(
            root.join("app").join("Services").join("A.agi"),
            "module App.Services.A\nimport App.Services.B\n\nfn helper() -> i32:\n    return helper_b()\n",
        )
        .unwrap();
        fs::write(
            root.join("app").join("Services").join("B.agi"),
            "module App.Services.B\nimport App.Services.A\n\nfn helper_b() -> i32:\n    return 1\n",
        )
        .unwrap();

        let source = SourceFile::load(root.join("app").join("Main.agi")).unwrap();
        let errs = parse(&source).unwrap_err();
        assert!(errs.iter().any(|err| err.code == "E3002"));
        fs::remove_dir_all(root).ok();
    }
}
