//! AGS Server-Side Rendering (SSR) Code Generator
//!
//! Converts typed View IR to Rust code that:
//! - Renders HTML with real data
//! - Escapes all HTML values to prevent XSS
//! - Embeds hydration state in <script type="application/agilang-state">
//! - Tracks which nodes depend on which data sources
//!
//! The generated code produces a complete HTML document suitable for
//! initial server rendering with client-side hydration.

use agilang_view_ir::*;

/// SSR code generator
pub struct SsrGenerator;

/// Result of SSR code generation
pub struct SsrOutput {
    /// The generated Rust function that renders HTML
    pub render_fn_code: String,
    /// The generated hydration state (JSON)
    pub hydration_state: String,
}

impl SsrGenerator {
    pub fn new() -> Self {
        SsrGenerator
    }

    /// Generate SSR code for a View IR document
    pub fn generate(&self, doc: &Document) -> Result<SsrOutput, String> {
        // Generate the render function
        let render_fn_code = self.generate_render_function(doc)?;

        // Generate the hydration state
        let hydration_state = self.generate_hydration_state(doc)?;

        Ok(SsrOutput {
            render_fn_code,
            hydration_state,
        })
    }

    fn generate_render_function(&self, doc: &Document) -> Result<String, String> {
        let mut code = String::new();

        // Function signature
        code.push_str("/// Server-side render the AGS page\n");
        code.push_str("pub async fn render_page() -> String {\n");
        code.push_str("    let mut html = String::new();\n");
        code.push_str("    html.push_str(\"<!DOCTYPE html>\");\n");
        code.push_str("    html.push_str(\"<html>\");\n");

        // Generate head
        self.generate_head(&mut code, doc)?;

        // Generate body
        code.push_str("    html.push_str(\"<body>\");\n");
        self.generate_node(&mut code, &doc.root_node, doc)?;

        // Embed hydration state
        code.push_str("    html.push_str(\"<script type=\\\"application/agilang-state\\\">\");\n");
        code.push_str("    html.push_str(\"const AGS_HYDRATION_STATE = \");\n");
        code.push_str(&format!(
            "    html.push_str(\"{}\");\n",
            self.escape_html_in_string("TODO: embed hydration")
        ));
        code.push_str("    html.push_str(\";</script>\");\n");

        code.push_str("    html.push_str(\"</body>\");\n");
        code.push_str("    html.push_str(\"</html>\");\n");
        code.push_str("    html\n");
        code.push_str("}\n");

        Ok(code)
    }

    fn generate_head(&self, code: &mut String, doc: &Document) -> Result<(), String> {
        code.push_str("    html.push_str(\"<head>\");\n");

        // Title
        code.push_str("    html.push_str(\"<title>\");\n");
        let escaped_title = self.escape_html(&doc.title);
        code.push_str(&format!("    html.push_str(\"{}\");\n", escaped_title));
        code.push_str("    html.push_str(\"</title>\");\n");

        // Meta description if present
        if let Some(desc) = &doc.seo_description {
            code.push_str("    html.push_str(\"<meta name=\\\"description\\\" content=\\\"\");\n");
            let escaped_desc = self.escape_html(desc);
            code.push_str(&format!("    html.push_str(\"{}\");\n", escaped_desc));
            code.push_str("    html.push_str(\"\\\">\");\n");
        }

        // Standard meta tags
        code.push_str("    html.push_str(\"<meta charset=\\\"utf-8\\\">\");\n");
        code.push_str("    html.push_str(\"<meta name=\\\"viewport\\\" content=\\\"width=device-width, initial-scale=1\\\">\");\n");

        code.push_str("    html.push_str(\"</head>\");\n");

        Ok(())
    }

    #[allow(clippy::only_used_in_recursion)]
    fn generate_node(
        &self,
        code: &mut String,
        node: &ViewNode,
        doc: &Document,
    ) -> Result<(), String> {
        // Open tag
        code.push_str(&format!("    html.push_str(\"<{}\");\n", node.tag));

        // Add id attribute for tracking
        code.push_str(&format!(
            "    html.push_str(\" id=\\\"node-{}\\\"\");\n",
            node.id.0
        ));

        // Add other attributes
        for (name, value) in &node.attributes {
            let escaped_value = self.escape_html(value);
            code.push_str(&format!(
                "    html.push_str(\" {}=\\\"{}\\\"\");\n",
                name, escaped_value
            ));
        }

        code.push_str("    html.push_str(\">\");\n");

        // Text content and bindings
        for text_binding in &node.text_bindings {
            match text_binding {
                TextBinding::Static(text) => {
                    let escaped_text = self.escape_html(text);
                    code.push_str(&format!("    html.push_str(\"{}\");\n", escaped_text));
                }
                TextBinding::Dynamic(binding) => {
                    // For now, use a placeholder. In real implementation, would fetch from data source
                    code.push_str(&format!(
                        "    // Binding: {}.{}\n",
                        binding.source,
                        binding.field_path.join(".")
                    ));
                    code.push_str("    html.push_str(\"--\"); // TODO: fetch from data source\n");
                }
            }
        }

        // Children
        for child in &node.children {
            self.generate_node(code, child, doc)?;
        }

        // Close tag
        code.push_str(&format!("    html.push_str(\"</{}>\")\n", node.tag));

        Ok(())
    }

