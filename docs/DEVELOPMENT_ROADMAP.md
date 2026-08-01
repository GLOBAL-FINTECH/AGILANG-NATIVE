# AGILANG Native Development Roadmap

## Overview

This document outlines the six-phase development plan to transform AGILANG into a complete, native-only platform suitable for production workloads. Each phase builds on the previous one while maintaining compatibility with the canonical AGILANG specification.

### Unified Compiler Architecture

AGILANG uses a **single compiler pipeline** with two coordinated languages:

```
AGI Source (.agi)              AGS Source (.ags)
     ↓                               ↓
AGI Compiler          AGS Compiler (as of Phase 3)
     ↓                               ↓
  Typed AST             Typed View IR
     ↓                               ↓
  ┌──────────────────────────────────┘
  │
  ▼
Shared Semantic Model
(symbol table, types, module bindings)
  │
  ├──► Native Codegen (Rust → C)
  ├──► SSR Renderer (Rust → HTML)
  └──► Hydration Graph (minimal JS/WASM)
```

**Key principle**: Both languages compile together with a shared symbol table, enabling:

- Type-safe cross-boundary bindings
- Zero-cost abstractions
- No runtime guessing about types
- Compile-time verification of data flows

---

## Phase 1: Freeze Language Contracts (Foundation)

**Goal**: Establish immutable specifications before implementation.

**Deliverables**:

- ✅ AGI syntax specification (bootstrap complete)
- ✅ AGS syntax specification (bootstrap complete)
- ✅ Typed abstract syntax tree (AST) design
- ✅ Native C ABI specification
- ✅ Module system design
- ✅ Ownership and lifetime rules
- ✅ Error model specification
- ✅ Async/await model specification
- ✅ Package and artifact format definitions

**Output Files**:

- `docs/AGI_SYNTAX_SPECIFICATION.md`
- `docs/AGS_SYNTAX_SPECIFICATION.md`
- `docs/AGILANG_TYPE_SYSTEM.md`
- `docs/C_ABI_SPECIFICATION.md`
- `docs/OWNERSHIP_AND_LIFETIMES.md`
- `docs/ERROR_HANDLING_MODEL.md`
- `docs/ASYNC_AND_CONCURRENCY.md`
- `docs/PACKAGE_FORMAT_SPECIFICATION.md`

**Success Criteria**:

- All language changes tracked in `compat/feature-matrix.csv`
- No breaking changes to frozen specifications
- Full compatibility coverage defined

---

## Phase 2: Build Common Compiler Core (Infrastructure)

**Goal**: Create the shared compilation infrastructure for both AGI and AGS.

**Deliverables**:

### Lexer & Parser

```
AGI Lexer
├─ Tokenization
├─ Keyword recognition
├─ Operator handling
└─ Error recovery

AGI Parser
├─ Expression parsing
├─ Statement parsing
├─ Function declarations
├─ Type annotations
└─ Module imports

AGS Lexer
├─ Template expression recognition
├─ Component tags
├─ Attribute binding
└─ Script block detection

AGS Parser
├─ Template structure
├─ Component composition
├─ Directive parsing
├─ Reactive state blocks
└─ Event binding
```

### Type System

```
Type Checker
├─ Inference
├─ Unification
├─ Generics
├─ Nullability
├─ Coercion rules
└─ Type compatibility

Capability Checker
├─ Effect tracking
├─ Async boundaries
├─ Network operations
├─ File I/O
└─ Privilege levels
```

### Intermediate Representation (IR)

```
Typed IR
├─ Constants
├─ Type definitions
├─ Functions
├─ Imports/exports
├─ Module structure
└─ Debug metadata
```

### Developer Tools

```
Diagnostics
├─ Error codes (AGI-ERR-*)
├─ Warnings
├─ Hints
├─ Source spans
└─ Useful messages

Language Server Protocol (LSP)
├─ Go to definition
├─ Hover information
├─ Code completion
├─ Signature help
└─ References

Formatter
├─ Indentation rules
├─ Line length
├─ Spacing conventions
└─ Comment alignment
```

**Success Criteria**:

- Full bootstrap syntax supported
- Zero diagnostic regressions from canonical
- LSP passes VS Code integration tests
- Formatting is deterministic and reversible

---

## Phase 3: Build AGS Compiler & Reactive Runtime (Q3 2026)

**Goal**: Implement AGS as a fully compiled language with its own compiler pipeline, Typed View IR, and native reactive rendering engine.

**Key Insight**: AGS is not HTML templating. It's a compiled language that generates:

