# Phase 3A: AGS Compiler Foundation — Implementation Roadmap

**Version**: 0.6.0  
**Status**: Under Development  
**Start Date**: 2026-07-21  
**Target Completion**: 2026-Q3

---

## Objective

Implement the AGS compiler as a compiled language (not templating), producing a **Typed View IR** that enables:

1. Server-side rendering (SSR) in native Rust
2. Hydration graphs for browser reactivity
3. Type-safe bindings across AGI↔AGS boundary
4. Compile-time error detection for invalid field references

---

## Success Criteria (The Smart Chain Fixture)

A single acceptance test proves the entire Phase 3A implementation:

```agi
@page title="Smart Chain Dashboard"
@live chain from "/api/status" every 1000

<main>
    <h1>Chain Height: {{ chain.height }}</h1>
    <p>Validators: {{ chain.validators }}</p>
</main>
```

This fixture must demonstrate:

✅ **Parse @page metadata** — Title extracted and available for SEO  
✅ **Parse @live binding** — Endpoint, interval, name resolved  
✅ **Resolve chain type** — Type `ChainStatus { height: i64, validators: i32 }` from AGI  
✅ **Type-safe bindings** — `{{ chain.height }}` OK, `{{ chain.heigth }}` compile error  
✅ **Generate SSR HTML** — Server renders complete `<main>` with initial data  
✅ **Generate hydration graph** — Browser code knows which nodes to update  
✅ **Poll API every 1000ms** — Fetch `/api/status` on interval  
✅ **Update affected bindings** — Change to `chain.height` updates only `<h1>` node  
✅ **Preserve page state** — No full refresh, no virtual DOM diffing  
✅ **Escape HTML values** — `<script>` in chain data becomes `&lt;script&gt;`  
✅ **Detect unknown fields** — `{{ chain.unknown_field }}` fails at compile time

---

## Implementation Tasks (In Order)

### Phase 3A-1: Core AGS Lexer & Parser (Week 1-2)

**Crate**: `agilang-ags-lexer`  
**Deliverable**: Tokenize AGS syntax without depending on HTML parser

```rust
// Input: @page title="Dashboard" @live chain from "/api/status"
// Output: Token stream
[
    Token::PageDirective,
    Token::Identifier("title"),
    Token::Assign,
    Token::String("Dashboard"),
    Token::LiveDirective,
    Token::Identifier("chain"),
    Token::From,
    Token::String("/api/status"),
]
```

**Tokens to implement**:

- `@page` directive
- `@live` directive
- `@component`, `@slot` (future)
- `@for`, `@if` (future)
- Template expressions: `{{` and `}}`
- Standard HTML tokens
- Comments

**Files**:

- `crates/agilang-ags-lexer/src/lib.rs` — Lexer implementation
- `crates/agilang-ags-lexer/src/token.rs` — Token definitions
- `crates/agilang-ags-lexer/tests/tokenize_test.rs` — Token tests

**Test**: Tokenize Smart Chain fixture → verify 20+ tokens

---

### Phase 3A-2: AGS AST Definition (Week 1-2)

**Crate**: `agilang-ags-ast`  
**Deliverable**: Define abstract syntax tree for AGS

```rust
pub struct Document {
    pub directives: Vec<Directive>,
    pub root: ViewNode,
}

pub enum Directive {
    Page { title: String, description: Option<String> },
    Live { name: String, endpoint: String, interval_ms: u64 },
}

pub struct ViewNode {
    pub tag: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<ViewNode>,
    pub bindings: Vec<Binding>,
}

pub struct Binding {
    pub expression: String,  // "chain.height"
    pub line: usize,
    pub column: usize,
}
```

**Files**:

- `crates/agilang-ags-ast/src/lib.rs` — AST node definitions
- `crates/agilang-ags-ast/src/directives.rs` — Directive types

**Test**: Create AST nodes for Smart Chain fixture → verify structure

---

### Phase 3A-3: AGS Parser (Week 2-3)

**Crate**: `agilang-ags-parser`  
**Deliverable**: Parse token stream into AST

```rust
pub fn parse_document(tokens: Vec<Token>) -> Result<Document> {
    // 1. Parse directives (@page, @live)
    // 2. Parse template (HTML)
    // 3. Bind expressions ({{ chain.height }})
    // 4. Return AST
}
```

**Parser tasks**:

1. Parse `@page` directive → Extract title
2. Parse `@live` directive → Extract name, endpoint, interval
3. Parse HTML elements → Build tree structure
4. Parse `{{ ... }}` expressions → Create bindings
5. Error recovery → Report line:column for syntax errors

