//! AGS Semantic Analyzer
//!
//! Validates AST against type information and produces Typed View IR

use agilang_agi_ags_bridge::{suggest_field, BridgeType, TypeRegistry as BridgeRegistry};
use agilang_ags_ast::*;
use agilang_view_ir::*;
use std::collections::HashMap;

pub struct SemanticAnalyzer {
    type_registry: TypeRegistry,
}

/// Type registry with known types (can be populated from AGI)
#[derive(Clone)]
pub struct TypeRegistry {
    pub types: HashMap<String, TypeInfo>,
    bridge: BridgeRegistry,
}

impl TypeRegistry {
    pub fn new() -> Self {
        TypeRegistry {
            types: HashMap::new(),
            bridge: BridgeRegistry::new(),
        }
    }

    pub fn register_struct(mut self, name: &str, fields: HashMap<String, TypeInfo>) -> Self {
        self.types.insert(
            name.to_string(),
            TypeInfo::Struct {
                name: name.to_string(),
                fields,
            },
        );
        self
    }

    pub fn get_type(&self, name: &str) -> Option<TypeInfo> {
        self.types.get(name).cloned()
    }

    pub fn from_bridge(bridge: BridgeRegistry) -> Self {
        Self {
            types: HashMap::new(),
            bridge,
        }
    }
}

impl Default for TypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticAnalyzer {
    pub fn new(type_registry: TypeRegistry) -> Self {
        SemanticAnalyzer { type_registry }
    }

    pub fn analyze(
        &self,
        ast: &agilang_ags_ast::Document,
    ) -> Result<agilang_view_ir::Document, String> {
        // Extract directives
        let mut page_title = "Page".to_string();
        let mut page_description = None;
        let mut page_robots = None;
        let mut data_sources: HashMap<String, DataSource> = HashMap::new();

        for directive in &ast.directives {
            match directive {
                Directive::Page {
                    title,
                    seo_description,
                    robots,
                } => {
                    page_title = title.clone();
                    page_description = seo_description.clone();
                    page_robots = robots.clone();
                }
                Directive::Fetch {
                    name,
                    response_type,
                    endpoint,
                } => {
                    let resolved_response = response_type
                        .as_deref()
                        .map(|name| self.resolve_exported_type(name))
                        .transpose()?
                        .unwrap_or(TypeInfo::Unknown);
                    data_sources
                        .entry(name.clone())
                        .and_modify(|source| source.initial_server = true)
                        .or_insert_with(|| DataSource {
                            name: name.clone(),
                            endpoint: endpoint.clone(),
                            interval_ms: 0,
                            timeout_ms: None,
                            retry_strategy: None,
                            initial_server: true,
                            response_type: resolved_response,
                        });
                }
                Directive::Live {
                    name,
                    response_type,
                    endpoint,
                    interval_ms,
                    timeout_ms,
                    retry_strategy,
                    initial_server,
                } => {
                    let resolved_response = response_type
                        .as_deref()
                        .map(|name| self.resolve_exported_type(name))
                        .transpose()?
                        .unwrap_or(TypeInfo::Unknown);
                    let fetched_initially = data_sources
                        .get(name)
                        .is_some_and(|source| source.initial_server);
                    data_sources.insert(
                        name.clone(),
                        DataSource {
                            name: name.clone(),
                            endpoint: endpoint.clone(),
                            interval_ms: *interval_ms,
                            timeout_ms: *timeout_ms,
                            retry_strategy: retry_strategy.clone(),
                            initial_server: *initial_server || fetched_initially,
                            response_type: resolved_response,
                        },
                    );
                }
                _ => {}
            }
        }

        // Convert AST tree to IR tree with type checking
        let root_node = self.convert_node(&ast.root, &data_sources)?;

        // Build IR document
        let mut document =
            agilang_view_ir::Document::new(page_title, page_description, data_sources, root_node);
        document.robots = page_robots;
        Ok(document)
    }

    fn convert_node(
        &self,
        node: &agilang_ags_ast::ViewNode,
        data_sources: &HashMap<String, DataSource>,
    ) -> Result<agilang_view_ir::ViewNode, String> {
        let mut ir_node = agilang_view_ir::ViewNode {
            id: NodeId::new(node.line as u32 * 1000 + node.column as u32),
            tag: node.tag.clone(),
            attributes: node.attributes.clone(),
            children: Vec::new(),
            text_bindings: Vec::new(),
        };

        // Convert text content and bindings
        if let Some(text_content) = &node.text_content {
            for item in text_content {
                match item {
                    TextOrBinding::Text(t) => {
                        ir_node
                            .text_bindings
                            .push(agilang_view_ir::TextBinding::Static(t.clone()));
                    }
                    TextOrBinding::Binding(binding) => {
                        let typed_binding = self.type_check_binding(binding, data_sources)?;
                        ir_node
                            .text_bindings
                            .push(agilang_view_ir::TextBinding::Dynamic(typed_binding));
                    }
                }
            }
        }

        // Convert children
        for child in &node.children {
            ir_node
                .children
                .push(self.convert_node(child, data_sources)?);
        }

        Ok(ir_node)
    }