1. Server-side rendered HTML (complete pages for SEO)
2. Typed View IR (dependency graph for reactive updates)
3. Hydration graph (minimal browser code for DOM updates)

### Architecture

```
AGS Source (.ags)
    ↓
AGS Lexer (tokenization + validation)
    ↓
AGS Parser (syntax analysis)
    ↓
Typed View IR
    │
    ├──► SSR Renderer (Rust) → HTML response
    │
    ├──► Hydration Graph → JavaScript generator
    │
    └──► IDE Support (LSP, diagnostics, completion)
```

### Deliverables

#### 1. AGS Compiler Core

```
AGS Lexer
├─ Template directives (@page, @live, @component)
├─ Binding expressions ({{ ... }})
├─ Attribute bindings (bind:value, @click)
└─ Script blocks (<script ags>)

AGS Parser
├─ Template structure parsing
├─ Component composition
├─ Directive parsing
├─ Reactive state declarations
└─ Event binding

Semantic Analyzer
├─ Type checking against shared symbol table
├─ Directive validation
├─ Data source type resolution
└─ Component prop validation
```

#### 2. Typed View IR

```
ViewNode Structure
├─ Tag and attributes
├─ Child nodes
├─ Bindings (TextBinding, EventBinding, PropertyBinding)
├─ Data sources (@live, @stream)
└─ Dependency graph (which nodes update on data changes)
```

**Example IR for binding**:

```rust
ViewNode {
    tag: "div",
    children: vec![
        ViewNode::TextBinding {
            path: "chain.height",
            type: Type::I64,
            node_id: 42,
        }
    ],
    data_sources: vec![
        DataSource {
            name: "chain",
            endpoint: "/api/status",
            interval_ms: 1000,
        }
    ],
    dependency_graph: DependencyGraph {
        bindings: vec![("chain.height", vec![42])],  // node 42 updates
    },
}
```

#### 3. SSR Renderer

```
Typed View IR → Rust renderer → HTML response

Capabilities:
├─ Safe HTML generation (XSS prevention via escaping)
├─ Server-side data binding
├─ Component rendering with props
├─ Slots and slot composition
├─ Conditional rendering (@if directives)
├─ Loops (@for directives)
├─ SEO metadata injection
└─ Layout inheritance and nesting
```

#### 4. Hydration Graph & Browser Runtime

```
Typed View IR → Dependency graph → Minimal JavaScript

Zero Virtual DOM:
├─ Browser receives hydration graph
├─ Direct DOM node references
├─ Event listeners attached to marked nodes
├─ On data update: only affected bindings re-render
└─ No DOM diffing, no reconciliation algorithm
```

**Browser updates** follow the dependency graph:

```javascript
// Server told us: "chain.height" updates node 42
dataSource.addEventListener("update", (data) => {
  let node = nodeRegistry[42]; // Direct reference
  node.textContent = data.height;
  // No diffing, no virtual DOM, O(1) update
});
```

#### 5. Compiler Plugin System

AGS plugins extend the language without modifying the core compiler:

```
Reserved Plugin Namespaces:
├─ @live (HTTP/API bindings)
├─ @stream (WebSocket/SSE streams)
├─ @ai (AIFlow integration)
├─ @agent (Autonomous agents)
├─ @chart (Visualizations)
├─ @map (Geographic data)
├─ @wallet (Blockchain operations)
├─ @payment (Payment processors)
└─ @cache (Caching strategies)
```

**See [COMPILER_PLUGIN_SYSTEM.md](COMPILER_PLUGIN_SYSTEM.md) for full details.**

### Phase 3 Feature Set

#### Page & Routing

```agi
@page title="Smart Chain Dashboard"
@layout base_layout
@route "/dashboard/:chain_id"
```

#### Reactive Data Bindings

```agi
@live chain from "/api/status" every 1000
@live validators from "/api/validators" every 5000

<section>
    <h1>Chain: {{ chain.name }}</h1>
    <p>Height: {{ chain.height }}</p>

    <ul>
        @for validator in validators:
            <li>{{ validator.address }}: {{ validator.stake }}</li>
    </ul>
</section>
```

#### Data Binding with Type Safety

```agi
// Compile-time error if chain.heigth (typo)
{{ chain.heigth }}  // ❌ ERROR: field not found in ChainStatus
{{ chain.height }}  // ✅ OK: matches shared symbol table
```

#### Component System

```agi
@component card
    @prop title: string
    @prop content: string
    @slot footer

<div class="card">
    <h2>{{ title }}</h2>
    <p>{{ content }}</p>
    <footer>
        <slot name="footer" />
    </footer>
</div>
```

