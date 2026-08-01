# AGILANG Compiler Plugin System

## Overview

The AGILANG compiler uses a **plugin system** to extend language capabilities without creating a monolith. Each plugin contributes lexer tokens, parser rules, semantic validation, code generation, and IDE support.

**Philosophy**: Core language stays minimal. Domain-specific features (AI, blockchain, visualizations, etc.) are added as first-class compiler plugins.

---

## Plugin Architecture

```
                  AGILANG Compiler Core
                   (7 core crates)
                         │
        ┌────────────────┼────────────────┐
        │                │                │
    Lexer          Parser + AST      Semantic
        │                │                │
        └────────────────┼────────────────┘
                         │
        ┌────────────────┼────────────────┐
        │                │                │
   Plugin         Plugin           Plugin
   Loader         Registry          Manager
        │                │                │
        └────────────────┼────────────────┘
                         │
        ┌────────────────┼────────────────┬─────────────┐
        │                │                │             │
    @ai            @chart            @wallet        @stream
    Plugin          Plugin            Plugin         Plugin
```

---

## Plugin Namespaces

Reserved namespaces for compiler plugins:

| Namespace  | Purpose               | Example Directives                      |
| ---------- | --------------------- | --------------------------------------- |
| `@live`    | Data binding          | `@live chain from "/api/status"`        |
| `@stream`  | Data streams          | `@stream blocks websocket "/ws/blocks"` |
| `@ai`      | AI operations         | `@ai generate_summary`                  |
| `@agent`   | Autonomous agents     | `@agent validator`                      |
| `@chart`   | Visualizations        | `@chart line_chart`                     |
| `@map`     | Geographic data       | `@map region_map`                       |
| `@wallet`  | Blockchain operations | `@wallet connect metamask`              |
| `@payment` | Payment processors    | `@payment stripe_checkout`              |
| `@form`    | Enhanced forms        | `@form validation email`                |
| `@cache`   | Caching strategies    | `@cache redis ttl 3600`                 |

Each reserved namespace is implemented as a separate compiler plugin.

---

## Plugin Interface

Every plugin implements this interface:

```rust
pub trait CompilerPlugin {
    /// Plugin metadata
    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn description(&self) -> &'static str;

    /// Lexer contributions
    fn lexer_tokens(&self) -> Vec<TokenDefinition>;

    /// Parser contributions
    fn parser_rules(&self) -> Vec<ParserRule>;
    fn ast_nodes(&self) -> Vec<ASTNodeDefinition>;

    /// Semantic analysis
    fn semantic_validators(&self) -> Vec<SemanticValidator>;
    fn type_constructors(&self) -> Vec<TypeConstructor>;

    /// Code generation
    fn codegen_strategies(&self) -> Vec<CodegenStrategy>;

    /// IDE support
    fn language_server_support(&self) -> LSPCapabilities;

    /// Validate dependencies (e.g., @ai needs AIFlow runtime)
    fn validate_dependencies(&self, ctx: &ValidationContext) -> Result<()>;
}
```

---

## Example: @live Plugin

### Token Definition

```rust
pub fn lexer_tokens() -> Vec<TokenDefinition> {
    vec![
        TokenDefinition {
            name: "LIVE_KW",
            pattern: r"@live",
            context: "ags",
        },
        TokenDefinition {
            name: "FROM_KW",
            pattern: r"\bfrom\b",
            context: "live_binding",
        },
        TokenDefinition {
            name: "EVERY_KW",
            pattern: r"\bevery\b",
            context: "live_binding",
        },
    ]
}
```

### Parser Rules

```rust
pub fn parser_rules() -> Vec<ParserRule> {
    vec![
        ParserRule {
            name: "live_binding",
            pattern: "@live <ident> from <string> [every <number> [retry <strategy>]]",
            produces: ASTNode::LiveBinding,
        },
    ]
}
```

### AST Node

```rust
#[derive(Debug, Clone)]
pub struct LiveBinding {
    pub name: String,
    pub endpoint: String,
    pub interval_ms: u64,
    pub retry_strategy: Option<RetryStrategy>,
    pub timeout_ms: Option<u64>,
    pub cache_duration_ms: Option<u64>,
    pub type_from_agi: Option<Type>,
}
```