    fn type_check_binding(
        &self,
        binding: &Binding,
        data_sources: &HashMap<String, DataSource>,
    ) -> Result<TypedBinding, String> {
        // Parse binding expression: "chain.height" → ("chain", ["height"])
        let parts: Vec<&str> = binding.expression.split('.').collect();
        if parts.is_empty() {
            return Err(format!(
                "Invalid binding expression at {}:{}: '{}'",
                binding.line, binding.column, binding.expression
            ));
        }

        let source_name = parts[0];
        let field_path: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

        // Verify data source exists
        let data_source = data_sources
            .get(source_name)
            .ok_or_else(|| format!("Unknown data source: '{}'", source_name))?;

        let mut resolved_type = data_source.response_type.clone();
        for field in &field_path {
            if resolved_type == TypeInfo::Unknown {
                break;
            }
            resolved_type = resolved_type.struct_field(field).ok_or_else(|| {
                let type_name = match &resolved_type {
                    TypeInfo::Struct { name, .. } => name.as_str(),
                    _ => "unknown",
                };
                let suggestion = self
                    .type_registry
                    .bridge
                    .get(type_name)
                    .and_then(|exported| suggest_field(exported, field));
                format!(
                    "AGS-E2301: unknown field `{field}` on `{type_name}` at {}:{}{}",
                    binding.line,
                    binding.column,
                    suggestion
                        .map(|name| format!("\nDid you mean `{name}`?"))
                        .unwrap_or_default()
                )
            })?;
        }

        Ok(TypedBinding {
            id: NodeId::new(binding.line as u32 * 1000 + binding.column as u32),
            source: source_name.to_string(),
            field_path,
            resolved_type,
            line: binding.line,
            column: binding.column,
        })
    }

    fn resolve_exported_type(&self, name: &str) -> Result<TypeInfo, String> {
        let exported = self
            .type_registry
            .bridge
            .get(name)
            .ok_or_else(|| format!("AGS-E2300: unknown live response type `{name}`"))?;
        let fields = exported
            .fields
            .iter()
            .map(|field| Ok((field.name.clone(), self.bridge_type_to_ir(&field.ty)?)))
            .collect::<Result<HashMap<_, _>, String>>()?;
        Ok(TypeInfo::Struct {
            name: exported.name.clone(),
            fields,
        })
    }

    fn bridge_type_to_ir(&self, ty: &BridgeType) -> Result<TypeInfo, String> {
        Ok(match ty {
            BridgeType::I32 => TypeInfo::Primitive(PrimitiveType::I32),
            BridgeType::I64 => TypeInfo::Primitive(PrimitiveType::I64),
            BridgeType::Bool => TypeInfo::Primitive(PrimitiveType::Bool),
            BridgeType::String => TypeInfo::Primitive(PrimitiveType::String),
            BridgeType::Array(inner) => TypeInfo::Array(Box::new(self.bridge_type_to_ir(inner)?)),
            BridgeType::Object(name) => self.resolve_exported_type(name)?,
            BridgeType::F32 | BridgeType::F64 => TypeInfo::Unknown,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agilang_ags_ast::Directive;

    #[test]
    fn test_semantic_analysis_basic() {
        let directives = vec![
            Directive::Page {
                title: "Test".to_string(),
                seo_description: None,
                robots: None,
            },
            Directive::Live {
                name: "chain".to_string(),
                response_type: None,
                endpoint: "/api/status".to_string(),
                interval_ms: 1000,
                timeout_ms: Some(5000),
                retry_strategy: None,
                initial_server: true,
            },
        ];

        let root = agilang_ags_ast::ViewNode::new("main", 1, 1);
        let ast = agilang_ags_ast::Document::new(directives, root);

        let analyzer = SemanticAnalyzer::new(TypeRegistry::new());
        let ir = analyzer.analyze(&ast);

        assert!(ir.is_ok());
        let doc = ir.unwrap();
        assert_eq!(doc.title, "Test");
        assert!(doc.data_sources.contains_key("chain"));
    }

    #[test]
    fn test_type_check_binding() {
        let analyzer = SemanticAnalyzer::new(TypeRegistry::new());
        let mut data_sources = HashMap::new();
        data_sources.insert(
            "chain".to_string(),
            DataSource {
                name: "chain".to_string(),
                endpoint: "/api/status".to_string(),
                interval_ms: 1000,
                timeout_ms: None,
                retry_strategy: None,
                initial_server: true,
                response_type: TypeInfo::Unknown,
            },
        );

        let binding = Binding {
            expression: "chain.height".to_string(),
            line: 1,
            column: 1,
        };

        let result = analyzer.type_check_binding(&binding, &data_sources);
        assert!(result.is_ok());

        let typed = result.unwrap();
        assert_eq!(typed.source, "chain");
        assert_eq!(typed.field_path, vec!["height"]);
        assert_eq!(typed.resolved_type, TypeInfo::Unknown);
    }

    #[test]
    fn test_unknown_data_source() {
        let analyzer = SemanticAnalyzer::new(TypeRegistry::new());
        let data_sources = HashMap::new();

        let binding = Binding {
            expression: "unknown_source.field".to_string(),
            line: 1,
            column: 1,
        };

        let result = analyzer.type_check_binding(&binding, &data_sources);
        assert!(result.is_err());
    }
}
