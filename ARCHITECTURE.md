# AGILANG Native Runtime Architecture

## Native-Only Platform Architecture

```
┌─────────────────────────────────────┐
│ AGI Backend Code                    │  Backend services, controllers, routes,
│ (server-side AGILANG)               │  agents, and business logic
├─────────────────────────────────────┤
│ AGS Reactive Templates              │  View layer, reactive components,
│ (template-driven rendering)         │  forms, and state management
├─────────────────────────────────────┤
│ Generated JavaScript/WebAssembly    │  Browser output only;
│ (reactive hydration in browser)     │  NOT part of backend execution
├─────────────────────────────────────┤
│ Rust Native Runtime                 │  Compiler, runtime kernel, ABI,
│ (ALL native execution)              │  networking, crypto, database,
│                                     │  AI inference/training, EVM
├─────────────────────────────────────┤
│ Operating System                    │  Platform services, I/O, hardware
│ CPU / GPU / NPU / Accelerators      │
└─────────────────────────────────────┘
```

**Critical Rule: Python is not in this stack at any layer.**

## Language Boundaries

| Language            | Layer          | Responsibility                                                                                                          |
| ------------------- | -------------- | ----------------------------------------------------------------------------------------------------------------------- |
| **AGI**             | Backend        | Controllers, services, routes, policies, agents, AIFlow definitions, business logic                                     |
| **AGS**             | View Compiler  | Independent compiled language: page metadata, routing, reactive data sources, UI declaration, type-safe bindings        |
| **JavaScript/WASM** | Browser        | Generated client code only; hydration, DOM updates, browser events                                                      |
| **Rust**            | Native Runtime | Compiler, ABI, async executor, networking, TLS, database, cryptography, EVM, AI (tensor/training), platform integration |

## Boundary

Compiler-generated machine code calls the versioned C ABI in `agilang-runtime-abi`. Rust's native ABI is never exposed. Breaking ABI changes require a new major ABI; additive changes increment the minor ABI.

**No Python code anywhere in the native execution path.**

## Unified Compiler Architecture

```
              AGI Source (.agi)          AGS Source (.ags)
                    │                          │
                    ▼                          ▼
            AGI Lexer/Parser             AGS Lexer/Parser
                    │                          │
                    ▼                          ▼
            AGI Typed AST                 AGS Typed AST
                    │                          │
                    └──────────┬───────────────┘
                               │
                               ▼
                    Shared Semantic Model
                    (symbol table, types, scope)
                               │
                ┌──────────────┼──────────────┐
                │              │              │
                ▼              ▼              ▼
           Native Codegen   SSR Renderer   Hydration Graph
                │              │              │
                ▼              ▼              ▼
          Rust/C Executable   HTML+Metadata  Browser Runtime
```

**Key insight**: AGI and AGS compile together with shared type information. When AGI exports a `DashboardModel`, AGS automatically knows its structure for type-safe bindings.

## Value ownership

AGILANG values are immutable runtime objects. Collections use copy-on-write/persistent semantics. Native callers hold opaque 64-bit handles. The low 32 bits identify a registry slot and the high 32 bits carry its generation. Releasing a handle increments the generation, which makes stale handles invalid even after slot reuse.

## Failure isolation

Every fallible exported operation executes behind `catch_unwind`. No Rust panic is permitted to cross the ABI. Errors are stored per operating-system thread and can be retrieved as structured JSON. A status code is always returned for fallible operations.

## Runtime layers

```text
Generated AGILANG machine code / LLVM JIT / C host
                         │
                 agilang_runtime.h
                         │
              Stable versioned C ABI
                         │
      ┌──────────────────┼──────────────────┐
      │                  │                  │
  Core values       Async executor     Platform policy
  and handles       and channels       and filesystem
```

## Production completion gates

The full AGILANG runtime reaches 1.0 only after these additional gates:

1. ✅ Callable/closure ABI and stack-frame metadata
2. ✅ Structured exceptions with source spans and native stack traces
3. ✅ Cycle-aware managed heap or compiler-enforced ownership model
4. 🔄 HTTP/1.1, HTTP/2, WebSocket, DNS, TLS, and certificate policy
5. 🔄 Cryptographic provider with secure key handles and zeroization
6. 🔄 Native module loader, signed packages, and dependency lockfiles
7. 🔄 Database provider interfaces and transaction semantics
8. 🔄 EVM, blockchain, ATCP, AI (tensor/training), payment, and web standard modules
9. 🔄 LLVM AOT/JIT intrinsic registration and conformance suite
10. 🔄 Fuzzing, sanitizers, ABI compatibility tests, SBOM, signing, MSI/DEB/RPM/PKG installers

