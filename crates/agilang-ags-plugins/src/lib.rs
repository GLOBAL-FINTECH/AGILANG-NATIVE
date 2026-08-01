//! AGS Plugin System
//!
//! Provides extensibility for AGS templates through plugins:
//! - Custom binding transformations
//! - Event handler registration
//! - Data source middleware
//! - View node processors
//! - Code generation hooks
//!
//! Plugins are registered during compilation and can modify:
//! - AST after parsing
//! - IR after semantic analysis
//! - Generated code before compilation
//! - JavaScript hydration logic
//!
//! Design: Plugins implement traits and are discovered via registry.
//! Plugins are stateless (no shared state between plugins).

use agilang_view_ir::*;
use std::any::Any;
use std::collections::HashMap;

/// Plugin trait: Core interface for AGS plugins
pub trait AgsPlugin: Send + Sync {
    /// Plugin name for logging and debugging
    fn name(&self) -> &str;

    /// Plugin version
    fn version(&self) -> &str;

    /// Called after AST parsing
    fn on_ast_parsed(&self, _ast: &mut crate::ast::Document) -> Result<(), String> {
        Ok(())
    }

    /// Called after semantic analysis (IR generation)
    fn on_ir_generated(&self, _ir: &Document) -> Result<(), String> {
        Ok(())
    }

    /// Called before SSR code generation
    fn on_pre_ssr(&self, _ir: &Document) -> Result<(), String> {
        Ok(())
    }

    /// Called before hydration code generation
    fn on_pre_hydration(&self, _ir: &Document) -> Result<(), String> {
        Ok(())
    }

    /// Query plugin capabilities
    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::default()
    }

    /// Get plugin metadata
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: self.name().to_string(),
            version: self.version().to_string(),
            description: "No description".to_string(),
            capabilities: self.capabilities(),
        }
    }

    /// Clone as Any for downcasting
    fn as_any(&self) -> &dyn Any;
}

/// Plugin capabilities bitflags
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PluginCapabilities {
    pub can_modify_ast: bool,
    pub can_modify_ir: bool,
    pub can_hook_ssr: bool,
    pub can_hook_hydration: bool,
}

impl PluginCapabilities {}
#[derive(Debug, Clone)]
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub capabilities: PluginCapabilities,
}

/// Plugin registry for managing loaded plugins
#[derive(Default)]
pub struct PluginRegistry {
    plugins: HashMap<String, Box<dyn AgsPlugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        PluginRegistry {
            plugins: HashMap::new(),
        }
    }

    /// Register a plugin
    pub fn register(&mut self, plugin: Box<dyn AgsPlugin>) -> Result<(), String> {
        let name = plugin.name().to_string();

        if self.plugins.contains_key(&name) {
            return Err(format!("Plugin '{}' already registered", name));
        }

        self.plugins.insert(name, plugin);
        Ok(())
    }

    /// Get a plugin by name
    pub fn get(&self, name: &str) -> Option<&dyn AgsPlugin> {
        self.plugins.get(name).map(|p| p.as_ref() as &dyn AgsPlugin)
    }

    /// List all registered plugins
    pub fn list(&self) -> Vec<PluginMetadata> {
        self.plugins.values().map(|p| p.metadata()).collect()
    }

    /// Call on_ast_parsed for all plugins
    pub fn on_ast_parsed(&self, ast: &mut crate::ast::Document) -> Result<(), String> {
        for plugin in self.plugins.values() {
            plugin.on_ast_parsed(ast)?;
        }
        Ok(())
    }

    /// Call on_ir_generated for all plugins
    pub fn on_ir_generated(&self, ir: &Document) -> Result<(), String> {
        for plugin in self.plugins.values() {
            plugin.on_ir_generated(ir)?;
        }
        Ok(())
    }

    /// Call on_pre_ssr for all plugins
    pub fn on_pre_ssr(&self, ir: &Document) -> Result<(), String> {
        for plugin in self.plugins.values() {
            plugin.on_pre_ssr(ir)?;
        }
        Ok(())
    }

    /// Call on_pre_hydration for all plugins
    pub fn on_pre_hydration(&self, ir: &Document) -> Result<(), String> {
        for plugin in self.plugins.values() {
            plugin.on_pre_hydration(ir)?;
        }
        Ok(())
    }

    /// Get count of plugins
    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }
}

/// Placeholder AST type (will reference agilang-ags-ast in real implementation)
pub mod ast {
    #[derive(Debug, Clone)]
    pub struct Document;
}

/// Example validation plugin
pub struct ValidationPlugin {
    strict_mode: bool,
}

impl ValidationPlugin {
    pub fn new(strict_mode: bool) -> Self {
        ValidationPlugin { strict_mode }
    }
}

impl AgsPlugin for ValidationPlugin {
    fn name(&self) -> &str {
        "validation"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn on_ast_parsed(&self, _ast: &mut ast::Document) -> Result<(), String> {
        if self.strict_mode {
            // Could validate AST structure here
        }
        Ok(())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities {
            can_modify_ast: true,
            can_modify_ir: false,
            can_hook_ssr: false,
            can_hook_hydration: false,
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_registry_new() {
        let registry = PluginRegistry::new();
        assert_eq!(registry.plugin_count(), 0);
    }

    #[test]
    fn test_register_plugin() {
        let mut registry = PluginRegistry::new();
        let plugin = Box::new(ValidationPlugin::new(false));

        let result = registry.register(plugin);
        assert!(result.is_ok());
        assert_eq!(registry.plugin_count(), 1);
    }

    #[test]
    fn test_duplicate_plugin_registration() {
        let mut registry = PluginRegistry::new();
        let plugin1 = Box::new(ValidationPlugin::new(false));
        let plugin2 = Box::new(ValidationPlugin::new(true));

        registry.register(plugin1).unwrap();
        let result = registry.register(plugin2);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already registered"));
    }

    #[test]
    fn test_get_plugin() {
        let mut registry = PluginRegistry::new();
        let plugin = Box::new(ValidationPlugin::new(false));

        registry.register(plugin).unwrap();
        let retrieved = registry.get("validation");

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name(), "validation");
    }

    #[test]
    fn test_list_plugins() {
        let mut registry = PluginRegistry::new();
        registry
            .register(Box::new(ValidationPlugin::new(false)))
            .unwrap();

        let list = registry.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "validation");
        assert_eq!(list[0].version, "1.0.0");
    }

    #[test]
    fn test_plugin_capabilities() {
        let plugin = ValidationPlugin::new(false);
        let caps = plugin.capabilities();

        assert!(caps.can_modify_ast);
        assert!(!caps.can_modify_ir);
        assert!(!caps.can_hook_ssr);
        assert!(!caps.can_hook_hydration);
    }

    #[test]
    fn test_plugin_metadata() {
        let plugin = ValidationPlugin::new(false);
        let metadata = plugin.metadata();

        assert_eq!(metadata.name, "validation");
        assert_eq!(metadata.version, "1.0.0");
        assert!(metadata.capabilities.can_modify_ast);
    }
}