#### Form Binding & Validation

```agi
<form @submit="submit_form">
    <input type="email" bind:value="user.email" />
    <input type="text" bind:value="user.name" />
    <button>Submit</button>
</form>

<script ags>
fn submit_form(form_data):
    // Form validation, API call, etc.
    // Called on form submission
</script>
```

#### SEO & Metadata

```agi
@page
    title = "Dashboard"
    description = "Real-time chain monitoring"
    og_image = "/images/dashboard-preview.png"

<head>
    <meta property="og:title" content="{{ page.title }}" />
    <meta property="og:description" content="{{ page.description }}" />
</head>
```

### Success Criteria

- ✅ AGS compiles to Typed View IR without relying on HTML templating
- ✅ Type checking works across AGI ↔ AGS boundary (shared symbol table)
- ✅ SSR renders complete HTML server-side with no JavaScript required
- ✅ Hydration uses dependency graph (no virtual DOM)
- ✅ Complex nested layouts render and update correctly
- ✅ XSS prevention is automatic via escaping
- ✅ IDE support (IntelliSense, go-to-definition, safe refactoring)
- ✅ Plugin system allows new directives without core compiler changes
- ✅ No Python dependencies anywhere in the rendering pipeline

### Deliverable Files

- `crates/agilang-ags-compiler/` — AGS compiler implementation
- `crates/agilang-ags-ir/` — Typed View IR definitions
- `crates/agilang-ags-renderer/` — SSR renderer in Rust
- `crates/agilang-ags-hydration/` — Browser hydration runtime
- `docs/AGS_COMPILER_ARCHITECTURE.md` — Implementation guide
- `docs/COMPILER_PLUGIN_SYSTEM.md` — Plugin development guide
- `examples/ags/` — Sample AGS templates and projects

---

## Phase 4: Implement Native AIFlow (AI Execution)

**Goal**: Build complete tensor and training runtime in Rust.

### Inference First (Simpler)

```
Step 1: Tensor Storage
├─ Multi-dimensional arrays
├─ dtypes (f32, f64, i32, etc.)
├─ Memory layout (row/column major)
├─ Copy-on-write semantics
└─ Zero-copy views

Step 2: Model Loading
├─ Weights deserialization
├─ Checkpoint formats (.agimodel)
├─ Quantization support
├─ Version compatibility
└─ Integrity verification

Step 3: Tokenization
├─ Byte-pair encoding (BPE)
├─ Vocabulary loading
├─ Special token handling
├─ Encoding/decoding
└─ Efficiency optimizations

Step 4: CPU Inference
├─ Forward passes
├─ Batch processing
├─ Sequence generation
├─ Streaming output
└─ Memory efficiency

Step 5: GPU Execution
├─ CUDA device management
├─ Kernel launches
├─ Memory pinning
├─ Async execution
└─ Multi-GPU support

Step 6: Quantization
├─ INT8 inference
├─ Dynamic quantization
├─ Calibration
├─ Mixed precision
└─ Performance profiling

Step 7: Streaming Generation
├─ Token-by-token output
├─ Stop sequences
├─ Temperature sampling
├─ Beam search
└─ Structured generation
```

### Training Second (More Complex)

```
Step 1: Autograd
├─ Forward accumulation
├─ Backward propagation
├─ Gradient taping
├─ Checkpointing (memory optimization)
└─ Custom gradients

Step 2: Optimizers
├─ SGD
├─ Adam
├─ AdamW
├─ Learning rate schedules
└─ Gradient clipping

Step 3: Dataset Streaming
├─ Batch loading
├─ Prefetching
├─ Shuffling
├─ Caching
└─ Multi-threaded I/O

Step 4: Checkpoints
├─ State serialization
├─ Resume from checkpoint
├─ Version compatibility
├─ Artifact validation
└─ Distributed state

Step 5: Mixed Precision
├─ FP32 master weights
├─ FP16 computation
├─ Loss scaling
├─ Gradient accumulation
└─ Performance monitoring

Step 6: Gradient Accumulation
├─ Accumulation buffer
├─ Multi-step gradient
├─ Synchronization
├─ Convergence stability
└─ Memory efficiency

Step 7: Distributed Training
├─ Data parallelism
├─ Model parallelism
├─ Communication patterns
├─ Fault tolerance
└─ Performance profiling
```

### Native Module Exposure