### Semantic Validation

```rust
pub fn validate_live_binding(
    binding: &LiveBinding,
    symbol_table: &SymbolTable,
) -> Result<ValidatedLiveBinding> {
    // 1. Verify name is not already bound
    if symbol_table.contains(&binding.name) {
        return Err("name already bound".into());
    }

    // 2. Fetch type from AGI's shared symbol table
    let response_type = symbol_table
        .lookup_api_response(&binding.endpoint)
        .ok_or("endpoint not found in AGI")?;

    // 3. Create binding with known type
    Ok(ValidatedLiveBinding {
        name: binding.name.clone(),
        endpoint: binding.endpoint.clone(),
        expected_type: response_type,
    })
}
```

### Code Generation

**For SSR** (Rust):

```rust
fn codegen_ssr_live_binding(binding: &ValidatedLiveBinding) -> String {
    format!(
        r#"
let {} = fetch_json("{}").await?;
render_with_binding(&mut html, "{}", {})
        "#,
        binding.name,
        binding.endpoint,
        binding.name,
        binding.name,
    )
}
```

**For Browser** (JavaScript):

```javascript
function hydrateBinding(name, endpoint, intervalMs) {
  let node = document.querySelector(`[data-bind="${name}"]`);

  function update() {
    fetch(endpoint)
      .then((r) => r.json())
      .then((data) => {
        node.textContent = data;
      });
  }

  update();
  setInterval(update, intervalMs);
}
```

### IDE Support

```rust
pub fn language_server_support() -> LSPCapabilities {
    LSPCapabilities {
        completion: vec![
            CompletionItem {
                label: "@live",
                detail: "Bind live data source",
                snippet: "@live ${1:name} from \"${2:/api/endpoint}\" every ${3:1000}",
            },
        ],
        hover: Some(|endpoint| {
            format!("Fetches from {} every interval", endpoint)
        }),
        references: Some(enable_reference_tracking),
        rename: Some(enable_safe_rename),
    }
}
```

---

## Example: @ai Plugin

### Token Definition

```rust
pub fn lexer_tokens() -> Vec<TokenDefinition> {
    vec![
        TokenDefinition { name: "AI_KW", pattern: r"@ai" },
        TokenDefinition { name: "GENERATE_KW", pattern: r"\bgenerate\b" },
        TokenDefinition { name: "EMBED_KW", pattern: r"\bembed\b" },
        TokenDefinition { name: "CLASSIFY_KW", pattern: r"\bclassify\b" },
    ]
}
```

### AST Nodes

```rust
#[derive(Debug, Clone)]
pub enum AIOperation {
    Generate {
        model: String,
        prompt: Expr,
        max_tokens: u32,
    },
    Embed {
        text: Expr,
        model: String,
    },
    Classify {
        text: Expr,
        labels: Vec<String>,
    },
}
```

### Semantic Validation

```rust
pub fn validate_ai_operation(
    op: &AIOperation,
    symbol_table: &SymbolTable,
) -> Result<()> {
    // 1. Verify AIFlow runtime is available
    if !symbol_table.has_runtime("agilang-aiflow") {
        return Err("AIFlow runtime not linked".into());
    }

    // 2. Validate model exists in AIFlow registry
    let model = match op {
        AIOperation::Generate { model, .. } => model,
        AIOperation::Embed { model, .. } => model,
    };

    if !aiflow_registry::has_model(model) {
        return Err(format!("Model not found: {}", model));
    }

    Ok(())
}
```

### Code Generation (Rust)

```rust
fn codegen_ai_generate(
    model: &str,
    prompt: &Expr,
    max_tokens: u32,
) -> String {
    format!(
        r#"
let prompt = {};
let result = aiflow::generate("{}", prompt, {})?;
        "#,
        prompt, model, max_tokens,
    )
}
```

---

## Example: @stream Plugin

### Syntax

```agi
@stream validator_updates
    websocket "/ws/validators"
    parser json
    retry exponential backoff 1000 max 10000

<section>
    @for update in validator_updates:
        <div>{{ update.address }}: {{ update.stake }}</div>
</section>
```