**Files**:

- `crates/agilang-ags-parser/src/lib.rs` — Parser implementation
- `crates/agilang-ags-parser/src/directives.rs` — Directive parsing
- `crates/agilang-ags-parser/src/template.rs` — Template parsing
- `crates/agilang-ags-parser/tests/parse_test.rs` — Parser tests

**Test**: Parse Smart Chain fixture → verify `@page`, `@live`, 3 nodes, 2 bindings

---

### Phase 3A-4: Typed View IR (Week 3)

**Crate**: `agilang-view-ir`  
**Deliverable**: Intermediate representation for rendering

```rust
pub struct ViewNode {
    pub id: NodeId,
    pub tag: String,
    pub attributes: HashMap<String, String>,
    pub bindings: Vec<Binding>,
    pub data_sources: Vec<DataSource>,
    pub children: Vec<ViewNode>,
}

pub struct Binding {
    pub path: String,      // "chain.height"
    pub node_id: NodeId,   // Which node to update
    pub type_hint: Type,   // i64 (from AGI)
}

pub struct DataSource {
    pub name: String,              // "chain"
    pub endpoint: String,          // "/api/status"
    pub interval_ms: u64,          // 1000
    pub response_type: Type,       // struct ChainStatus
}
```

**Files**:

- `crates/agilang-view-ir/src/lib.rs` — IR definitions

**Test**: AST → IR conversion produces correct structure

---

### Phase 3A-5: AGS Semantic Analysis (Week 3-4)

**Crate**: `agilang-ags-semantic`  
**Deliverable**: Type checking against AGI's symbol table

```rust
pub fn analyze(ast: Document, agi_symbol_table: &SymbolTable)
    -> Result<SemanticDocument>
{
    for binding in &ast.bindings {
        // 1. Parse "chain.height" into path
        // 2. Look up "chain" in data sources
        // 3. Resolve "ChainStatus" type
        // 4. Check field "height" exists
        // 5. Verify it's assignable to binding location
    }
}
```

**Semantic checks**:

1. **Data source validation** — `@live chain from "/api/status"` endpoint exists in AGI
2. **Field existence** — `{{ chain.height }}` field exists in resolved type
3. **Type compatibility** — Binding target accepts the field type
4. **Name uniqueness** — `@live chain` name doesn't conflict

**Error messages** (with line:column):

```
error: Unknown field 'heigth' in type 'ChainStatus'
  --> dashboard.ags:5:20
   |
5  |     <p>{{ chain.heigth }}</p>
   |                    ^^^^^^ Did you mean 'height'?
```

**Files**:

- `crates/agilang-ags-semantic/src/lib.rs` — Semantic analyzer
- `crates/agilang-ags-semantic/src/validator.rs` — Validation rules
- `crates/agilang-ags-semantic/tests/type_checking_test.rs` — Type tests

**Test**:

- ✅ `{{ chain.height }}` passes analysis
- ❌ `{{ chain.heigth }}` produces compile error
- ❌ Missing data source reference fails
- ❌ Type mismatch reported

---

### Phase 3A-6: AGS→SSR Code Generation (Week 4)

**Crate**: `agilang-ags-ssr`  
**Deliverable**: Generate Rust code for server-side rendering

```rust
pub fn codegen_ssr(ir: &ViewNode, values: &HashMap<String, Value>) -> String {
    // Generate Rust code that:
    // 1. Takes input data (chain status)
    // 2. Renders complete HTML
    // 3. Escapes values for XSS safety
    // 4. Produces string output
}
```

**Generated output** (example for Smart Chain):

```html
<main>
  <h1>Chain Height: 1000</h1>
  <p>Validators: 100</p>
</main>
```

**Guarantees**:

- HTML escaping for all `{{ ... }}` expressions
- XSS protection (e.g., `<script>` becomes `&lt;script&gt;`)
- Valid HTML output
- SEO metadata in `<head>`

**Files**:

- `crates/agilang-ags-ssr/src/lib.rs` — SSR code generator
- `crates/agilang-ags-ssr/src/escape.rs` — HTML escaping
- `crates/agilang-ags-ssr/tests/html_test.rs` — HTML output tests

**Test**:

- ✅ Generate valid HTML for Smart Chain fixture
- ✅ Escape `<` to `&lt;` in data
- ✅ Preserve attributes
- ✅ Format readable output

---

### Phase 3A-7: Hydration Graph Generation (Week 4)