```
use ai.tensor
use ai.nn
use ai.optim
use ai.data

model MyModel:
    embedding: nn.Embedding
    transformer: TransformerBlock
    output: nn.Linear

    fn forward(tokens: Tensor<i64>) -> Tensor<f32>:
        let hidden = self.embedding(tokens)
        hidden = self.transformer(hidden)
        return self.output(hidden)

fn main():
    let model = MyModel.new()
    let optimizer = ai.optim.AdamW.new(model.params())
    train(model, optimizer)
```

**Success Criteria**:

- Inference latency matches or exceeds reference implementations
- Training convergence curves match canonical
- All quantization variants pass accuracy tests
- Distributed training scales to multiple GPUs
- Memory usage is within expected bounds

---

## Phase 5: Add Migration Tools (Bridge)

**Goal**: Reduce manual effort in moving from Python to native.

**Deliverables**:

### Python-to-AGI Migrator

```
Command: agi migrate python-ai <source-directory>

Input: Python training script (restricted subset)
├─ Class definitions
├─ Function definitions
├─ Type hints
├─ Control flow
├─ Data structures
├─ Tensor operations
└─ Selected library calls

Output:
├─ Generated AGI files
├─ Migration report
├─ Unsupported constructs list
├─ Test comparison script
└─ Artifact conversion plan
```

### Features

- Pattern recognition for common ML patterns
- Automatic type inference where possible
- Library-specific translation (PyTorch → native AIFlow)
- Limitation documentation
- Diff viewing between source and target

### Artifact Converter

```
Command: agi convert-artifacts <model-dir> <output-dir>

Input: Python checkpoint files
├─ .pkl weights
├─ .safetensors
├─ Hugging Face checkpoints
└─ Custom formats

Output: Native .agimodel files
├─ Metadata
├─ Weights (optimized layout)
├─ Quantization params
├─ Version info
└─ Checksums
```

**Success Criteria**:

- Common PyTorch models convert with <5% manual adjustment
- Generated AGI code compiles without errors
- Artifacts load and produce identical outputs
- Migration report is clear and actionable

---

## Phase 6: Prove Parity (Validation)

**Goal**: Verify one-to-one compatibility for all migrated features.

**Deliverables**:

### Differential Testing

```
For each migrated feature:

Run canonical implementation
  ↓
Capture output

Run native implementation
  ↓
Capture output

Compare:
├─ Tensor shapes match
├─ Numeric values match (within tolerance)
├─ Error conditions match
├─ Tokenizer outputs match
├─ Model logits match
├─ Loss curves match
└─ Generated tokens match
```

### Performance Profiling

```
Measure:
├─ Inference latency
├─ Training throughput
├─ Memory consumption
├─ Compilation time
├─ Startup time
└─ Device utilization
```

### Regression Testing

```
Automated tests:
├─ Unit tests for all kernel operations
├─ Integration tests for full pipelines
├─ Cross-platform tests (Windows, Linux, macOS)
├─ Multi-device tests (CPU, CUDA, Metal, etc.)
├─ Quantization tests
└─ Distributed training tests
```

### Benchmark Suite

```
Public benchmarks:
├─ Inference speed (tokens/sec)
├─ Training speed (samples/sec)
├─ Memory efficiency (GB per model size)
├─ Compilation time (seconds)
└─ Reproducibility (deterministic runs)
```

**Success Criteria**:

- All numeric outputs match within 0.01% relative tolerance
- Performance meets or exceeds reference
- Zero regressions on full test suite
- Benchmarks are reproducible
- Documentation includes performance expectations

---

## Timeline Estimate

| Phase   | Duration       | Status                      |
| ------- | -------------- | --------------------------- |
| Phase 1 | ✅ Complete    | Specifications frozen       |
| Phase 2 | ✅ In Progress | Core compiler ~80% complete |
| Phase 3 | 🔄 In Progress | SSR engine foundations done |
| Phase 4 | ⏳ Planned     | Q4 2026 estimated start     |
| Phase 5 | ⏳ Planned     | Q1 2027 estimated start     |
| Phase 6 | ⏳ Planned     | Q2 2027 estimated start     |

---

## Success Criteria Summary

- ✅ Immutable language contracts (Phase 1)
- ✅ Working compiler with LSP (Phase 2)
- 🔄 Production-grade SSR (Phase 3)
- ⏳ Native tensor runtime (Phase 4)
- ⏳ Python migration tools (Phase 5)
- ⏳ Comprehensive parity testing (Phase 6)
- 🎯 **Final Goal**: AGILANG v1.0 — a complete, native-only platform production-ready for enterprise use

---

**Document Version**: 1.0  
**Last Updated**: 2026-07-21  
**Status**: Mandatory development guide