### Plugin Implementation

```rust
#[derive(Debug, Clone)]
pub struct StreamBinding {
    pub name: String,
    pub protocol: StreamProtocol, // websocket, sse, grpc, etc.
    pub endpoint: String,
    pub parser: ParserType,        // json, protobuf, msgpack, etc.
    pub retry_strategy: RetryStrategy,
}

pub fn validate_stream(
    binding: &StreamBinding,
    symbol_table: &SymbolTable,
) -> Result<ValidatedStreamBinding> {
    // Verify endpoint and parser compatibility
    // Get type from the stream's event schema
    // Create validated binding with event type
}
```

---

## Plugin Discovery & Loading

### Plugin Registry

```rust
// Built at compile time
pub struct PluginRegistry {
    plugins: HashMap<String, Box<dyn CompilerPlugin>>,
}

impl PluginRegistry {
    pub fn load() -> Result<Self> {
        let mut plugins = HashMap::new();

        // Core plugins (always loaded)
        plugins.insert("live".into(), Box::new(LivePlugin::new()));
        plugins.insert("stream".into(), Box::new(StreamPlugin::new()));
        plugins.insert("ai".into(), Box::new(AIPlugin::new()));

        // Optional plugins (feature-gated)
        #[cfg(feature = "wallet")]
        plugins.insert("wallet".into(), Box::new(WalletPlugin::new()));

        #[cfg(feature = "chart")]
        plugins.insert("chart".into(), Box::new(ChartPlugin::new()));

        Ok(PluginRegistry { plugins })
    }

    pub fn get(&self, name: &str) -> Option<&dyn CompilerPlugin> {
        self.plugins.get(name).map(|b| &**b)
    }
}
```

### Compilation with Plugins

```rust
pub fn compile_ags_with_plugins(
    source: &str,
    registry: &PluginRegistry,
) -> Result<CompiledAGS> {
    // 1. Combine all plugin lexer tokens
    let mut lexer = Lexer::new(source);
    for (name, plugin) in &registry.plugins {
        for token_def in plugin.lexer_tokens() {
            lexer.register(token_def);
        }
    }

    // 2. Tokenize
    let tokens = lexer.tokenize()?;

    // 3. Parse with all plugin parser rules
    let mut parser = Parser::new(tokens);
    for (name, plugin) in &registry.plugins {
        for rule in plugin.parser_rules() {
            parser.register(rule);
        }
    }
    let ast = parser.parse()?;

    // 4. Semantic analysis with all validators
    let mut symbol_table = SymbolTable::new();
    for (name, plugin) in &registry.plugins {
        plugin.validate_dependencies(&symbol_table)?;
        for validator in plugin.semantic_validators() {
            validator.validate(&ast, &mut symbol_table)?;
        }
    }

    // 5. Code generation with all strategies
    let mut codegen = Codegen::new();
    for (name, plugin) in &registry.plugins {
        for strategy in plugin.codegen_strategies() {
            codegen.register(strategy);
        }
    }
    let ssr_code = codegen.generate_ssr(&ast)?;
    let hydration_graph = codegen.generate_hydration(&ast)?;

    Ok(CompiledAGS { ssr_code, hydration_graph })
}
```

---

## Feature Flags

Plugins are feature-gated in Cargo.toml:

```toml
[features]
default = ["live", "stream"]
live = []
stream = []
ai = ["agilang-aiflow"]
chart = []
map = []
wallet = ["web3"]
payment = ["stripe"]
```

Build with plugins:

```bash
cargo build --features "live,stream,ai,chart"
```

---

## Type Sharing Between Plugins

When AGI exports a type and AGS needs to reference it:

```rust
// AGI defines a type
pub struct DashboardModel {
    pub chain_height: i64,
    pub validators: Vec<Validator>,
    pub status: ChainStatus,
}
```

The **shared symbol table** makes this available to AGS plugins:

```rust
// @live plugin queries the symbol table
let model_type = symbol_table.lookup_type("DashboardModel")?;

// Type is now known at compile time
// {{ dashboard.chain_heigth }} would be a compile error (typo)
```

