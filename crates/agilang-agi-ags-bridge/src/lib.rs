//! Shared semantic metadata exported by AGI and consumed by AGS.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedType {
    pub module: String,
    pub name: String,
    pub fields: Vec<ExportedField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedField {
    pub name: String,
    pub ty: BridgeType,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeType {
    I32,
    I64,
    F32,
    F64,
    Bool,
    String,
    Array(Box<BridgeType>),
    Object(String),
}

#[derive(Debug, Clone, Default)]
pub struct TypeRegistry {
    types: HashMap<String, ExportedType>,
}

impl TypeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, exported: ExportedType) -> Result<(), String> {
        if self.types.contains_key(&exported.name) {
            return Err(format!("type `{}` is already registered", exported.name));
        }
        self.types.insert(exported.name.clone(), exported);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&ExportedType> {
        self.types.get(name)
    }
}

pub fn field(name: &str, ty: BridgeType) -> ExportedField {
    ExportedField {
        name: name.into(),
        ty,
        nullable: false,
    }
}

pub fn sibaq_type_registry() -> TypeRegistry {
    let mut registry = TypeRegistry::new();
    registry
        .register(ExportedType {
            module: "App.Blockchain.SibaqStatus".into(),
            name: "ChainStatus".into(),
            fields: vec![
                field("chain_id", BridgeType::I64),
                field("symbol", BridgeType::String),
                field("status", BridgeType::String),
                field("height", BridgeType::I64),
                field("finalized_height", BridgeType::I64),
                field("validator_count", BridgeType::I64),
                field("transaction_count", BridgeType::I64),
                field("consensus", BridgeType::String),
                field("head_hash", BridgeType::String),
                field("updated_at", BridgeType::String),
            ],
        })
        .expect("the built-in SIBAQ registry has unique types");
    registry
}

/// Extract exported struct metadata from an AGI source file.
///
/// This intentionally accepts only declarative struct fields. Methods and executable code are
/// ignored, keeping type extraction deterministic and independent from runtime initialization.
pub fn parse_agi_types(source: &str) -> Result<TypeRegistry, String> {
    let module = source
        .lines()
        .find_map(|line| line.trim().strip_prefix("module "))
        .unwrap_or("App")
        .trim()
        .to_string();
    let lines: Vec<&str> = source.lines().collect();
    let mut registry = TypeRegistry::new();
    let mut index = 0;
    while index < lines.len() {
        let trimmed = lines[index].trim();
        let Some(name) = trimmed
            .strip_prefix("struct ")
            .and_then(|value| value.strip_suffix(':'))
        else {
            index += 1;
            continue;
        };
        let struct_indent = indentation(lines[index]);
        index += 1;
        let mut fields = Vec::new();
        while index < lines.len() {
            let line = lines[index];
            let value = line.trim();
            if value.is_empty() || value.starts_with("//") {
                index += 1;
                continue;
            }
            if indentation(line) <= struct_indent {
                break;
            }
            let Some((field_name, type_source)) = value.split_once(':') else {
                index += 1;
                continue;
            };
            let type_source = type_source.split('=').next().unwrap_or(type_source).trim();
            let nullable = type_source.ends_with('?');
            let ty = parse_bridge_type(type_source.trim_end_matches('?'))?;
            fields.push(ExportedField {
                name: field_name.trim().to_string(),
                ty,
                nullable,
            });
            index += 1;
        }
        registry.register(ExportedType {
            module: module.clone(),
            name: name.trim().to_string(),
            fields,
        })?;
    }
    if registry.types.is_empty() {
        return Err("no AGI struct declarations were found".into());
    }
    Ok(registry)
}

fn indentation(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

fn parse_bridge_type(source: &str) -> Result<BridgeType, String> {
    if let Some(inner) = source
        .strip_prefix("array<")
        .and_then(|value| value.strip_suffix('>'))
    {
        return Ok(BridgeType::Array(Box::new(parse_bridge_type(inner)?)));
    }
    Ok(match source {
        "i32" => BridgeType::I32,
        "i64" => BridgeType::I64,
        "f32" => BridgeType::F32,
        "f64" => BridgeType::F64,
        "bool" => BridgeType::Bool,
        "string" => BridgeType::String,
        name if !name.is_empty() => BridgeType::Object(name.to_string()),
        _ => return Err("empty AGI field type".into()),
    })
}

pub fn suggest_field(exported: &ExportedType, unknown: &str) -> Option<String> {
    exported
        .fields
        .iter()
        .min_by_key(|field| edit_distance(&field.name, unknown))
        .filter(|field| edit_distance(&field.name, unknown) <= 3)
        .map(|field| field.name.clone())
}

fn edit_distance(left: &str, right: &str) -> usize {
    let mut row: Vec<usize> = (0..=right.len()).collect();
    for (i, a) in left.bytes().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, b) in right.bytes().enumerate() {
            let above = row[j + 1];
            row[j + 1] = if a == b {
                diagonal
            } else {
                1 + diagonal.min(above).min(row[j])
            };
            diagonal = above;
        }
    }
    row[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registers_sibaq_contract() {
        let registry = sibaq_type_registry();
        let chain = registry.get("ChainStatus").unwrap();
        assert_eq!(chain.module, "App.Blockchain.SibaqStatus");
        assert!(chain.fields.iter().any(|field| field.name == "height"));
    }
    #[test]
    fn suggests_misspelled_field() {
        let registry = sibaq_type_registry();
        assert_eq!(
            suggest_field(registry.get("ChainStatus").unwrap(), "heigth"),
            Some("height".into())
        );
    }

    #[test]
    fn extracts_structs_from_agi_source() {
        let registry = parse_agi_types("module App.Status\n\nstruct ChainStatus:\n    chain_id: i64\n    symbol: string\n    note: string?\n").unwrap();
        let exported = registry.get("ChainStatus").unwrap();
        assert_eq!(exported.module, "App.Status");
        assert_eq!(exported.fields.len(), 3);
        assert!(exported.fields[2].nullable);
    }
}