**Crate**: `agilang-ags-hydration`  
**Deliverable**: Generate JavaScript for browser reactivity

```rust
pub fn codegen_hydration(ir: &ViewNode) -> String {
    // Generate minimal JavaScript that:
    // 1. Knows which nodes to update
    // 2. Registers data source listeners
    // 3. Updates DOM nodes on data changes
    // 4. No virtual DOM, no diffing
}
```

**Generated output** (example for Smart Chain):

```javascript
const nodeRegistry = new Map([
  [1, document.querySelector("h1")],
  [2, document.querySelector("p")],
]);

const chainSource = new EventSource("/api/status");
chainSource.addEventListener("update", (event) => {
  const data = JSON.parse(event.data);
  nodeRegistry.get(1).textContent = "Chain Height: " + data.height;
  nodeRegistry.get(2).textContent = "Validators: " + data.validators;
});
```

**Guarantees**:

- No virtual DOM
- No reconciliation algorithm
- Direct node references
- O(1) per update

**Files**:

- `crates/agilang-ags-hydration/src/lib.rs` — Hydration code generator
- `crates/agilang-ags-hydration/tests/hydration_test.rs` — Hydration tests

**Test**:

- ✅ Generate JavaScript without syntax errors
- ✅ Node registry contains correct references
- ✅ Event listener registered for data source

---

### Phase 3A-8: AGI↔AGS Type Bridge (Week 5)

**Task**: Update `agilang-semantic` to coordinate with AGS analyzer

```rust
// In agilang-semantic (AGI compiler):
pub fn export_symbol_table() -> SymbolTable {
    // Exports all public types, functions, endpoints
}

// In agilang-ags-semantic (AGS compiler):
pub fn analyze_with_agi(ast: Document, agi_symbols: SymbolTable)
    -> Result<SemanticDocument>
{
    // Uses AGI symbols to validate bindings
}
```

**Integration**:

1. Build AGI code → export symbol table
2. Build AGS code → import symbol table
3. Validate AGS bindings against AGI types
4. Report mismatches as compile errors

**Test**:

- ✅ AGI `struct ChainStatus` available to AGS
- ✅ AGS `{{ chain.height }}` validates against AGI type
- ❌ AGS `{{ chain.unknown }}` fails with clear error
- ❌ Type mismatch in binding reported

---

### Phase 3A-9: Compiler Plugin Foundation (Week 5)

**Crate**: `agilang-compiler-plugins`  
**Deliverable**: Plugin registry and loader

```rust
pub trait CompilerPlugin {
    fn name(&self) -> &'static str;
    fn lexer_tokens(&self) -> Vec<TokenDefinition>;
    fn parser_rules(&self) -> Vec<ParserRule>;
    fn semantic_validators(&self) -> Vec<SemanticValidator>;
    fn codegen_strategies(&self) -> Vec<CodegenStrategy>;
}

pub struct PluginRegistry {
    plugins: HashMap<String, Box<dyn CompilerPlugin>>,
}
```

**Initial plugins**:

- `@page` — Built-in, no plugin
- `@live` — First plugin implementation
- `@stream` — Second plugin (future)

**Files**:

- `crates/agilang-compiler-plugins/src/lib.rs` — Plugin trait
- `crates/agilang-compiler-plugins/src/registry.rs` — Plugin registry
- `crates/agilang-ags-plugin-live/src/lib.rs` — @live plugin

**Test**: Plugin registry loads and provides tokens/rules

---

### Phase 3A-10: Integration & Testing (Week 5-6)

**Crate**: `agilang-compiler` (update)  
**Deliverable**: Full AGS compilation pipeline

```rust
pub fn compile_ags(source: &str, agi_symbols: SymbolTable) -> Result<CompiledAGS> {
    // 1. Lexer: source → tokens
    // 2. Parser: tokens → AST
    // 3. Semantic: AST → verified document
    // 4. View IR: document → view nodes
    // 5. SSR: view nodes → Rust code
    // 6. Hydration: view nodes → JavaScript
}
```

**CLI command**:

```bash
agilang-native build dashboard.ags \
    --agi-symbols dashboard.agi \
    --emit-ssr \
    --emit-hydration
```

**Output files**:

- `dashboard_ssr.rs` — Server-side renderer
- `dashboard_hydrate.js` — Browser hydration
- `dashboard.html` — Example rendered output

**Tests**:

