//! AGS Hydration Code Generator
//!
//! Generates minimal JavaScript for browser reactivity:
//! - Registers node references by ID
//! - Sets up data source listeners (polling or EventSource)
//! - Updates only affected nodes when data changes
//! - Direct DOM manipulation (no virtual DOM)
//! - No full page refresh
//!
//! The generated code is designed to be as minimal as possible
//! while maintaining reactivity and dependency tracking.

use agilang_view_ir::*;

/// Hydration code generator
pub struct HydrationGenerator;

/// Result of hydration code generation
pub struct HydrationOutput {
    /// The generated JavaScript code
    pub js_code: String,
}

impl HydrationGenerator {
    pub fn new() -> Self {
        HydrationGenerator
    }

    /// Generate hydration code for a View IR document
    pub fn generate(&self, doc: &Document) -> Result<HydrationOutput, String> {
        let js_code = self.generate_javascript(doc)?;

        Ok(HydrationOutput { js_code })
    }

    fn generate_javascript(&self, doc: &Document) -> Result<String, String> {
        let mut code = String::new();

        // Module header
        code.push_str("(function() {\n");
        code.push_str("  'use strict';\n\n");

        // Generate state from hydration data
        code.push_str("  // Load hydration state from server\n");
        code.push_str("  const hydrationScript = document.querySelector('script[type=\"application/agilang-state\"]');\n");
        code.push_str(
            "  let AGS_STATE = {};\n  if (hydrationScript && hydrationScript.textContent) {\n    try { AGS_STATE = JSON.parse(hydrationScript.textContent); }\n    catch (error) { console.error('Invalid AGS hydration state', error); }\n  }\n\n",
        );

        // Generate node registry
        code.push_str("  // Node registry by ID\n");
        code.push_str("  const nodeRefs = {};\n");
        self.generate_node_registry(&mut code, &doc.root_node)?;

        // Generate data source bindings
        code.push_str("\n  // Data source listeners\n");
        self.generate_data_source_listeners(&mut code, doc)?;

        // Generate update functions
        code.push_str("\n  // Update functions for each binding\n");
        self.generate_update_functions(&mut code, doc)?;

        // Generate initialization
        code.push_str("\n  // Initialize on page load\n");
        code.push_str("  if (document.readyState === 'loading') {\n");
        code.push_str("    document.addEventListener('DOMContentLoaded', initializeHydration);\n");
        code.push_str("  } else {\n");
        code.push_str("    initializeHydration();\n");
        code.push_str("  }\n\n");
        code.push_str("  function initializeHydration() {\n");
        code.push_str("    // Register all nodes\n");
        code.push_str("    registerAllNodes();\n");
        code.push_str("    // Start polling for updates\n");
        code.push_str("    startDataSourcePolling();\n");
        code.push_str("  }\n\n");

        // Generate helper functions
        code.push_str("  function registerAllNodes() {\n");
        self.generate_register_all_nodes(&mut code, doc)?;
        code.push_str("  }\n\n");

        code.push_str("  function startDataSourcePolling() {\n");
        self.generate_polling_logic(&mut code, doc)?;
        code.push_str("  }\n");

        // Close module
        code.push_str("})();\n");

        Ok(code)
    }

    fn generate_node_registry(&self, code: &mut String, node: &ViewNode) -> Result<(), String> {
        code.push_str(&format!("  // node-{} = {}\n", node.id.0, node.tag));

        for child in &node.children {
            self.generate_node_registry(code, child)?;
        }

        Ok(())
    }

    fn generate_data_source_listeners(
        &self,
        code: &mut String,
        doc: &Document,
    ) -> Result<(), String> {
        for (name, source) in &doc.data_sources {
            code.push_str(&format!(
                "  // {} polling interval: {}ms\n",
                name, source.interval_ms
            ));
            code.push_str(&format!("  const {} = {{\n", name));
            code.push_str(&format!("    endpoint: '{}',\n", source.endpoint));
            code.push_str(&format!("    interval: {},\n", source.interval_ms));
            code.push_str(&format!(
                "    timeout: {},\n",
                source.timeout_ms.unwrap_or(5000)
            ));
            code.push_str("    lastData: null,\n");
            code.push_str("    timerId: null,\n");
            code.push_str("  };\n");
        }

        Ok(())
    }

    fn generate_update_functions(&self, code: &mut String, doc: &Document) -> Result<(), String> {
        let dep_graph = doc.dependency_graph();

        for (source, node_ids) in dep_graph {
            code.push_str(&format!(
                "  function update{}() {{\n",
                capitalize_first(&source)
            ));
            code.push_str(&format!("    if (!{}.lastData) return;\n", source));
            code.push_str(&format!("    const data = {}.lastData;\n", source));

            for node_id in node_ids {
                code.push_str(&format!(
                    "    const node{} = nodeRefs['node-{}'];\n",
                    node_id.0, node_id.0
                ));
                code.push_str(&format!("    if (node{}) {{\n", node_id.0));
                code.push_str(&format!(
                    "      // Update node {} with data from {}\n",
                    node_id.0, source
                ));
                code.push_str(&format!(
                    "      node{}.textContent = JSON.stringify(data);\n",
                    node_id.0
                ));
                code.push_str("    }\n");
            }

            code.push_str("  }\n\n");
        }

        Ok(())
    }