---

## IDE Integration

Each plugin contributes to IDE support:

### IntelliSense

```rust
@live chain from "/api/status"

// IDE shows completion:
// - from (with parameter suggestions)
// - every (with interval examples)
// - retry (with strategy options)
// - timeout
// - cache
```

### Hover Information

```
@live chain from "/api/status" every 1000
               ↑
            Hovering shows:
            "Fetches from /api/status every 1000ms
             Returns: ChainStatus (from shared AGI symbol table)
             Type-safe for: chain.height (i64)"
```

### Go to Definition

```
{{ chain.height }}
      ↓
      Jumps to AGI definition of DashboardModel.chain_height
```

### Refactoring

Renaming `chain` to `blockchain` automatically updates:

- The `@live` binding
- All `{{ chain.* }}` references in the template
- AGI import statements

---

## Standard Plugin Template

```rust
// plugins/agilang-plugin-example/src/lib.rs

use agilang_compiler_plugin::*;

pub struct ExamplePlugin;

impl CompilerPlugin for ExamplePlugin {
    fn name(&self) -> &'static str { "example" }
    fn version(&self) -> &'static str { "0.5.0" }
    fn description(&self) -> &'static str { "Example plugin for AGILANG" }

    fn lexer_tokens(&self) -> Vec<TokenDefinition> {
        vec![
            TokenDefinition {
                name: "EXAMPLE_KW",
                pattern: r"@example",
                context: "ags",
            },
        ]
    }

    fn parser_rules(&self) -> Vec<ParserRule> {
        todo!()
    }

    fn ast_nodes(&self) -> Vec<ASTNodeDefinition> {
        todo!()
    }

    fn semantic_validators(&self) -> Vec<SemanticValidator> {
        todo!()
    }

    fn type_constructors(&self) -> Vec<TypeConstructor> {
        vec![]
    }

    fn codegen_strategies(&self) -> Vec<CodegenStrategy> {
        todo!()
    }

    fn language_server_support(&self) -> LSPCapabilities {
        LSPCapabilities::default()
    }

    fn validate_dependencies(&self, ctx: &ValidationContext) -> Result<()> {
        Ok(())
    }
}
```

---

## Benefits

✅ **Modularity** — Each feature is a separate plugin  
✅ **Extensibility** — Add new features without modifying core compiler  
✅ **Feature Flags** — Include only needed plugins (smaller binary)  
✅ **IDE Integration** — Each plugin contributes to language server  
✅ **Compile-Time** — All plugins are resolved at compilation, zero runtime overhead  
✅ **Type Safety** — Plugins use shared symbol table for cross-domain type checking  
✅ **Community** — Third-party plugins can extend AGILANG

---

## Planned Plugins

| Plugin     | Status     | Purpose                      |
| ---------- | ---------- | ---------------------------- |
| `@live`    | ✅ Phase 3 | HTTP/API data binding        |
| `@stream`  | ✅ Phase 3 | WebSocket/SSE streams        |
| `@ai`      | ✅ Phase 4 | AIFlow integration           |
| `@agent`   | ⏳ Phase 4 | Autonomous agent definitions |
| `@chart`   | ⏳ Phase 5 | D3/Recharts visualizations   |
| `@map`     | ⏳ Phase 5 | Mapbox/Leaflet integration   |
| `@wallet`  | ⏳ Phase 5 | Web3/blockchain wallets      |
| `@payment` | ⏳ Phase 5 | Stripe/PayPal processors     |
| `@form`    | ⏳ Phase 5 | Enhanced form validation     |
| `@cache`   | ⏳ Phase 5 | Redis/Memcached integration  |

---

## Next Steps

1. **Phase 3**: Implement `@live` and `@stream` plugins with full IDE support
2. **Phase 4**: Add `@ai` and `@agent` plugins for AIFlow integration
3. **Phase 5**: Extend with visualization and blockchain plugins
4. **Community**: Publish plugin SDK and templates for third-party extensions

---

**Plugin Architecture Version**: 1.0  
**Status**: Design complete, Phase 3 implementation pending