1. **Lexer tests** — Token stream correctness
2. **Parser tests** — AST structure
3. **Semantic tests** — Type validation, error detection
4. **View IR tests** — Node structure
5. **SSR tests** — HTML output, escaping, XSS
6. **Hydration tests** — JavaScript syntax, node references
7. **Integration tests** — End-to-end: source → executable → rendered page

---

## SIBAQ Smart Chain Observability Dashboard (Live Conformance Test)

**Network**: SIBAQ Smart Chain  
**RPC Endpoint**: `https://rpc.sibaq.us`  
**Chain ID**: `1990` (hex: `0x7c6`)  
**Symbol**: `SBQ`  
**Polling**: 1000ms (one-second updates)  
**Target**: Replace placeholder-based observability with native AGS server-side rendering

**Current Issue**: The live observability at `https://rpc.sibaq.us/` displays placeholder values (`--`) for chain height, block hash, validators, and transaction count. These are populated only after the browser downloads and runs JavaScript. Native AGS should render complete, accurate values server-side.

### AGS Template (examples/ags/sibaq-dashboard.ags)

```agi
@page
    title="Smart Chain Dashboard"
    seo_description="Live Smart Chain execution and validator status."

@live chain
    from "/api/status"
    every 1000
    initial server
    retry exponential
    timeout 5000

<main class="shell explorer-shell">
    <header class="explorer-header">
        <div>
            <p class="eyebrow">Canonical Intelligence Layer</p>
            <h1>{{ app_name }}</h1>
            <p>
                Every canonical transaction, receipt, state transition,
                and proof root—live from the chain database.
            </p>
        </div>

        <div class="network-pill">
            <i></i>
            <span>{{ chain.status }}</span>
            <small>Chain {{ chain.chain_id }}</small>
        </div>
    </header>

    <section class="stats">
        <article>
            <span>Block height</span>
            <strong>{{ chain.height }}</strong>
        </article>

        <article>
            <span>Finalized</span>
            <strong>{{ chain.finalized_height }}</strong>
        </article>

        <article>
            <span>Validators</span>
            <strong>{{ chain.validator_count }}</strong>
        </article>

        <article>
            <span>Status</span>
            <strong>{{ chain.status }}</strong>
        </article>

        <article>
            <span>Transactions</span>
            <strong>{{ chain.transaction_count }}</strong>
        </article>
    </section>

    <section class="card-grid">
        <article class="card">
            <span>Chain ID</span>
            <strong>{{ chain.chain_id }}</strong>
        </article>

        <article class="card">
            <span>Symbol</span>
            <strong>{{ chain.symbol }}</strong>
        </article>

        <article class="card">
            <span>Consensus</span>
            <strong>{{ chain.consensus }}</strong>
        </article>
    </section>

    <section class="head-card">
        <span>Canonical head</span>
        <code>{{ chain.head_hash }}</code>
    </section>
</main>

@loading chain:
    <section class="status-panel">
        Connecting to SIBAQ Smart Chain…
    </section>

@error chain as error:
    <section class="status-panel error">
        <h2>Chain telemetry unavailable</h2>
        <p>{{ error.message }}</p>
        <button on:click="chain.retry">Retry</button>
    </section>

@stale chain:
    <span class="stale-indicator">
        Last update {{ chain.last_success_at }}
    </span>
```

**Key feature**: `initial server` tells AGS:

- Fetch `/api/status` during server rendering
- Embed actual values in HTML (no placeholders)
- Serialize state into hydration script
- Browser reads hydration state without duplicate request
- Continue polling every 1000ms for updates

### AGI Backend Adapter (examples/agi/sibaq_status.agi)

```agi
module App.Blockchain.SibaqStatus

use std.http
use std.json
use std.time
use blockchain.evm

const SIBAQ_RPC: string = "https://rpc.sibaq.us"
const SIBAQ_CHAIN_ID: i64 = 1990
const SIBAQ_SYMBOL: string = "SBQ"

struct ChainStatus:
    chain_id: i64
    symbol: string
    status: string
    height: i64
    finalized_height: i64
    validator_count: i64
    transaction_count: i64
    consensus: string
    head_hash: string
    updated_at: string

enum ChainError:
    WrongChain { expected: i64, received: i64 }
    NetworkError { message: string }
    InvalidResponse { message: string }

async fn load_chain_status() -> Result<ChainStatus, ChainError>:
    let client = evm.rpc_client({
        "url": SIBAQ_RPC,
        "timeout_ms": 5000
    })

    let chain_id = await client.chain_id()

    if chain_id != SIBAQ_CHAIN_ID:
        return Err(ChainError.WrongChain {
            "expected": SIBAQ_CHAIN_ID,
            "received": chain_id
        })

    let block_number = await client.block_number()
    let block = await client.get_block_by_number(block_number, false)

    let telemetry = await http.get_json(
        SIBAQ_RPC + "/api/status"
    )

    return Ok(ChainStatus {
        "chain_id": chain_id,
        "symbol": SIBAQ_SYMBOL,
        "status": telemetry.status ?? "online",
        "height": block_number,
        "finalized_height": telemetry.finalized_height ?? block_number,
        "validator_count": telemetry.validator_count ?? 0,
        "transaction_count": telemetry.transaction_count ?? 0,
        "consensus": telemetry.consensus ?? "Proof of Stake",
        "head_hash": block.hash,
        "updated_at": time.now_iso8601()
    })
```

