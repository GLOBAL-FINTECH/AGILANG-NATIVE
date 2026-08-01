# AGS Architecture Refined: Compiled Language with Typed View IR

## Executive Summary

AGS is **not HTML templating**. It is a **compiled language** that sits alongside AGI in a unified compiler, with its own:

- **Lexer** — Tokenizes template syntax with validation
- **Parser** — Parses AGS syntax into a typed abstract syntax tree
- **Semantic Analyzer** — Type checks against shared symbol table from AGI
- **Code Generator** — Produces both SSR code (Rust) and hydration graphs (JavaScript)

The unified compiler produces a **Typed View Intermediate Representation (IR)** that enables:

1. **Complete SSR** — Server renders full HTML with no JavaScript bootstrap required
2. **Zero-Cost Reactive Updates** — Browser follows dependency graph, no virtual DOM
3. **Compile-Time Type Safety** — `{{ user.email }}` fails at compile time if field doesn't exist
4. **Extensibility via Plugins** — New features added as compiler plugins, not hardcoded

---

## Part 1: The Unified Compiler Pipeline

### High-Level Flow

```
┌──────────────────────────────────────────────────────────────┐
│                    AGILANG Compiler v0.5.0                  │
└──────────────────────────────────────────────────────────────┘

         ┌─────────────────────────────────────┐
         │     Build System & Entry Point      │
         │  (orchestrates AGI and AGS paths)   │
         └─────────────────┬───────────────────┘
                           │
          ┌────────────────┴─────────────────┐
          │                                   │
          ▼                                   ▼
    ┌───────────────┐              ┌──────────────────┐
    │  AGI Path     │              │   AGS Path       │
    │               │              │                  │
    │ • Lexer       │              │ • Lexer          │
    │ • Parser      │              │ • Parser         │
    │ • Semantic    │              │ • Semantic       │
    │ • IR Gen      │              │ • View IR Gen    │
    │ • Codegen     │              │                  │
    └───────┬───────┘              └────────┬─────────┘
            │                               │
            └───────────────┬───────────────┘
                            │
                            ▼
        ┌─────────────────────────────────┐
        │  Shared Symbol Table            │
        │  • Type definitions             │
        │  • Function signatures          │
        │  • API response types           │
        │  • Component props & slots      │
        │  • Exported data structures     │
        └──────────┬──────────────────────┘
                   │
      ┌────────────┼────────────┐
      │            │            │
      ▼            ▼            ▼
┌──────────┐  ┌──────────┐  ┌──────────┐
│ Native   │  │   SSR    │  │Hydration │
│ Codegen  │  │ Renderer │  │ Graph    │
└────┬─────┘  └────┬─────┘  └────┬─────┘
     │             │              │
     ▼             ▼              ▼
┌─────────────────────────────────────┐
│  Output Generation                  │
│  • Rust executable                  │
│  • HTML response                    │
│  • JavaScript/WASM bundle           │
└─────────────────────────────────────┘
```

### Key Insight: Shared Symbol Table

The **shared symbol table** is the bridge between AGI and AGS:

```rust
// In AGI (main.agi):
pub struct ChainStatus {
    height: i64,
    validators: i32,
    status: String,
}

pub fn get_chain_status() -> ChainStatus { ... }
```

```agi
// In AGS (dashboard.ags):
@live chain from "/api/status" every 1000

<h1>Chain Height: {{ chain.height }}</h1>

// ✅ Compiler knows:
// 1. chain is of type ChainStatus (from AGI)
// 2. height exists and is i64
// 3. This binding will auto-update every 1000ms

// ❌ This would be a compile error:
<p>{{ chain.chain_height }}</p>  // Field doesn't exist
```

**Benefit**: Type errors caught at compile time, not runtime.

---

## Part 2: AGS Compiler Architecture

### Phase 3: AGS Compiler Implementation (Q3 2026)