✅ = Complete
🔄 = In Progress
⏳ = Planned

### AGS Typed View IR

The AGS compiler generates an internal **Typed View Intermediate Representation** that enables efficient, native reactive rendering:

```
AGS Source (.ags)
    ↓
AGS Parser (validated against shared symbol table)
    ↓
Typed View IR
    │
    ├─ ViewNode metadata
    ├─ TextBinding(chain.height)
    ├─ EventListener(onClick)
    └─ DependencyGraph (which nodes update on which data changes)
    │
    ├──────────► SSR Renderer (generates complete HTML server-side)
    │
    └──────────► Hydration Graph (minimal JS for reactive updates)
```

**Example**:

```agi
@live chain from "/api/status" every 1000

<section>
    {{ chain.height }}
</section>
```

Becomes:

```rust
ViewNode {
    tag: "section",
    children: vec![
        ViewNode::TextBinding {
            path: "chain.height",
            type: Type::I64,
        }
    ],
    data_source: Some(DataSource {
        name: "chain",
        endpoint: "/api/status",
        interval: 1000,
    }),
}
```

**Advantages**:

- **No virtual DOM diffing** — The compiler builds a dependency graph; only bound nodes update
- **Type-safe bindings** — Compile-time error if `chain.heigth` (typo) is used
- **Native reactive engine** — Simple reference updates, no DOM reconciliation overhead
- **Zero React equivalent** — Direct node updates via the hydration graph

### AGS Compilation Targets

The Typed View IR produces two complementary outputs:

#### 1. Server-Side Rendering (SSR)

```
Typed View IR → Rust renderer → Complete HTML + metadata
```

The server renders full, SEO-friendly HTML before browser execution.

**Capabilities:**

- Complete page rendering server-side
- Layout composition
- Component instantiation with props
- Slot composition
- Data binding from AGI services (via shared symbol table)
- Form rendering with server-side validation
- SEO metadata injection

#### 2. Browser Hydration (Reactive Runtime)

```
Typed View IR → Dependency graph → Generated JavaScript/WASM → Browser runtime
```

The browser receives minimal reactive code that updates only affected nodes.

**Capabilities:**

- Reactive state management (only DOM nodes re-render when bindings change)
- Event handling (click, submit, input)
- WebSocket/SSE integration (live data from `/api/status`)
- Form event binding with validation
- Component lifecycle (mount, update, unmount)
- No virtual DOM; direct dependency-driven updates

### AIFlow Implementation Path

All AI operations follow this native pipeline:

```
AGI code defining training/inference
    ↓
AGILANG typed compiler
    ↓
Native AIFlow intermediate representation (IR)
    ↓
Rust tensor/training runtime
    ↓
CPU / CUDA / DirectML / Vulkan / NPU execution
```

**No Python models, PyTorch wrappers, TensorFlow layers, or lazy-loaded Python interpreters.**

Rust native implementation covers:

- Tensor storage and operations
- Automatic differentiation (autograd)
- Optimizers (Adam, SGD, AdamW, etc.)
- Model layers (Linear, Embedding, Attention, Transformer blocks)
- Tokenizer runtime
- Checkpoint serialization
- Quantization and mixed precision
- GPU device management
- Distributed training

## Compatibility policy

- **ABI major**: breaking symbol, layout, ownership, or behavior change
- **ABI minor**: additive symbol or capability
- **ABI patch**: implementation fix without contract changes
- **Runtime package version**: follows semantic versioning independently of ABI version


## Native Compute ABI (v1.1)

The runtime now exposes owned tensor and model handles through the stable C ABI.
Tensor storage is allocated and released inside Rust; callers never free Rust
memory directly. Supported ABI operations include tensor creation, shape/data
copy-out, addition, subtraction, Hadamard multiplication, matrix multiplication,
scaling, ReLU, softmax, MSE, linear-model creation, prediction, and batch SGD.

The compute backend reports `native=true`, `python_required=false`, and
`wasm_required=false`. This is a standalone CPU baseline. GPU and NPU providers
remain optional acceleration backends and are not required for correctness.

Generated C now includes explicit free, reserve, insert, remove, pop, and
recursive nested-list destruction helpers. Compiler lifetime insertion remains
a separate frontend pass; runtime ownership primitives are no longer missing.
