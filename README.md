# Raskell

**Write Rust. Get Haskell.**

Raskell is a Rust-to-Haskell transpiler. It accepts a deliberately supported subset of Rust and translates programs into idiomatic, compilable Haskell.

Raskell is **not** a Rust compiler and does **not** translate Rust into Rust. Rust is the source language; Haskell is the target language.

The goal is to explore how far ordinary Rust programming patterns can be understood and expressed as good functional Haskell — preserving observable semantics rather than mechanically reproducing ownership, mutation, or allocation strategies.

## Why Raskell exists

Most “Rust ↔ functional” discussions start from feature overlap:

> Which Rust features also exist in Haskell?

Raskell asks a different question:

> Can we understand what this Rust code *means* and express that meaning naturally in Haskell?

That means the translator is allowed — encouraged — to perform semantic transformations.

```rust
fn sum_positive(values: Vec<i32>) -> i32 {
    let mut total = 0;
    for value in values {
        if value > 0 {
            total += value;
        }
    }
    total
}
```

becomes:

```haskell
sumPositive :: [Int] -> Int
sumPositive values = sum (filter (\x -> x > 0) values)
```

## Architecture

```text
Rust source
    │
    ▼
Lexer / Parser  (syn → Raskell AST)
    │
    ▼
Semantic analysis  (names, types, ownership hints)
    │
    ▼
Translation IR  (semantic, not Rust-shaped)
    │
    ▼
Haskell lowering / optimisation
    │
    ▼
Haskell AST → pretty printer → .hs
```

There is **no** Rust code-generation stage. The pipeline is AST-based end to end — not regex rewriting.

## Quick start

```bash
# Build
cargo build --release

# Check a program (analyse without emitting Haskell)
./target/release/raskell check examples/mutation.rs

# Translate to stdout
./target/release/raskell translate examples/mutation.rs

# Translate to a file
./target/release/raskell translate examples/mutation.rs -o Mutation.hs

# Explain translation decisions
./target/release/raskell explain examples/mutation.rs

./target/release/raskell --version
./target/release/raskell --help
```

Requires [GHC](https://www.haskell.org/ghc/) to compile and run generated Haskell (and for compilation / differential tests).

## Examples

See `examples/`:

| Example | Idea |
|---------|------|
| `hello.rs` | `println!` → `putStrLn` |
| `fibonacci.rs` / `factorial.rs` | Recursion |
| `mutation.rs` | Scalar mutation & accumulator → arithmetic / `filter`+`sum` |
| `iterators.rs` | `.iter().filter().map().collect()` → `map` / `filter` |
| `collection_processing.rs` | `for` + `push` → `map` |
| `structs.rs` / `enums.rs` | Records & ADTs |
| `option.rs` / `result.rs` | `Option`→`Maybe`, `Result`→`Either` |
| `pattern_matching.rs` | `match` → `case` |
| `state_machine.rs` | Enum state transitions |
| `io.rs` | Basic IO |

## Supported Rust (current subset)

Growing over time. Currently includes:

- Functions, parameters, return values, recursion
- `let` / `let mut` (with mutation→pure rewrites where recognised)
- Primitive types, tuples, structs, enums
- `Option` / `Result`
- `Vec`, array literals, `vec!`
- Strings, arithmetic, comparisons, booleans
- `if`, `match`, `for` (recognised patterns), closures
- Iterator chains: `iter` / `map` / `filter` / `collect` / `sum` / …
- References / borrowing erased when semantics are clear
- Basic IO via `println!` / `print!`

## Unsupported (rejected with diagnostics)

Unsupported constructs must **not** silently produce wrong Haskell. Examples:

- `unsafe`, raw FFI / `extern`
- Traits / `impl` blocks (roadmap)
- `async` / `await` (roadmap)
- Arbitrary macros (only `vec!`, `println!`, `print!`, `format!`)
- Ambiguous mutation / ownership patterns the analyser cannot prove sound

Example diagnostic:

```text
error[E0421]: unsupported Rust construct: unsafe
 --> example.rs:12:5
  |
12 |     unsafe {
  |     ^^^^^^
  |
  = Raskell cannot currently establish a safe semantic translation
    for this operation.
```

## Semantic transformations

The Haskell backend seeks idiomatic output:

| Rust pattern | Haskell |
|--------------|---------|
| `Option<T>` | `Maybe a` |
| `Result<T, E>` | `Either e a` (note parameter order) |
| `struct` | `data` with record fields |
| `enum` | algebraic data type |
| mutable accumulator + filter | `sum` / `filter` |
| `for` + `push` | `map` |
| scalar `+=` / `*=` chain | nested arithmetic |
| iterator chain | `map` / `filter` / `foldl` / … |

Haskell is **not** restricted to Rust-shaped features. Generated code may use ADTs, higher-order functions, `Maybe`/`Either`, `IO`, and the Prelude freely.

## Runtime philosophy

Avoid a giant Raskell runtime. Prefer standard Haskell (`Maybe`, `Either`, `map`, `filter`, `sum`, lists). Only introduce helpers when there is no reasonable Prelude mapping.

## Tests

```bash
cargo test
```

The suite covers:

- **Parser** — functions, structs, enums, closures, loops, …
- **Semantic** — acceptance & rejection
- **Translation** — idiomatic output (`Option`→`Maybe`, iterators→`map`, …)
- **Compile** — generated Haskell typechecks/compiles with GHC
- **Differential** — Rust and Haskell programs compared on the same inputs
- **Invalid** — unsupported constructs produce clean diagnostics
- **Patterns** — mutation / accumulator recognition

## Contributing

1. Prefer vertical slices: parse → analyse → IR → Haskell → GHC → behavioural test.
2. Every new feature needs parser + translation + (where relevant) GHC and differential tests.
3. **Do not fake support.** If a construct cannot be translated correctly, reject it with a good diagnostic and add it to the roadmap.
4. Correctness beats feature count.

## Roadmap

- Richer ownership / borrow-aware transforms
- Traits → type classes; generics → Haskell polymorphism
- Async → IO / concurrency; channels → STM
- Broader stdlib mapping, property-based & fuzz testing
- Source maps, comment preservation, LSP

## License

MIT