```
dashboard.ags
     ↓
   Lexer (tokenize @page, @live, @component, etc.)
     ↓
  Parser (validate syntax, build AST)
     ↓
Semantic Analyzer (validate against shared symbol table)
     ↓
Typed View IR Generator
     ↓
  ┌────────────────────────────┐
  │  Typed View IR             │
  │  (platform-independent)    │
  └───────┬────────────┬───────┘
          │            │
          ▼            ▼
  ┌──────────────┐  ┌─────────────────────┐
  │ SSR Renderer │  │ Hydration Generator │
  │ (Rust)       │  │ (JavaScript)        │
  └──────┬───────┘  └──────────┬──────────┘
         │                      │
         ▼                      ▼
   HTML Response          Browser Runtime
   (complete HTML)       (minimal JS + WASM)
```

### Typed View IR Structure

The IR is a **data structure**, not code. It describes:

```rust
#[derive(Debug, Clone)]
pub struct ViewNode {
    /// HTML tag name or component identifier
    pub id: NodeId,
    pub tag: String,

    /// Static attributes
    pub attributes: HashMap<String, String>,

    /// Bound attributes ({{ expressions }})
    pub bindings: Vec<Binding>,

    /// Event listeners
    pub events: Vec<EventListener>,

    /// Data sources for this subtree
    pub data_sources: Vec<DataSource>,

    /// Child nodes
    pub children: Vec<ViewNode>,
}

#[derive(Debug, Clone)]
pub struct Binding {
    pub path: String,              // "chain.height"
    pub type_hint: Type,           // i64
    pub node_id: NodeId,           // Which DOM node to update
}

#[derive(Debug, Clone)]
pub struct DataSource {
    pub name: String,              // "chain"
    pub endpoint: String,          // "/api/status"
    pub interval_ms: u64,          // 1000
    pub retry_strategy: RetryStrategy,
    pub event_type: Type,          // struct ChainStatus
}
```

**Example IR for a template:**

```agi
@live chain from "/api/status" every 1000

<section>
    <h1>Height: {{ chain.height }}</h1>
    <p>Validators: {{ chain.validators }}</p>
</section>
```

Becomes IR:

```rust
ViewNode {
    id: NodeId(0),
    tag: "section",
    bindings: vec![],
    data_sources: vec![
        DataSource {
            name: "chain",
            endpoint: "/api/status",
            interval_ms: 1000,
            event_type: Type::Struct("ChainStatus"),
        }
    ],
    children: vec![
        ViewNode {
            id: NodeId(1),
            tag: "h1",
            bindings: vec![
                Binding {
                    path: "chain.height",
                    type_hint: Type::I64,
                    node_id: NodeId(1),
                }
            ],
            children: vec![]
        },
        ViewNode {
            id: NodeId(2),
            tag: "p",
            bindings: vec![
                Binding {
                    path: "chain.validators",
                    type_hint: Type::I32,
                    node_id: NodeId(2),
                }
            ],
            children: vec![]
        },
    ],
}
```

---

## Part 3: From IR to Runtime

### SSR Path: IR → Rust → HTML

The SSR renderer walks the IR and generates complete HTML server-side:

```rust
fn render_to_html(ir: &ViewNode, data: &HashMap<&str, Value>) -> String {
    let mut html = String::new();

    // Open tag
    html.push('<');
    html.push_str(&ir.tag);

    // Static attributes
    for (k, v) in &ir.attributes {
        html.push_str(&format!(" {}=\"{}\"", k, escape(v)));
    }
    html.push('>');

    // Render bindings (substitute values)
    for binding in &ir.bindings {
        let value = get_value(data, &binding.path)?;
        html.push_str(&escape(&value.to_string()));
    }

    // Render children
    for child in &ir.children {
        html.push_str(&render_to_html(child, data)?);
    }

    // Close tag
    html.push_str(&format!("</{}>", ir.tag));

    Ok(html)
}
```

**Result**: Browser receives complete HTML + data in page response.

**Advantage**: Fast initial load, perfect SEO, works without JavaScript.

### Hydration Path: IR → JavaScript Generator → Minimal JS

The hydration generator creates minimal JavaScript that:

1. Receives the IR and initial data
2. Sets up event listeners
3. Registers data source updates
4. Updates only affected nodes