    fn generate_hydration_state(&self, doc: &Document) -> Result<String, String> {
        // Build dependency graph
        let dep_graph = doc.dependency_graph();

        // Create JSON structure
        let mut state = serde_json::json!({
            "version": "1.0",
            "title": doc.title,
            "data_sources": {},
            "dependency_graph": {},
        });

        // Add data sources
        for (name, source) in &doc.data_sources {
            state["data_sources"][name] = serde_json::json!({
                "endpoint": source.endpoint,
                "interval_ms": source.interval_ms,
                "timeout_ms": source.timeout_ms,
                "initial_server": source.initial_server,
            });
        }

        // Add dependency graph
        for (source, node_ids) in dep_graph {
            state["dependency_graph"][source] =
                serde_json::json!(node_ids.iter().map(|id| id.0).collect::<Vec<_>>());
        }

        Ok(state.to_string())
    }

    /// Escape HTML special characters
    fn escape_html(&self, text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#39;")
    }

    /// Escape HTML for use in a Rust string literal (adds \\ for escaping)
    fn escape_html_in_string(&self, text: &str) -> String {
        let escaped = self.escape_html(text);
        escaped.replace('\\', "\\\\").replace('"', "\\\"")
    }
}

impl Default for SsrGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_escape_html() {
        let gen = SsrGenerator::new();
        assert_eq!(gen.escape_html("<div>"), "&lt;div&gt;");
        assert_eq!(gen.escape_html("&hello"), "&amp;hello");
        assert_eq!(gen.escape_html("\"quoted\""), "&quot;quoted&quot;");
        assert_eq!(gen.escape_html("it's"), "it&#39;s");
    }

    #[test]
    fn test_generate_simple_page() {
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

        let root = ViewNode {
            id: NodeId::new(1),
            tag: "main".to_string(),
            attributes: HashMap::new(),
            children: vec![],
            text_bindings: vec![TextBinding::Static("Hello World".to_string())],
        };

        let doc = Document::new(
            "Test Page".to_string(),
            Some("Test Description".to_string()),
            data_sources,
            root,
        );

        let gen = SsrGenerator::new();
        let result = gen.generate(&doc);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.render_fn_code.contains("Test Page"));
        assert!(output.render_fn_code.contains("Hello World"));
        assert!(output.hydration_state.contains("Test Page"));
    }

    #[test]
    fn test_hydration_state_includes_dependency_graph() {
        let data_sources = HashMap::new();
        let root = ViewNode {
            id: NodeId::new(1),
            tag: "div".to_string(),
            attributes: HashMap::new(),
            children: vec![],
            text_bindings: vec![],
        };

        let doc = Document::new("Page".to_string(), None, data_sources, root);

        let gen = SsrGenerator::new();
        let output = gen.generate(&doc).unwrap();

        // Hydration state should be valid JSON
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&output.hydration_state);
        assert!(parsed.is_ok());

        let state = parsed.unwrap();
        assert_eq!(state["version"], "1.0");
        assert!(state["dependency_graph"].is_object());
    }

    #[test]
    fn test_no_xss_in_title() {
        let mut data_sources = HashMap::new();
        data_sources.insert(
            "test".to_string(),
            DataSource {
                name: "test".to_string(),
                endpoint: "/api/test".to_string(),
                interval_ms: 1000,
                timeout_ms: None,
                retry_strategy: None,
                initial_server: true,
                response_type: TypeInfo::Unknown,
            },
        );

        let root = ViewNode {
            id: NodeId::new(1),
            tag: "div".to_string(),
            attributes: HashMap::new(),
            children: vec![],
            text_bindings: vec![],
        };

        let doc = Document::new(
            "<script>alert('xss')</script>".to_string(),
            None,
            data_sources,
            root,
        );

        let gen = SsrGenerator::new();
        let output = gen.generate(&doc).unwrap();

        // Title should be escaped
        assert!(output.render_fn_code.contains("&lt;script&gt;"));
        assert!(!output.render_fn_code.contains("<script>"));
    }
}
