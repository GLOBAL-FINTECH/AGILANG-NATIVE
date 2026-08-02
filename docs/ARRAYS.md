# AGILANG Native Arrays

AGILANG Native arrays are ordered, mutable collections whose element type is inferred from the array literal.

## Declaration

```agi
let values: array = [1.0, 2.0, 3.0]
```

The shorter inferred form is also valid:

```agi
let values = [1.0, 2.0, 3.0]
```

Array elements must currently be homogeneous. Mixing incompatible values is rejected during semantic analysis.

```agi
let valid = [1.0, 2.0, 3.0]
let invalid = [1.0, "two", 3.0]
```

## Reading an element

Array indexes begin at zero.

```agi
let values: array = [10.0, 20.0, 30.0]
let first: f64 = values[0]
let second: f64 = values[1]
```

The index must be an integer.

## Updating an element

Arrays declared with `let` are mutable:

```agi
let values: array = [10.0, 20.0, 30.0]
values[0] = 50.0
```

Arrays declared with `const` cannot be changed:

```agi
const values: array = [10.0, 20.0, 30.0]
```

Attempting indexed assignment through a constant produces a compiler diagnostic.

## Appending values

Use `append`:

```agi
let values: array = [1.0, 2.0]
append(values, 3.0)
```

Method-call syntax is also recognized by the language frontend:

```agi
values.append(3.0)
```

## Array length

```agi
let count: i64 = len(values)
```

## Iteration

```agi
fn sum_array(values: array) -> f64:
    let total: f64 = 0.0

    for value in values:
        total += value

    return total
```

## Passing arrays to functions

```agi
fn first_value(values: array) -> f64:
    return values[0]

fn main() -> i32:
    let values: array = [8.0, 9.0, 10.0]
    let first: f64 = first_value(values)

    if first == 8.0:
        return 0

    return 1
```

## Nested arrays

The language type system recognizes nested array types internally. Broader nested-array execution and differential fixtures remain under implementation.

Target syntax:

```agi
let matrix = [
    [1.0, 2.0],
    [3.0, 4.0]
]
```

## Typed generic arrays

The type system recognizes canonical forms such as:

```text
array<f64>
array<i64>
array<array<f64>>
```

The current parser-facing stable annotation is:

```agi
let values: array = [1.0, 2.0, 3.0]
```

Direct `array<T>` source annotation is the next parser slice. Until that lands, element types are inferred from literals and assignments.

## Current supported element scope

The complete execution path is currently strongest for homogeneous numeric arrays. Expansion to fully specialized arrays of strings, booleans, structures, and generic user-defined types is tracked separately.

## Complete example

See:

```text
examples/native/arrays.agi
```