```javascript
// Generated hydration code (minimal)

const viewIR = {
  id: 0,
  tag: "section",
  children: [
    { id: 1, tag: "h1", bindingPath: "chain.height" },
    { id: 2, tag: "p", bindingPath: "chain.validators" },
  ],
};

const nodeRegistry = new Map([
  [1, document.querySelector("h1")],
  [2, document.querySelector("p")],
]);

// Register data source
const chainSource = new EventSource("/api/status");
chainSource.addEventListener("update", (event) => {
  const data = JSON.parse(event.data);

  // Update ONLY affected nodes
  nodeRegistry.get(1).textContent = data.height;
  nodeRegistry.get(2).textContent = data.validators;

  // No DOM diffing
  // No virtual DOM
  // O(1) per update
});
```

**Result**: Browser has direct node references, updates follow dependency graph.

**Advantage**: No virtual DOM overhead, predictable performance.

---

## Part 4: Compiler Plugins

### Plugin System Overview

Instead of hardcoding every feature, the compiler uses a **plugin registry**:

```
┌─────────────────────────────────┐
│  AGILANG Compiler Core          │
│  • Lexer infrastructure         │
│  • Parser infrastructure        │
│  • Semantic infrastructure      │
│  • IR infrastructure            │
└──────────────┬──────────────────┘
               │
        ┌──────┴───────┬────────────────┬──────────────┐
        │              │                │              │
        ▼              ▼                ▼              ▼
    @live          @stream           @ai           @chart
    Plugin         Plugin            Plugin         Plugin

    Each provides:
    • Lexer tokens
    • Parser rules
    • Semantic validation
    • Code generation
    • IDE support
```

### Example: @live Plugin

```rust
// plugins/agilang-plugin-live/src/lib.rs

impl CompilerPlugin for LivePlugin {
    fn name(&self) -> &'static str { "live" }

    fn lexer_tokens(&self) -> Vec<TokenDefinition> {
        vec![
            TokenDefinition {
                name: "LIVE_KW",
                pattern: r"@live",
            },
            TokenDefinition {
                name: "FROM_KW",
                pattern: r"\bfrom\b",
            },
        ]
    }

    fn parser_rules(&self) -> Vec<ParserRule> {
        vec![
            ParserRule {
                name: "live_binding",
                pattern: "@live <ident> from <string> [every <number>]",
                produces: ASTNode::LiveBinding,
            },
        ]
    }

    fn semantic_validators(&self) -> Vec<SemanticValidator> {
        vec![
            SemanticValidator {
                validate: |binding, symbol_table| {
                    // Verify endpoint exists
                    // Get response type from API schema
                    // Validate interval is positive
                },
            },
        ]
    }

    fn codegen_strategies(&self) -> Vec<CodegenStrategy> {
        vec![
            CodegenStrategy {
                generate_ssr: |binding| {
                    // Generate server-side fetch
                },
                generate_hydration: |binding| {
                    // Generate browser-side event source
                },
            },
        ]
    }
}
```

### Reserved Plugin Namespaces

These are the first plugins (Phase 3-5):

| Namespace  | Purpose            | Example                                     |
| ---------- | ------------------ | ------------------------------------------- |
| `@live`    | HTTP bindings      | `@live chain from "/api/status" every 1000` |
| `@stream`  | WebSocket/SSE      | `@stream blocks websocket "/ws/blocks"`     |
| `@ai`      | AIFlow integration | `@ai generate_summary model="gpt-4"`        |
| `@agent`   | Autonomous agents  | `@agent validator`                          |
| `@chart`   | D3 visualizations  | `@chart line_chart`                         |
| `@map`     | Geographic data    | `@map region_map`                           |
| `@wallet`  | Web3 operations    | `@wallet connect metamask`                  |
| `@payment` | Payment processors | `@payment stripe_checkout`                  |
| `@cache`   | Caching            | `@cache redis ttl 3600`                     |
| `@form`    | Form validation    | `@form validation email`                    |

---

## Part 5: Putting It All Together

### Complete Flow: From Source to Execution

