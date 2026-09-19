use anyhow::{bail, Result};
use std::{collections::{BTreeMap,BTreeSet},path::{Path,PathBuf}};

pub fn resolve(root:&Path, modules:&[String])->Result<BTreeMap<String,PathBuf>>{
    let mut out=BTreeMap::new();
    for name in modules {
        if name.contains("..") { bail!("module path escapes project root: {}",name); }
        let rel=name.replace('.','/')+".agi";
        let path=root.join(rel);
        if !path.starts_with(root) { bail!("module path escapes project root"); }
        out.insert(name.clone(),path);
    }
    Ok(out)
}
pub fn detect_cycles(graph:&BTreeMap<String,Vec<String>>)->Result<()>{
    fn visit(n:&str,g:&BTreeMap<String,Vec<String>>,temp:&mut BTreeSet<String>,done:&mut BTreeSet<String>)->Result<()>{
        if done.contains(n){return Ok(())} if !temp.insert(n.to_string()){bail!("module dependency cycle at {}",n)}
        if let Some(next)=g.get(n){for x in next{visit(x,g,temp,done)?;}}
        temp.remove(n); done.insert(n.to_string()); Ok(())
    }
    let mut t=BTreeSet::new();let mut d=BTreeSet::new();
    for n in graph.keys(){visit(n,graph,&mut t,&mut d)?;} Ok(())
}
