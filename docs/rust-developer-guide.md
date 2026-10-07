# Raskell for Rust developers

You know Rust. You need Haskell. You do **not** need to learn Haskell first.

Raskell lets you write a supported Rust subset and emit idiomatic, GHC-compilable Haskell. Think of it as: *express the algorithm in Rust, ship the Haskell*.

## Recommended workflow

```bash
# 1. Write ordinary Rust in the supported subset (see below)
$EDITOR algo.rs

# 2. Check Raskell understands it (fail fast on unsupported constructs)
raskell check algo.rs

# 3. See what it will become — and *why*
raskell explain algo.rs

# 4. Emit Haskell
raskell translate algo.rs -o Algo.hs

# 5. Compile / run with GHC (no Cabal required for simple modules)
ghc -c Algo.hs          # typecheck + compile object
# or, if you have a main:
runghc Algo.hs
```

If `check` or `translate` errors with `E0421`, that construct is intentionally unsupported — rewrite in a supported pattern rather than fighting the diagnostic.

## Mental model (Rust → Haskell)

| You write (Rust) | You get (Haskell) |
|------------------|-------------------|
| `Option<T>` | `Maybe a` |
| `Result<T, E>` | `Either e a` (error on the left) |
| `Vec<T>` / slices | `[a]` |
| `String` | `String` (`[Char]`) |
| `struct` / `enum` | `data` ADT |
| `trait` + `impl` | `class` + `instance` |
| `fn foo<T: Trait>` | `foo :: Trait t => …` |
| `x?` in a fallible fn | `do` / `<-` binds |
| `let mut` + loop accumulators | `sum` / `filter` / `go` helper |
| `.iter().map().filter().collect()` | `map` / `filter` / Prelude HOFs |
| `println!` / `print!` | `putStrLn` / `putStr` |
| `main` | `main :: IO ()` |

Ownership, borrowing, and lifetimes are **erased** when the meaning is clear. Mutation is rewritten into pure updates or higher-order functions when Raskell recognises the pattern.

## Write ordinary Rust

You do **not** need to memorise Raskell-approved AST shapes. These are equivalent for Raskell:

```rust
// if + push
for value in values {
    if value > 0 { result.push(value * 2); }
}

// temporary
for value in values {
    let doubled = value * 2;
    if doubled > 0 { result.push(doubled); }
}

// continue
for value in values {
    if value <= 0 { continue; }
    result.push(value * 2);
}

// iterators
values.iter().map(|x| *x * 2).filter(|x| *x > 0).copied().collect()
```

All lower toward `map` / `filter`. Loop-local `let`s and `continue` are normalised before recognition.

**Prefer**

- Clear functions with explicit types on parameters / returns
- `Option` / `Result` instead of panics for expected failure
- Iterator chains **or** imperative loops with obvious intent (Raskell converges them)
- Traits for shared behaviour (they become type classes)
- `?` inside a function that already returns `Result` / `Option`
- Ordinary `&` / `&mut` / moves when behaviour is clear (ownership is erased)

**Avoid (rejected or poorly supported)**

- `unsafe`, raw pointers, `extern "C"`
- `async` / `await`
- Arbitrary macros (only `vec!`, `println!`, `print!`, `format!`)
- Clever interior mutability (`RefCell`, mutexes, atomics)
- Patterns Raskell cannot prove equivalent — it will refuse rather than guess

Stress corpus of realistic programs: `tests/rusty/` (see `reports/coverage.md`).

## Mini cookbook

### Fallible parsing with `?`

```rust
fn parse_i32(s: String) -> Result<i32, String> {
    if s.is_empty() { Err(String::from("empty")) } else { Ok(s.len() as i32) }
}

fn add_parsed(a: String, b: String) -> Result<i32, String> {
    let x = parse_i32(a)?;
    let y = parse_i32(b)?;
    Ok(x + y)
}
```

→ Haskell `do` notation with `Either String Int`.

### Traits → type classes

```rust
trait Greet { fn greet(&self) -> String; }
impl Greet for Person { fn greet(&self) -> String { self.name.clone() } }
fn say_hello<T: Greet>(x: T) -> String { x.greet() }
```

→ `class Greet a`, `instance Greet Person`, constrained `sayHello`.

### While loops

```rust
fn triangle(n: i32) -> i32 {
    let mut i = n;
    let mut total = 0;
    while i > 0 { total += i; i -= 1; }
    total
}
```

→ tail-recursive `go` helper (no mutable variables in the Haskell).

### Common stdlib methods that map cleanly

| Rust | Haskell |
|------|---------|
| `v.len()` / `s.len()` | `length` |
| `v.is_empty()` | `null` |
| `opt.is_some()` / `is_none()` | `isJust` / `isNothing` |
| `res.is_ok()` / `is_err()` | `isRight` / `isLeft` |
| `opt.unwrap_or(d)` | `fromMaybe d` |
| `s.contains(p)` | `isInfixOf` |
| `s.starts_with` / `ends_with` | `isPrefixOf` / `isSuffixOf` |
| `s.lines()` | `lines` |
| `s.push_str(t)` (as expr) | `s ++ t` |
| `n.abs()` | `abs` |
| `String::from(x)` | `x` (erased) |

## Using `explain` when you don't know Haskell

`raskell explain file.rs` prints, per function:

1. **Detected** — what computation Raskell saw (traversal, filtering, accumulation, …)
2. **Semantic form** — the meaning-level rewrite (`filter + map`, `sum`, `find`, …)
3. **Haskell strategy** — the generated function body
4. **Full module** — the complete `.hs` you can paste into GHC

You do not need to invent Haskell idioms yourself. Write the Rust you would write for a small library function, run `explain`, and keep the output when it matches your intent.

## End-to-end checklist for a new Haskell module

1. Sketch the API in Rust (`fn` signatures first).
2. Implement with `Option`/`Result`, iterators, and simple loops.
3. `raskell check` until clean.
4. `raskell explain` — confirm rewrites look right (accumulators, `?`, traits).
5. `raskell translate -o Module.hs`.
6. `ghc -c Module.hs` (or `runghc` if you have `main`).
7. Commit both the Rust source (as the editable form) and the `.hs` if your project wants generated artefacts.

## Examples to start from

| Example | What a Rust dev learns |
|---------|------------------------|
| `examples/mutation.rs` | How mutation becomes pure |
| `examples/iterators.rs` / `more_iterators.rs` | Iterator → Prelude |
| `examples/option.rs` / `result.rs` | Maybe / Either |
| `examples/try_operator.rs` | `?` → do-notation |
| `examples/traits.rs` / `generics.rs` | Type classes & polymorphism |
| `examples/while_loop.rs` | Stateful loops → `go` |
| `examples/stdlib_helpers.rs` | Everyday method mappings |
| `examples/io.rs` | `println!` → `IO` |

## When something fails

- **Diagnostic `E0421`**: unsupported construct — rewrite or wait for the [roadmap](./roadmap.md).
- **GHC type error on output**: open an issue with the Rust input and the `.hs`; that is a Raskell bug, not your Haskell knowledge gap.
- **Behaviour differs from Rust**: prefer differential tests (`cargo test -p raskell --test differential`) patterns; keep examples small and deterministic.
