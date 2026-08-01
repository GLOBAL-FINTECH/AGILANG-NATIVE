# AGILANG Compiler Architecture

## Overview

The AGILANG native compiler transforms `.agi` backend code into optimized native executables through a multi-stage pipeline. The entire process is implemented in Rust with no Python dependencies.

## Compilation Pipeline

```
Source (.agi)
    ↓
[Stage 1: Lexical Analysis]
    Tokenization with indentation tracking
    → Token stream
    ↓
[Stage 2: Syntactic Analysis]
    Recursive-descent parser
    → Abstract Syntax Tree (AST)
    ↓
[Stage 3: Semantic Analysis]
    Type checking, symbol resolution
    → Type-checked AST + Symbol Tables
    ↓
[Stage 4: Intermediate Representation]
    Higher-order IR with type annotations
    → High-level IR (HIR)
    ↓
[Stage 5: Code Generation]
    Generate native C code
    → C source file
    ↓
[Stage 6: C Compilation & Linking]
    Compile with native C compiler
    Link with AGILANG runtime ABI
    → Native executable
    ↓
Executable (Windows/Linux/macOS)
```

## Crate Organization

### Core Compiler Crates

**`agilang-lexer`** — Tokenization

- Indentation-aware tokenization
- Keyword and operator recognition
- String, number, and identifier scanning
- Newline and indentation tracking (INDENT/DEDENT tokens)

**`agilang-parser`** — Syntax Analysis

- Recursive-descent parser
- Expression parsing with operator precedence
- Statement and function parsing
- Error recovery and synchronization

**`agilang-semantic`** — Type Checking & Analysis

- Symbol table construction and lookup
- Type inference and unification
- Function return type checking
- Scope management

**`agilang-ir`** — Intermediate Representation

- Type-annotated IR suitable for code generation
- Function, statement, and expression nodes
- Type information on all nodes for validation

**`agilang-codegen-c`** — C Code Generation

- Translates HIR to C source code
- Type mapping (i32 → int32_t, string → const char\*, etc.)
- Call translation with ABI naming (\_agi suffix convention)
- Expression and statement generation

### Supporting Crates

**`agilang-compiler`** — Compiler Orchestration

- Exposes high-level `hir()` function for pipeline stages
- Coordinates lexing, parsing, semantic analysis
- Returns typed IR or diagnostics

**`agilang-build`** — Project Building

- Loads .agi files and compiles to executable
- Generates C files with `emit_c` option
- Invokes linker to produce native binary
- Creates build manifests

**`agilang-linker`** — Linking & Native Compilation

- Invokes platform C compiler (MSVC on Windows, GCC/Clang on Unix)
- Links against AGILANG runtime library (agilang_runtime_abi)
- Produces platform-native executable

**`agilang-types`** — Type System

- Represents all AGILANG types
- Type name lookups and compatibility checking
- Type-to-C mapping

**`agilang-symbols`** — Symbol Management

- Symbol tables with scope tracking
- Symbol kind enum (Function, Variable, Parameter, Builtin)
- Lookup and duplicate detection

**`agilang-source`** — Source Management

- Source file loading and caching
- Span tracking for diagnostics
- Character indexing and slicing

**`agilang-diagnostics`** — Error Reporting

- Structured diagnostic codes (E001, E002, E1002, etc.)
- Source span information
- Rendered error messages

## Type System

### Primitive Types

```
i32         32-bit signed integer
i64         64-bit signed integer
u32         32-bit unsigned integer
u64         64-bit unsigned integer
f32         32-bit floating point
f64         64-bit floating point
bool        Boolean (true/false)
string      UTF-8 encoded text
void        No return value
```

### Composite Types (Planned)

```
Array<T>    Ordered collection
Map<K, V>   Key-value mapping
Result<T>   Success or error
Option<T>   Value or none
```

## ABI (Application Binary Interface)

### C ABI Convention

Generated functions use a naming convention:

```c
// AGI function: fn add(x: i32, y: i32) -> i32:
int32_t add_agi(int32_t x, int32_t y);

// AGI main entry point
int32_t main_agi(void);

// Standard C main for OS loader
int main(void);
```

### Runtime Bindings

The generated C code links against `agilang_runtime_abi`:

```c
// Print function (requires linking against runtime)
extern void agi_print(const char* msg);

// Future: More runtime functions as features expand
extern void agi_panic(const char* msg);
extern void* agi_alloc(size_t bytes);
extern void agi_free(void* ptr);
```

## Example: Compilation of a Simple Function

**AGI Source:**

```agi
fn greet(name: string) -> i32:
    print(name)
    return 0
```

**Stage 1: Tokens (Lexer Output)**