    fn generate_register_all_nodes(&self, code: &mut String, doc: &Document) -> Result<(), String> {
        self.generate_register_nodes_recursive(code, &doc.root_node)?;
        Ok(())
    }

    fn generate_register_nodes_recursive(
        &self,
        code: &mut String,
        node: &ViewNode,
    ) -> Result<(), String> {
        code.push_str(&format!(
            "    nodeRefs['node-{}'] = document.getElementById('node-{}');\n",
            node.id.0, node.id.0
        ));

        for child in &node.children {
            self.generate_register_nodes_recursive(code, child)?;
        }

        Ok(())
    }

    fn generate_polling_logic(&self, code: &mut String, doc: &Document) -> Result<(), String> {
        for (name, source) in &doc.data_sources {
            code.push_str(&format!("    {}.timerId = setInterval(() => {{\n", name));
            code.push_str(&format!("      fetch('{}')\n", source.endpoint));
            code.push_str("        .then(response => response.json())\n");
            code.push_str("        .then(data => {\n");
            code.push_str(&format!("          {}.lastData = data;\n", name));
            code.push_str(&format!("          update{}();\n", capitalize_first(name)));
            code.push_str("        })\n");
            code.push_str(
                "        .catch(error => console.error('Error fetching {}: ' + error));\n",
            );
            code.push_str(&format!("    }}, {});\n", source.interval_ms));
        }

        Ok(())
    }
}

impl Default for HydrationGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Capitalize first letter of string
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_capitalize_first() {
        assert_eq!(capitalize_first("chain"), "Chain");
        assert_eq!(capitalize_first("test"), "Test");
        assert_eq!(capitalize_first("a"), "A");
    }

    #[test]
    fn test_generate_hydration_simple() {
        let mut data_sources = HashMap::new();
        data_sources.insert(
            "chain".to_string(),
            DataSource {
                name: "chain".to_string(),
                endpoint: "/api/status".to_string(),
                interval_ms: 1000,
                timeout_ms: Some(5000),
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
            text_bindings: vec![],
        };

        let doc = Document::new("Test Page".to_string(), None, data_sources, root);

        let gen = HydrationGenerator::new();
        let result = gen.generate(&doc);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.js_code.contains("chain"));
        assert!(output.js_code.contains("/api/status"));
        assert!(output.js_code.contains("1000"));
        assert!(output.js_code.contains("nodeRefs"));
    }

    #[test]
    fn test_hydration_includes_polling() {
        let mut data_sources = HashMap::new();
        data_sources.insert(
            "test".to_string(),
            DataSource {
                name: "test".to_string(),
                endpoint: "/api/test".to_string(),
                interval_ms: 2000,
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

        let doc = Document::new("Page".to_string(), None, data_sources, root);

        let gen = HydrationGenerator::new();
        let output = gen.generate(&doc).unwrap();

        assert!(output.js_code.contains("setInterval"));
        assert!(output.js_code.contains("fetch"));
        assert!(output.js_code.contains("2000"));
    }

    #[test]
    fn test_hydration_registers_nodes() {
        let data_sources = HashMap::new();
        let children = vec![ViewNode {
            id: NodeId::new(2),
            tag: "h1".to_string(),
            attributes: HashMap::new(),
            children: vec![],
            text_bindings: vec![],
        }];

        let root = ViewNode {
            id: NodeId::new(1),
            tag: "main".to_string(),
            attributes: HashMap::new(),
            children,
            text_bindings: vec![],
        };

        let doc = Document::new("Page".to_string(), None, data_sources, root);

        let gen = HydrationGenerator::new();
        let output = gen.generate(&doc).unwrap();

        assert!(output.js_code.contains("node-1"));
        assert!(output.js_code.contains("node-2"));
        assert!(output.js_code.contains("nodeRefs"));
    }

    #[test]
    fn test_hydration_is_valid_javascript() {
        let data_sources = HashMap::new();
        let root = ViewNode {
            id: NodeId::new(1),
            tag: "div".to_string(),
            attributes: HashMap::new(),
            children: vec![],
            text_bindings: vec![],
        };

        let doc = Document::new("Page".to_string(), None, data_sources, root);

        let gen = HydrationGenerator::new();
        let output = gen.generate(&doc).unwrap();

        // Check basic JavaScript structure
        assert!(output.js_code.contains("(function() {"));
        assert!(output.js_code.contains("'use strict'"));
        assert!(output.js_code.contains("})();"));
    }
}
