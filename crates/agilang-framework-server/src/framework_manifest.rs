use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_MANIFEST_PATH: &str = "build/manifests/framework-manifest.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FrameworkManifest {
    pub generated_at_unix: u64,
    pub controllers: HashMap<String, ControllerManifest>,
    pub models: HashMap<String, ModelManifest>,
    pub requests: HashMap<String, RequestManifest>,
    pub policies: HashMap<String, PolicyManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ControllerManifest {
    pub kind: String,
    pub resource: Option<String>,
    pub actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelManifest {
    pub table: String,
    pub primary_key: String,
    pub fillable: Vec<String>,
    pub hidden: Vec<String>,
    pub casts: HashMap<String, String>,
    pub timestamps: bool,
    pub soft_deletes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequestManifest {
    pub rules: HashMap<String, Vec<String>>,
    pub validated_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PolicyManifest {
    pub actions: Vec<String>,
}

pub fn compile_framework_manifest(project_root: &Path) -> Result<FrameworkManifest> {
    let mut manifest = FrameworkManifest {
        generated_at_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_secs())
            .unwrap_or(0),
        ..FrameworkManifest::default()
    };

    let controllers_dir = project_root.join("app/Controllers");
    for file in agi_files(&controllers_dir)? {
        let content = fs::read_to_string(&file)?;
        let controller_name = relative_class_name(&controllers_dir, &file)?;
        let actions = parse_function_names(&content);
        let kind = if content.contains("\"repository\": \"orm\"") {
            "resource".to_string()
        } else {
            "source".to_string()
        };
        let resource = if kind == "resource" {
            controller_name
                .strip_suffix("Controller")
                .map(|value| value.to_string())
        } else {
            None
        };
        manifest.controllers.insert(
            controller_name,
            ControllerManifest {
                kind,
                resource,
                actions,
            },
        );
    }

    let models_dir = project_root.join("app/Models");
    for file in agi_files(&models_dir)? {
        let content = fs::read_to_string(&file)?;
        manifest.models.insert(
            relative_class_name(&models_dir, &file)?,
            ModelManifest {
                table: parse_string_assignment(&content, "table")
                    .unwrap_or_else(|| "records".to_string()),
                primary_key: parse_string_assignment(&content, "primary_key")
                    .unwrap_or_else(|| "id".to_string()),
                fillable: parse_list_assignment(&content, "fillable"),
                hidden: parse_list_assignment(&content, "hidden"),
                casts: parse_map_assignment(&content, "casts"),
                timestamps: parse_bool_assignment(&content, "timestamps").unwrap_or(false),
                soft_deletes: parse_bool_assignment(&content, "soft_deletes").unwrap_or(false),
            },
        );
    }

    let requests_dir = project_root.join("app/Requests");
    for file in agi_files(&requests_dir)? {
        let content = fs::read_to_string(&file)?;
        manifest.requests.insert(
            relative_class_name(&requests_dir, &file)?,
            RequestManifest {
                rules: parse_rules_map(&content),
                validated_fields: parse_list_after(&content, "fn validated_fields() -> list:"),
            },
        );
    }

    let policies_dir = project_root.join("app/Policies");
    for file in agi_files(&policies_dir)? {
        let content = fs::read_to_string(&file)?;
        manifest.policies.insert(
            relative_class_name(&policies_dir, &file)?,
            PolicyManifest {
                actions: parse_function_names(&content),
            },
        );
    }

    Ok(manifest)
}

pub fn write_framework_manifest(project_root: &Path) -> Result<PathBuf> {
    let manifest = compile_framework_manifest(project_root)?;
    let path = project_root.join(DEFAULT_MANIFEST_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, serde_json::to_vec_pretty(&manifest)?)?;
    Ok(path)
}

pub fn load_framework_manifest(project_root: &Path) -> Result<Option<FrameworkManifest>> {
    let path = project_root.join(DEFAULT_MANIFEST_PATH);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path)?;
    Ok(Some(serde_json::from_slice(&bytes)?))
}

fn agi_files(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    walk(dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk(&path, files)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("agi") {
            files.push(path);
        }
    }
    Ok(())
}

fn relative_class_name(base: &Path, file: &Path) -> Result<String> {
    let relative = file
        .strip_prefix(base)
        .map_err(|error| anyhow!(error.to_string()))?;
    let relative = relative.with_extension("");
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn parse_function_names(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let remainder = line.strip_prefix("fn ")?;
            Some(
                remainder
                    .split('(')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            )
        })
        .filter(|value| !value.is_empty())
        .collect()
}

