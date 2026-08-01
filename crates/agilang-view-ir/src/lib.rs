//! Typed View Intermediate Representation (IR)
//!
//! This module defines the IR produced after semantic analysis.
//! The IR is used for:
//! - Server-side rendering (SSR) code generation
//! - Browser hydration graph generation
//! - Reactive dependency tracking

use std::collections::HashMap;

/// Unique identifier for a node in the view graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

impl NodeId {
    pub fn new(id: u32) -> Self {
        NodeId(id)
    }
}

/// Data source binding (e.g., chain from "/api/status")
#[derive(Debug, Clone)]
pub struct DataSource {
    pub name: String,                   // "chain"
    pub endpoint: String,               // "/api/status"
    pub interval_ms: u64,               // 1000
    pub timeout_ms: Option<u64>,        // 5000
    pub retry_strategy: Option<String>, // "exponential"
    pub initial_server: bool,           // fetch during SSR?
    pub response_type: TypeInfo,        // The struct type returned
}

/// Type information for data sources and bindings
#[derive(Debug, Clone, PartialEq)]
pub enum TypeInfo {
    Primitive(PrimitiveType),
    Struct {
        name: String,
        fields: HashMap<String, TypeInfo>,
    },
    Array(Box<TypeInfo>),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrimitiveType {
    I32,
    I64,
    String,
    Bool,
}

impl TypeInfo {
    pub fn struct_field(&self, field_name: &str) -> Option<TypeInfo> {
        match self {
            TypeInfo::Struct { fields, .. } => fields.get(field_name).cloned(),
            _ => None,
        }
    }
}

/// A typed binding expression with resolved type information
#[derive(Debug, Clone)]
pub struct TypedBinding {
    pub id: NodeId,
    pub source: String,          // Data source name ("chain")
    pub field_path: Vec<String>, // Path segments ["height"]
    pub resolved_type: TypeInfo, // Resolved type: i64
    pub line: usize,
    pub column: usize,
}

/// An HTML view node with resolved bindings
#[derive(Debug, Clone)]
pub struct ViewNode {
    pub id: NodeId,
    pub tag: String,
    pub attributes: HashMap<String, String>,
    pub children: Vec<ViewNode>,
    pub text_bindings: Vec<TextBinding>,
}

/// Text content that may contain bindings
#[derive(Debug, Clone)]
pub enum TextBinding {
    Static(String),
    Dynamic(TypedBinding),
}

/// Complete typed IR document
#[derive(Debug, Clone)]
pub struct Document {
    pub title: String,
    pub seo_description: Option<String>,
    pub robots: Option<String>,
    pub data_sources: HashMap<String, DataSource>,
    pub root_node: ViewNode,
    pub all_bindings: Vec<TypedBinding>,
    pub next_node_id: u32,
}

impl Document {
    pub fn new(
        title: String,
        seo_description: Option<String>,
        data_sources: HashMap<String, DataSource>,
        root_node: ViewNode,
    ) -> Self {
        let all_bindings = Self::collect_bindings(&root_node);
        let next_node_id = all_bindings.iter().map(|b| b.id.0).max().unwrap_or(0) + 1;

        Document {
            title,
            seo_description,
            robots: None,
            data_sources,
            root_node,
            all_bindings,
            next_node_id,
        }
    }

    /// Collect all bindings from the view tree
    fn collect_bindings(node: &ViewNode) -> Vec<TypedBinding> {
        let mut bindings = Vec::new();

        for text_binding in &node.text_bindings {
            if let TextBinding::Dynamic(binding) = text_binding {
                bindings.push(binding.clone());
            }
        }

        for child in &node.children {
            bindings.extend(Self::collect_bindings(child));
        }

        bindings
    }

    /// Get bindings that depend on a specific data source
    pub fn bindings_for_source(&self, source: &str) -> Vec<&TypedBinding> {
        self.all_bindings
            .iter()
            .filter(|b| b.source == source)
            .collect()
    }