### API Route (examples/agi/routes_api_status.agi)

```agi
module App.Routes.ApiStatus

use Framework.Http
use App.Blockchain.SibaqStatus

@get "/api/status"
async fn status(request: Request) -> Response:
    match await load_chain_status():
        Ok(chain):
            return Response.json({
                "ok": true,
                "chain": chain
            })

        Err(error):
            return Response.json({
                "ok": false,
                "error": error.public_message()
            }, status=503)
```

### Acceptance Test Checklist

```bash
# 1. Parse AGS template
agilang-native check examples/ags/sibaq-dashboard.ags
  └─ ✅ No syntax errors

# 2. Compile AGS + AGI together
agilang-native build examples/ags/sibaq-dashboard.ags \
    --agi-symbols examples/agi/sibaq_status.agi \
    --agi-symbols examples/agi/routes_api_status.agi
  └─ ✅ Produces executable

# 3. Run server
./build/sibaq-observability --listen=0.0.0.0:8080
  └─ ✅ Server ready

# 4. Verify server-side rendering with real data
curl http://localhost:8080/
  └─ ✅ Response contains:
    <strong>15820</strong>  (actual chain height)
    <strong>1990</strong>   (chain ID)
    <strong>SBQ</strong>    (symbol)
    <code>0xabc123...</code> (real block hash)
    NOT <strong>--</strong> (no placeholders)

# 5. Verify hydration state embedded
curl http://localhost:8080/ | grep 'ags-state'
  └─ ✅ Response includes:
    <script type="application/agilang-state" id="ags-state">
    { "chain": { "chain_id": 1990, "height": 15820, ... } }
    </script>

# 6. Type checking catches errors
echo '{{ chain.heigth }}' >> broken.ags
agilang-native check broken.ags
  └─ ❌ Compile error: Unknown field 'heigth'

# 7. Chain ID validation works
# Mock /api/status returning chain_id: 9999
./build/sibaq-observability
  └─ ❌ 503 error: "Chain ID mismatch: expected 1990, got 9999"

# 8. HTML escaping prevents XSS
# Mock /api/status with: status: "<img src=x onerror=alert('xss')>"
curl http://localhost:8080/
  └─ ✅ Response contains: &lt;img src=x onerror=alert('xss')&gt;
  └─ ✅ No JavaScript execution

# 9. Browser hydration works
# Open http://localhost:8080 in browser
# Check Network tab:
  └─ ✅ One initial HTML request (complete page)
  └─ ✅ One /api/status request for hydration state
  └─ ✅ Subsequent polls every 1000ms (not on page load)
  └─ ✅ Only affected DOM nodes update (height, validators, etc.)
  └─ ✅ No full page refresh
  └─ ✅ No virtual DOM diffing

# 10. Dependency graph is minimal
# Watch browser console:
  └─ ✅ Node registry contains only 9 entries (one per binding)
  └─ ✅ On update with only height changed, only node 7 updates
  └─ ✅ Head, card grid, legend unchanged

# 11. All tests pass
cargo test -p agilang-ags-lexer
cargo test -p agilang-ags-parser
cargo test -p agilang-ags-semantic
cargo test -p agilang-ags-ssr
cargo test -p agilang-ags-hydration
cargo test -p agilang-evm-provider  # For chain_id validation
  └─ ✅ All tests pass
  └─ ✅ No Clippy warnings
```

---

## Comprehensive Test Specifications

These tests must pass before Phase 3A is considered complete.

### Compiler & Semantic Tests