```
User creates dashboard.ags:
    @live chain from "/api/status" every 1000
    <h1>{{ chain.height }}</h1>

    │
    ├─ Compiler reads AGI files → builds shared symbol table
    │  └─ Knows: chain endpoint returns ChainStatus { height: i64, ... }
    │
    ├─ Compiler reads AGS file → validates against symbol table
    │  └─ OK: chain.height exists and is i64
    │
    ├─ Generates Typed View IR
    │  └─ ViewNode { bindings: [Binding { path: "chain.height", ... }] }
    │
    ├─ SSR Renderer path
    │  └─ Calls /api/status → Gets { height: 1000, ... }
    │  └─ Renders: <h1>1000</h1>
    │  └─ Response: <html>...</html>
    │
    ├─ Hydration Generator path
    │  └─ Generates JavaScript
    │  └─ Fetches /api/status every 1000ms
    │  └─ Updates h1 node with new height
    │
Browser receives:
    1. Complete <html> page (no blank loading screen)
    2. JavaScript that registers updates
    3. Minimal code (no framework overhead)

User refreshes endpoint:
    Browser runs generated JavaScript
    └─ Fetches /api/status → Gets { height: 1001, ... }
    └─ document.querySelector('h1').textContent = '1001'
    └─ No diffing, no reconciliation, O(1) update
```

---

## Part 6: Why This Architecture Is Stronger

### 1. Type Safety Across Boundary

```agi
// AGI defines the type
pub struct DashboardData {
    height: i64,
}

// AGS uses it
{{ dashboard.height }}      // ✅ OK
{{ dashboard.heigth }}      // ❌ Compile error (typo)
```

Traditional templating:

- `{{ dashboard.heigth }}` produces **runtime error** (undefined is not a number)

AGILANG templating:

- `{{ dashboard.heigth }}` produces **compile error** (field doesn't exist)

### 2. No Virtual DOM Overhead

React/Vue/Svelte:

```
Data change
  → Full re-render
  → Virtual DOM diffing
  → Patch algorithm
  → DOM update
```

AGILANG:

```
Data change
  → Dependency lookup: "chain.height" → node 42
  → Direct update: node.textContent = value
  → O(1) operation
```

### 3. Server-Side Rendering Without Framework

Next.js/Nuxt pattern:

```
Request
  → JavaScript server-side rendering
  → Send HTML + JavaScript bundle
  → Browser hydrates
```

AGILANG pattern:

```
Request
  → Native SSR (Rust)
  → Send HTML + minimal JavaScript
  → Browser runs hydration graph
  → No framework overhead
```

### 4. Extensibility Without Core Changes

Adding new feature (e.g., `@chart`):

❌ Traditional approach:

- Modify lexer
- Modify parser
- Modify semantic analyzer
- Modify code generator
- Update tests

✅ AGILANG approach:

- Create plugin crate
- Implement CompilerPlugin trait
- Register in plugin registry
- Done

---

## Part 7: Development Timeline

### Phase 3: AGS Compiler (Q3 2026)

**Deliverables**:

1. ✅ AGS Lexer (tokenizes @page, @live, @component, etc.)
2. ✅ AGS Parser (builds AST)
3. ✅ Semantic Analyzer (validates against shared symbol table)
4. ✅ Typed View IR Generator
5. ✅ SSR Renderer (Rust)
6. ✅ Hydration Generator (JavaScript)
7. ✅ @live plugin (HTTP bindings)
8. ✅ @stream plugin (WebSocket/SSE)
9. ✅ IDE support (IntelliSense, go-to-definition, refactoring)

### Phase 4: Plugins & AIFlow (Q4 2026)

**Deliverables**:

1. Plugin system registration and loading
2. @ai plugin (AIFlow integration)
3. @agent plugin (autonomous agents)
4. Feature-gated plugin compilation

### Phase 5: Extended Plugins (Q1 2027)

**Deliverables**:

1. @chart plugin (D3 visualizations)
2. @map plugin (Mapbox integration)
3. @wallet plugin (Web3 operations)
4. @payment plugin (Stripe/PayPal)

---

## Conclusion

AGS is a **compiled language** that leverages a **shared symbol table** with AGI to provide:

✅ **Type safety** — Errors caught at compile time  
✅ **Performance** — No virtual DOM, dependency graph updates  
✅ **SSR** — Complete HTML from native Rust  
✅ **Extensibility** — Plugins for new features  
✅ **Native** — All execution is Rust/C, no Python

This architecture positions AGILANG as a **single-language full-stack compiler** for building native web applications.

---

**Document Version**: 2.0 (Refined Architecture)  
**Date**: 2026-07-21  
**Status**: Phase 3 design complete, implementation pending