    /// Build dependency graph: which nodes update when a source changes
    pub fn dependency_graph(&self) -> HashMap<String, Vec<NodeId>> {
        let mut graph = HashMap::new();

        for binding in &self.all_bindings {
            graph
                .entry(binding.source.clone())
                .or_insert_with(Vec::new)
                .push(binding.id);
        }

        graph
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_info_struct_field() {
        let chain_type = TypeInfo::Struct {
            name: "ChainStatus".to_string(),
            fields: {
                let mut fields = HashMap::new();
                fields.insert(
                    "height".to_string(),
                    TypeInfo::Primitive(PrimitiveType::I64),
                );
                fields.insert(
                    "symbol".to_string(),
                    TypeInfo::Primitive(PrimitiveType::String),
                );
                fields
            },
        };

        let height_type = chain_type.struct_field("height");
        assert_eq!(height_type, Some(TypeInfo::Primitive(PrimitiveType::I64)));

        let missing = chain_type.struct_field("unknown");
        assert_eq!(missing, None);
    }

    #[test]
    fn test_create_document() {
        let root = ViewNode {
            id: NodeId(0),
            tag: "main".to_string(),
            attributes: HashMap::new(),
            children: Vec::new(),
            text_bindings: vec![TextBinding::Dynamic(TypedBinding {
                id: NodeId(1),
                source: "chain".to_string(),
                field_path: vec!["height".to_string()],
                resolved_type: TypeInfo::Primitive(PrimitiveType::I64),
                line: 1,
                column: 1,
            })],
        };

        let mut data_sources = HashMap::new();
        data_sources.insert(
            "chain".to_string(),
            DataSource {
                name: "chain".to_string(),
                endpoint: "/api/status".to_string(),
                interval_ms: 1000,
                timeout_ms: Some(5000),
                retry_strategy: Some("exponential".to_string()),
                initial_server: true,
                response_type: TypeInfo::Unknown,
            },
        );

        let doc = Document::new("Test".to_string(), None, data_sources, root);

        assert_eq!(doc.title, "Test");
        assert_eq!(doc.all_bindings.len(), 1);
    }

    #[test]
    fn test_dependency_graph() {
        let binding = TypedBinding {
            id: NodeId(1),
            source: "chain".to_string(),
            field_path: vec!["height".to_string()],
            resolved_type: TypeInfo::Primitive(PrimitiveType::I64),
            line: 1,
            column: 1,
        };

        let root = ViewNode {
            id: NodeId(0),
            tag: "main".to_string(),
            attributes: HashMap::new(),
            children: Vec::new(),
            text_bindings: vec![TextBinding::Dynamic(binding.clone())],
        };

        let doc = Document::new("Test".to_string(), None, HashMap::new(), root);

        let graph = doc.dependency_graph();
        assert!(graph.contains_key("chain"));
        assert_eq!(graph.get("chain"), Some(&vec![NodeId(1)]));
    }

    #[test]
    fn test_bindings_for_source() {
        let binding1 = TypedBinding {
            id: NodeId(1),
            source: "chain".to_string(),
            field_path: vec!["height".to_string()],
            resolved_type: TypeInfo::Primitive(PrimitiveType::I64),
            line: 1,
            column: 1,
        };

        let binding2 = TypedBinding {
            id: NodeId(2),
            source: "chain".to_string(),
            field_path: vec!["symbol".to_string()],
            resolved_type: TypeInfo::Primitive(PrimitiveType::String),
            line: 2,
            column: 1,
        };

        let root = ViewNode {
            id: NodeId(0),
            tag: "main".to_string(),
            attributes: HashMap::new(),
            children: Vec::new(),
            text_bindings: vec![
                TextBinding::Dynamic(binding1.clone()),
                TextBinding::Dynamic(binding2.clone()),
            ],
        };

        let doc = Document::new("Test".to_string(), None, HashMap::new(), root);

        let chain_bindings = doc.bindings_for_source("chain");
        assert_eq!(chain_bindings.len(), 2);
    }
}
