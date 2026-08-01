# AGILANG Language Guide

## Overview

AGILANG is a statically-typed, compiled programming language designed for native execution with zero Python dependencies.

**Syntax philosophy**: Clean, readable, indentation-based (like Python's syntax) but semantics are strictly typed and compile to native C.

## Syntax at a Glance

```agi
// Comments use double slashes

fn main() -> i32:
    let x: i32 = 42
    let name: string = "AGILANG"
    print(name)
    return x

fn add(a: i32, b: i32) -> i32:
    return a + b
```

## Data Types

### Primitive Types

| Type     | Description               | Example                   |
| -------- | ------------------------- | ------------------------- |
| `i32`    | 32-bit signed integer     | `let x: i32 = 42`         |
| `i64`    | 64-bit signed integer     | `let x: i64 = 1000000`    |
| `u32`    | 32-bit unsigned integer   | `let x: u32 = 42`         |
| `u64`    | 64-bit unsigned integer   | `let x: u64 = 1000000`    |
| `f32`    | 32-bit float              | `let x: f32 = 3.14`       |
| `f64`    | 64-bit float              | `let x: f64 = 3.14159`    |
| `bool`   | Boolean                   | `let x: bool = true`      |
| `string` | UTF-8 text                | `let x: string = "hello"` |
| `void`   | No value (functions only) | `fn func() -> void:`      |

### Type Inference

Type annotations are required but can be omitted in some contexts:

```agi
fn main() -> i32:
    let explicit: i32 = 42       // Type specified
    let implicit = 42             // Type inferred (i32 from literal)
    return 0
```

**Note**: Full type inference is planned for Phase 2. Currently, all top-level annotations are required.

## Functions

### Basic Function

```agi
fn greet(name: string) -> i32:
    print(name)
    return 0

// Call it
fn main() -> i32:
    greet("Alice")
    return 0
```

### Function with Multiple Parameters

```agi
fn add(a: i32, b: i32) -> i32:
    return a + b

fn multiply(x: i32, y: i32, z: i32) -> i32:
    return x * y * z
```

### Functions with No Return

```agi
fn say_hello() -> void:
    print("Hello!")

// Or omit return type (defaults to void)
fn say_goodbye():
    print("Goodbye!")
```

### Main Entry Point

Every AGILANG program needs a `main` function that returns `i32`:

```agi
fn main() -> i32:
    print("Program started")
    return 0  // Exit code: 0 = success
```

The return value becomes the process exit code.

## Variables

### Declaration

```agi
fn main() -> i32:
    let x: i32 = 10
    return 0
```

### Type Annotation Syntax

```
let <name> : <type> = <value>
```

### Examples

```agi
fn main() -> i32:
    let count: i32 = 5
    let name: string = "Alice"
    let is_active: bool = true
    let ratio: f64 = 2.5
    return 0
```

### Scope

Variables are scoped to their function and accessible after declaration:

```agi
fn main() -> i32:
    let x: i32 = 5
    let y: i32 = 10
    let sum: i32 = x + y  // Can use x here
    return sum
```

## Operators

### Arithmetic

```agi
fn main() -> i32:
    let a: i32 = 10
    let b: i32 = 3

    let sum: i32 = a + b         // 13
    let diff: i32 = a - b        // 7
    let product: i32 = a * b     // 30
    let quotient: i32 = a / b    // 3

    return quotient
```

### Comparison

```agi
fn main() -> i32:
    let a: i32 = 10
    let b: i32 = 5

    let eq: bool = a == b        // false
    let ne: bool = a != b        // true
    let lt: bool = a < b         // false
    let le: bool = a <= b        // false
    let gt: bool = a > b         // true
    let ge: bool = a >= b        // true

    return 0
```

### Logical

```agi
fn main() -> i32:
    let x: bool = true
    let y: bool = false

    let both: bool = x && y      // false
    let either: bool = x || y    // true

    return 0
```

## Control Flow (Planned - Phase 2)

### If/Else (Coming Soon)

```agi
fn abs(x: i32) -> i32:
    if x >= 0:
        return x
    else:
        return 0 - x  // Negation
```

### While Loops (Coming Soon)

```agi
fn factorial(n: i32) -> i32:
    let result: i32 = 1
    let i: i32 = 1

    while i <= n:
        result = result * i
        i = i + 1

    return result
```

### For Loops (Coming Soon)

```agi
fn sum_to(n: i32) -> i32:
    let sum: i32 = 0

    for i in 1 to n:
        sum = sum + i

    return sum
```

## Collections (Planned - Phase 2)

### Arrays (Coming Soon)

```agi
fn main() -> i32:
    let numbers: Array<i32> = [1, 2, 3, 4, 5]
    let first: i32 = numbers[0]
    return first
```

### Maps (Coming Soon)

```agi
fn main() -> i32:
    let scores: Map<string, i32> = {
        "Alice": 95,
        "Bob": 87
    }
    let alice_score: i32 = scores["Alice"]
    return alice_score
```

## Standard Library

### Print Function

```agi
fn main() -> i32:
    print("Hello, World!")
    print("AGILANG native runtime")
    return 0
```

**Note**: `print` takes a single string argument. String interpolation and formatting are planned for Phase 2.

### Future Stdlib Functions

- `length(str)` — String length
- `substring(str, start, end)` — Extract substring
- `array.push(item)` — Add to array
- `array.pop()` — Remove from array
- `map.get(key)` — Get map value
- `map.set(key, value)` — Set map value

## Error Handling (Planned - Phase 2)

### Result Type (Coming Soon)

```agi
fn divide(a: i32, b: i32) -> Result<i32>:
    if b == 0:
        return Error("division by zero")
    return Ok(a / b)
```

### Option Type (Coming Soon)

```agi
fn get_user(id: i32) -> Option<User>:
    // Returns None if user not found
```

## Modules (Planned - Phase 2)

### Using Modules (Coming Soon)

```agi
use math.vectors
use http.client

fn main() -> i32:
    let v: Vector = vectors.create(1.0, 2.0, 3.0)
    return 0
```

### Defining Modules (Coming Soon)

```agi
// math/vectors.agi
module vectors

fn create(x: f64, y: f64, z: f64) -> Vector:
    // Implementation
```

## Type System Features

### Type Checking

All types are checked at compile time:

```agi
fn main() -> i32:
    let x: i32 = 42
    let y: string = "hello"

    // This would be a compile error:
    // let z: i32 = y  // Type mismatch

    return 0
```

### Type Compatibility

```agi
fn main() -> i32:
    let x: i32 = 5
    let y: i32 = x           // OK: same type
    let z: f64 = x           // ERROR: i32 is not f64
    return 0
```

**Note**: Implicit type coercion is limited in AGILANG. Explicit conversions are planned for Phase 2.

## Performance Characteristics

- **Zero-cost abstractions**: No runtime overhead for type system
- **Compile-time optimization**: Full constant folding
- **Native code**: Compiles to efficient C then machine code
- **No GC**: Stack-allocated values, RAII semantics
- **No virtual dispatch**: Monomorphic functions only

## Restrictions

### What's NOT in AGILANG (by design)

❌ **Python** — Permanently excluded  
❌ **Dynamic typing** — All types checked at compile time  
❌ **Runtime reflection** — No `typeof()` or dynamic dispatch  
❌ **Global mutable state** — Not supported (planned: immutable-first design)  
❌ **Macros** — Not planned; prefer functions  
❌ **Exception handling** — Uses Result<T> instead (coming Phase 2)

## Compilation Model

Every AGILANG program compiles through this pipeline:

```
Source (.agi)
    ↓
Lexer (tokenization)
    ↓
Parser (syntax analysis)
    ↓
Type checker (semantic analysis)
    ↓
Code generator (→ C code)
    ↓
C compiler (→ machine code)
    ↓
Executable (native binary)
```

No Python involved at any stage.

## Hello World - Explained

```agi
fn main() -> i32:
    print("Hello, World!")
    return 0
```

**Breakdown**:

- `fn` — Declare a function
- `main` — Function name (required entry point)
- `() -> i32` — Takes no parameters, returns i32 (exit code)
- `:` — Start of function body
- `print(...)` — Call print function
- `return 0` — Exit with status 0 (success)

## Best Practices

### 1. Name Functions Clearly

```agi
// Good
fn calculate_total_price(items: Array<Item>) -> f64:
    ...

// Avoid
fn calc(x: Array<Item>) -> f64:
    ...
```

### 2. Type Annotate Parameters

```agi
// Good: Types are explicit
fn add(x: i32, y: i32) -> i32:
    return x + y

// Avoid: Types are ambiguous
fn add(x, y):
    return x + y
```

### 3. Return Early

```agi
// Good: Early return for edge cases
fn process(data: string) -> Result<Data>:
    if data.length == 0:
        return Error("empty data")

    // Process normal case
    ...
```

### 4. Keep Functions Small

```agi
// Good: Single responsibility
fn validate_email(email: string) -> bool:
    return email.contains("@")

fn process_user(email: string, name: string) -> User:
    if not validate_email(email):
        return Error("invalid email")
    // ...
```

## Migration from Python

If you're coming from Python, here's what to expect:

| Python         | AGILANG                      |
| -------------- | ---------------------------- |
| `def func():`  | `fn func() -> void:`         |
| Dynamic typing | Static typing with inference |
| Lists          | `Array<T>` (planned)         |
| Dicts          | `Map<K, V>` (planned)        |
| Exceptions     | `Result<T>` (planned)        |
| `None`         | `Option<T>` (planned)        |
| Duck typing    | Trait system (planned)       |

## Resources

- [Compiler Architecture](COMPILER_ARCHITECTURE.md) — How code compiles
- [CLI Reference](CLI_REFERENCE.md) — Build and run commands
- [Getting Started](GETTING_STARTED.md) — Installation and first app
- [Examples](../examples/native/) — Real code samples

---

**Language Version**: 0.5.0 (bootstrap)  
**Status**: Core syntax complete, collections/control-flow in Phase 2