fn parse_string_assignment(content: &str, name: &str) -> Option<String> {
    let marker = format!("{name} =");
    let index = content.find(&marker)?;
    let remainder = &content[index + marker.len()..];
    let start = remainder.find('"').or_else(|| remainder.find('\''))?;
    let rest = &remainder[start + 1..];
    let end = rest.find('"').or_else(|| rest.find('\''))?;
    Some(rest[..end].to_string())
}

fn parse_bool_assignment(content: &str, name: &str) -> Option<bool> {
    let marker = format!("{name} =");
    let index = content.find(&marker)?;
    let remainder = content[index + marker.len()..].trim_start();
    if remainder.starts_with("true") {
        Some(true)
    } else if remainder.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn parse_list_assignment(content: &str, name: &str) -> Vec<String> {
    let marker = format!("{name} =");
    let Some(index) = content.find(&marker) else {
        return Vec::new();
    };
    parse_bracket_list(&content[index + marker.len()..])
}

fn parse_map_assignment(content: &str, name: &str) -> HashMap<String, String> {
    let marker = format!("{name} =");
    let Some(index) = content.find(&marker) else {
        return HashMap::new();
    };
    let remainder = &content[index + marker.len()..];
    let Some(start) = remainder.find('{') else {
        return HashMap::new();
    };
    let Some(end) = remainder[start..].find('}') else {
        return HashMap::new();
    };
    split_top_level(&remainder[start + 1..start + end], ',')
        .into_iter()
        .filter_map(|entry| {
            let (key, value) = entry.split_once(':')?;
            Some((
                key.trim().trim_matches('"').trim_matches('\'').to_string(),
                value.trim().trim_matches('"').trim_matches('\'').to_string(),
            ))
        })
        .collect()
}

fn parse_rules_map(content: &str) -> HashMap<String, Vec<String>> {
    let marker = "fn rules() -> map:";
    let Some(index) = content.find(marker) else {
        return HashMap::new();
    };
    let remainder = &content[index + marker.len()..];
    let Some(start) = remainder.find('{') else {
        return HashMap::new();
    };
    let Some(end) = remainder[start..].find('}') else {
        return HashMap::new();
    };
    split_top_level(&remainder[start + 1..start + end], ',')
        .into_iter()
        .filter_map(|entry| {
            let (key, value) = entry.split_once(':')?;
            Some((
                key.trim().trim_matches('"').trim_matches('\'').to_string(),
                parse_inline_list(value),
            ))
        })
        .collect()
}

fn parse_list_after(content: &str, marker: &str) -> Vec<String> {
    let Some(index) = content.find(marker) else {
        return Vec::new();
    };
    parse_bracket_list(&content[index + marker.len()..])
}

fn parse_bracket_list(content: &str) -> Vec<String> {
    let Some(start) = content.find('[') else {
        return Vec::new();
    };
    let Some(end) = content[start..].find(']') else {
        return Vec::new();
    };
    parse_inline_list(&content[start..start + end + 1])
}

fn parse_inline_list(content: &str) -> Vec<String> {
    let trimmed = content.trim().trim_start_matches('[').trim_end_matches(']');
    split_top_level(trimmed, ',')
        .into_iter()
        .map(|value| value.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn split_top_level(content: &str, delimiter: char) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut curly = 0i32;
    let mut square = 0i32;
    let mut paren = 0i32;
    let mut in_single = false;
    let mut in_double = false;
    for ch in content.chars() {
        match ch {
            '"' if !in_single => in_double = !in_double,
            '\'' if !in_double => in_single = !in_single,
            '{' if !in_single && !in_double => curly += 1,
            '}' if !in_single && !in_double => curly -= 1,
            '[' if !in_single && !in_double => square += 1,
            ']' if !in_single && !in_double => square -= 1,
            '(' if !in_single && !in_double => paren += 1,
            ')' if !in_single && !in_double => paren -= 1,
            _ => {}
        }
        if ch == delimiter && !in_single && !in_double && curly == 0 && square == 0 && paren == 0
        {
            result.push(current.trim().to_string());
            current.clear();
            continue;
        }
        current.push(ch);
    }
    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }
    result
}
