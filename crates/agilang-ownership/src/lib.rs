use agilang_diagnostics::Diagnostic;
use agilang_ir::{HirExpr,HirProgram,HirStmt};
use std::collections::HashSet;

pub fn validate(program:&HirProgram)->Vec<Diagnostic>{
    let mut errors=Vec::new();
    for f in &program.functions {
        let mut immutable=HashSet::new();
        for s in &f.body { validate_stmt(s,&mut immutable,&mut errors); }
    }
    errors
}
fn validate_stmt(s:&HirStmt, immutable:&mut HashSet<String>, errors:&mut Vec<Diagnostic>){
    match s {
        HirStmt::Let{name,mutable,..}=>{if !mutable{immutable.insert(name.clone());}}
        HirStmt::Assign{target,span,..}=>{
            if let HirExpr::Identifier(name,_,_) = target {
                if immutable.contains(name) {
                    errors.push(Diagnostic::error("E3001","cannot assign to immutable binding",*span));
                }
            }
        }
        HirStmt::If{then_body,else_body,..}=>{for x in then_body{validate_stmt(x,immutable,errors)} for x in else_body{validate_stmt(x,immutable,errors)}}
        HirStmt::While{body,..}|HirStmt::ForIn{body,..}=>for x in body{validate_stmt(x,immutable,errors)},
        HirStmt::Match{arms,..}=>for a in arms{for x in &a.body{validate_stmt(x,immutable,errors)}},
        _=>{}
    }
}
