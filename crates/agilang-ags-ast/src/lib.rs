//! AST definitions for AGS (Agilang View Language)

use std::collections::HashMap;

/// Root document structure containing directives and template
#[derive(Debug, Clone)]
pub struct Document {
    pub directives: Vec<Directive>,
    pub root: ViewNode,
}

/// Top-level directives (@page, @live, etc.)
#[derive(Debug, Clone)]
pub enum Directive {
    Page {
        title: String,
        seo_description: Option<String>,
        robots: Option<String>,
    },
    Fetch {
        name: String,
        response_type: Option<String>,
        endpoint: String,
    },
    Live {
        name: String,
        response_type: Option<String>,
        endpoint: String,
        interval_ms: u64,
        timeout_ms: Option<u64>,
        retry_strategy: Option<String>, // "exponential", etc.
        initial_server: bool,
    },
    Loading {
        binding_name: String,
    },
    Error {
        binding_name: String,
        error_var: Option<String>, // "as error"
    },
    Stale {
        binding_name: String,
    },
}

/// HTML view node with bindings and attributes
#[derive(Debug, Clone)]
pub struct ViewNode {
    pub tag: String,                         // "main", "div", "h1", etc.
    pub attributes: HashMap<String, String>, // class, id, etc.
    pub children: Vec<ViewNode>,
    pub text_content: Option<Vec<TextOrBinding>>,
    pub line: usize,
    pub column: usize,
}

/// Text content or binding expression
#[derive(Debug, Clone)]
pub enum TextOrBinding {
    Text(String),
    Binding(Binding),
}

/// Binding expression {{ source.field.path }}
#[derive(Debug, Clone)]
pub struct Binding {
    pub expression: String, // "chain.height", "chain.symbol", etc.
    pub line: usize,
    pub column: usize,
}

impl ViewNode {
    pub fn new(tag: &str, line: usize, column: usize) -> Self {
        ViewNode {
            tag: tag.to_string(),
            attributes: HashMap::new(),
            children: Vec::new(),
            text_content: None,
            line,
            column,
        }
    }

    pub fn with_attribute(mut self, key: String, value: String) -> Self {
        self.attributes.insert(key, value);
        self
    }

    pub fn add_child(mut self, child: ViewNode) -> Self {
        self.children.push(child);
        self
    }

    pub fn set_text_content(mut self, content: Vec<TextOrBinding>) -> Self {
        self.text_content = Some(content);
        self
    }
}

impl Document {
    pub fn new(directives: Vec<Directive>, root: ViewNode) -> Self {
        Document { directives, root }
    }

    pub fn find_directive<F>(&self, predicate: F) -> Option<&Directive>
    where
        F: Fn(&Directive) -> bool,
    {
        self.directives.iter().find(|d| predicate(d))
    }

    pub fn get_live_directives(&self) -> Vec<&Directive> {
        self.directives
            .iter()
            .filter(|d| matches!(d, Directive::Fetch { .. } | Directive::Live { .. }))
            .collect()
    }

    pub fn get_page_directive(&self) -> Option<&Directive> {
        self.find_directive(|d| matches!(d, Directive::Page { .. }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_document() {
        let root = ViewNode::new("main", 1, 1).add_child(ViewNode::new("h1", 2, 1));

        let directives = vec![Directive::Page {
            title: "Test".to_string(),
            seo_description: None,
            robots: None,
        }];

        let doc = Document::new(directives, root);

        assert_eq!(doc.directives.len(), 1);
        assert_eq!(doc.root.tag, "main");
        assert_eq!(doc.root.children.len(), 1);
    }

    #[test]
    fn test_view_node_builder() {
        let node = ViewNode::new("div", 1, 1)
            .with_attribute("class".to_string(), "container".to_string())
            .with_attribute("id".to_string(), "main".to_string());

        assert_eq!(node.attributes.get("class"), Some(&"container".to_string()));
        assert_eq!(node.attributes.get("id"), Some(&"main".to_string()));
    }

    #[test]
    fn test_binding_expression() {
        let binding = Binding {
            expression: "chain.height".to_string(),
            line: 5,
            column: 20,
        };

        assert_eq!(binding.expression, "chain.height");
        assert_eq!(binding.line, 5);
        assert_eq!(binding.column, 20);
    }

    #[test]
    fn test_get_live_directives() {
        let directives = vec![
            Directive::Page {
                title: "Test".to_string(),
                seo_description: None,
                robots: None,
            },
            Directive::Live {
                name: "chain".to_string(),
                response_type: Some("ChainStatus".to_string()),
                endpoint: "/api/status".to_string(),
                interval_ms: 1000,
                timeout_ms: Some(5000),
                retry_strategy: Some("exponential".to_string()),
                initial_server: true,
            },
        ];

        let root = ViewNode::new("main", 1, 1);
        let doc = Document::new(directives, root);

        let live_dirs = doc.get_live_directives();
        assert_eq!(live_dirs.len(), 1);
    }
}