```
[Fn] [Identifier("greet")] [LParen] [Identifier("name")]
[Colon] [Identifier("string")] [RParen] [Arrow]
[Identifier("i32")] [Colon] [Newline] [Indent]
[Identifier("print")] [LParen] [Identifier("name")] [RParen] [Newline]
[Identifier("return")] [Integer(0)] [Newline] [Dedent] [Eof]
```

**Stage 2: AST (Parser Output)**

```
Program {
  functions: [
    Function {
      name: "greet",
      params: [Parameter { name: "name", ty: Some(TypeRef { name: "string" }) }],
      return_type: Some(TypeRef { name: "i32" }),
      body: [
        Stmt::Expr(Call { callee: Identifier("print"), args: [Identifier("name")] }),
        Stmt::Return { value: Some(Integer(0)) }
      ]
    }
  ]
}
```

**Stage 3: Symbol Tables (Semantic Analysis Output)**

```
GlobalScope {
  "greet" → Symbol { name: "greet", kind: Function, ty: (string) -> i32 },
  "print" → Symbol { name: "print", kind: Builtin, ty: (string) -> void }
}

FunctionScope("greet") {
  "name" → Symbol { name: "name", kind: Parameter, ty: string }
}
```

**Stage 4: HIR (Type-Annotated IR)**

```
HirProgram {
  functions: [
    HirFunction {
      name: "greet",
      params: [HirParameter { name: "name", ty: String }],
      return_type: I32,
      body: [
        HirStmt::Expr(
          HirExpr::Call {
            callee: Identifier("print", Function([String] -> Void)),
            args: [Identifier("name", String)],
            ty: Void
          }
        ),
        HirStmt::Return { value: Some(Integer(0, I32)) }
      ]
    }
  ]
}
```

**Stage 5: Generated C Code (Codegen Output)**

```c
#include <stdint.h>
#include <stdio.h>
#include <stdbool.h>

extern void agi_print(const char* msg);

int32_t greet_agi(const char* name);

int32_t greet_agi(const char* name) {
    agi_print(name);
    return 0;
}

int32_t agilang_main(void) {
    return (int32_t)main_agi();
}

int main(void) {
    return (int)agilang_main();
}
```

**Stage 6: Native Executable**

```
$ agilang build main.agi
Compiling native target and linking AGILANG runtime
Output: main.exe (or ./main on Linux/macOS)

$ ./main
Hello, World!
```

## Error Codes

### Lexical Errors (E001–E099)

| Code | Message                  | Cause                    |
| ---- | ------------------------ | ------------------------ |
| E001 | Unexpected character     | Invalid syntax character |
| E002 | Inconsistent indentation | Indent level mismatch    |

### Parse Errors (E100–E199)

| Code | Message                         | Cause                                   |
| ---- | ------------------------------- | --------------------------------------- |
| E100 | expected `fn`                   | Missing function keyword                |
| E101 | expected `(`                    | Missing opening paren in function       |
| E102 | expected `)`                    | Missing closing paren                   |
| E103 | expected `:`                    | Missing colon after function signature  |
| E104 | expected newline                | Expected line break                     |
| E105 | expected indented function body | Missing indent after function signature |
| E106 | expected end of function body   | Missing dedent at function end          |
| E110 | expected `=`                    | Missing equals in variable declaration  |
| E120 | expected expression             | Invalid expression                      |
| E121 | expected `)`                    | Missing closing paren in expression     |
| E122 | expected `)`                    | Missing closing paren in function call  |

### Semantic Errors (E1000–E1999)

| Code  | Message                  | Cause                          |
| ----- | ------------------------ | ------------------------------ |
| E1002 | duplicate symbol         | Function or variable redefined |
| E1004 | unknown type             | Type name not recognized       |
| E2008 | missing return statement | Function should return value   |

## Performance Characteristics

- **Compilation time**: Typically < 100ms for small programs
- **Executable size**: ~10–50 KB for basic programs (with runtime)
- **Startup time**: < 10ms for native executables
- **No GC overhead**: Rust's RAII semantics handle memory

## Future Expansion

### Planned Language Features

- Control flow: `if`, `else`, `while`, `for`, `match`
- Collections: `Array<T>`, `Map<K, V>`
- Module system: `use`, `module`, `include`
- Generics and type parameters
- Error handling: `Result<T, E>`, `?` operator
- Pattern matching
- Async/await

### Planned Code Generation Targets

- Direct machine code (via LLVM)
- WebAssembly (for browser execution)
- Native bytecode (`.agib` portable format)

---

**Document Version**: 1.0  
**Status**: Compiler bootstrap complete, language features in development