```
✅ @page directive is parsed
✅ @live directive is parsed with all parameters
✅ every 1000 becomes Duration(1000 ms)
✅ initial server flag recognized
✅ retry exponential recognized
✅ timeout 5000 becomes 5000ms
✅ chain bindings are registered
✅ unknown fields fail compilation with suggestion
✅ duplicate @live bindings fail compilation
✅ invalid intervals (negative, zero) fail compilation
✅ @loading/@error/@stale directives recognized
✅ error binding captures error type
```

### Server-Side Rendering Tests

```
✅ Initial API data appears in generated HTML
✅ Metadata from @page appears in <title> tag
✅ seo_description appears in meta tag
✅ All values are HTML-escaped
✅ No placeholder (--) emitted when data available
✅ Hydration state embedded in <script type="application/agilang-state">
✅ Hydration state matches server-rendered values
✅ HTTP 503 returned when chain_id validation fails
✅ Chain ID mismatch error message clear
```

### Reactive Update Tests

```
✅ Only changed nodes are updated (dependency graph)
✅ No full-page reload occurs
✅ No duplicate initial API request occurs
✅ Polling starts after hydration completes
✅ Polling stops when component unmounted/destroyed
✅ Retry follows exponential backoff (1s, 2s, 4s, 8s...)
✅ Stale data remains visible during retry
✅ Last successful state preserved across failures
✅ Error state rendered when max retries exceeded
✅ Manual retry via button restarts polling
```

### Chain Validation Tests

```
✅ eth_chainId 0x7c6 (1990 decimal) passes
✅ eth_chainId other than 0x7c6 produces error
✅ Hexadecimal quantities normalized (0x7c6 → 1990)
✅ Malformed hexadecimal fails safely with message
✅ Missing required field (e.g., head_hash) produces typed error
✅ Timeout on slow RPC produces stale state
✅ Connection refused produces error state
✅ Invalid JSON response produces error with details
```

### Security Tests

```
✅ HTML in chain.status is escaped
✅ HTML in head_hash is escaped
✅ HTML in symbol is escaped
✅ API response cannot inject JavaScript
✅ Script tags in data rendered as text
✅ Event handler attributes escaped
✅ Endpoint URL cannot be changed by response data
✅ Secrets are never serialized into hydration state
✅ Authentication headers (if used) not exposed in client
✅ XSS prevention verified with OWASP payloads
```

### Performance Tests

```
✅ Initial page load includes complete HTML
✅ Hydration state is compact (< 5KB JSON)
✅ Poll response processed without blocking (< 50ms)
✅ Only 1 DOM node updated per binding change
✅ No GC pressure from repeated updates
✅ Memory stable after 1000+ updates
```

### Browser Compatibility Tests

```
✅ Works on Chrome 90+
✅ Works on Firefox 88+
✅ Works on Safari 14+
✅ Works on Edge 90+
✅ Fallback for browsers without EventSource
✅ Graceful degradation without ES2020 features
```

---

## Deliverables Summary

| Crate                      | Lines of Code | Feature                  |
| -------------------------- | ------------- | ------------------------ |
| `agilang-ags-lexer`        | 500           | Tokenization             |
| `agilang-ags-ast`          | 300           | AST definitions          |
| `agilang-ags-parser`       | 800           | Parser                   |
| `agilang-view-ir`          | 200           | View IR                  |
| `agilang-ags-semantic`     | 600           | Type checking            |
| `agilang-ags-ssr`          | 400           | SSR codegen              |
| `agilang-ags-hydration`    | 300           | Hydration codegen        |
| `agilang-compiler-plugins` | 400           | Plugin system            |
| Tests                      | 2000+         | Comprehensive test suite |

**Total**: ~5500 lines of implementation code + 2000+ lines of tests

---

## Success Criteria (Phase 3A Complete)

✅ `cargo build` succeeds for all AGS crates  
✅ `cargo test` passes for all AGS crates  
✅ `cargo clippy` reports no warnings  
✅ Smart Chain fixture parses without errors  
✅ Smart Chain fixture type checks successfully  
✅ Smart Chain fixture generates valid HTML  
✅ Smart Chain fixture generates valid JavaScript  
✅ `{{ chain.heigth }}` produces compile error with suggestion  
✅ HTML escaping prevents XSS  
✅ Feature matrix updated with AGS completion status

---

## Version

**Release**: AGILANG Native Runtime v0.6.0  
**Phase**: 3A — AGS Compiler Foundation  
**Status**: Under Development  
**ETA**: Q3 2026

This is the first major deliverable where AGS transitions from "documented architecture" to "verified implementation."

---

**Document Version**: 1.0  
**Created**: 2026-07-21  
**Status**: Implementation roadmap (not yet started)
