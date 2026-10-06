# Raskell stress-corpus coverage report

Generated from `tests/rusty/**/*.rs` after the Rusty Code Stress Test pass.

## Headline numbers

| Metric | Count |
|--------|------:|
| Rust constructs / programs tested | **78** |
| Successfully translated (GHC `-c` ok) | **68** |
| Rejected correctly (`unsupported/`) | **10** |
| Incorrect translations | **0** |
| Missed opportunities (reasonable Rust, not yet translated) | **0** |

Success rate on non-unsupported corpus: **68 / 68 = 100%**.

## Corpus layout

```text
tests/rusty/
  basic/          arithmetic, helpers, recursion, tuples
  collections/    iterators + imperative map/filter/sum
  ownership/      moves, borrows, record updates
  iterators/      enumerate, flat_map, find, …
  control_flow/   if/match/while/early-return/continue-break
  data_types/     structs, enums, nested data
  errors/         Option/Result/?/and_then
  traits/         type classes + generics
  strings/        everyday string methods
  algorithms/     max, search, fib, gcd, sorted
  state/          counters, state machines, multi-accum
  mixed/          multi-concept programs + convergence pairs
  unsupported/    deliberately rejected constructs
```

## Incorrect translations

None remaining in this corpus. Earlier incorrect cases (module names starting with digits, `String::from` → `id`, IO mis-detection of `(>>=)`, tuple closure arity, discarded field assigns, while-counter rename colliding with parameters) were fixed as **general** bugs.

## New semantic patterns taught this pass

1. **Conditional collection build** (`for` + `if` + `push`) → `map` ∘ `filter`
2. **Mapped reduction** (`total += f(x)`) → `sum (map f xs)`
3. **Multi-accumulator loop** → `foldl` over a tuple
4. **Record field mutation** → Haskell record update `{ field = … }`
5. **Unit `&mut` updates** → promoted to functions returning the updated record
6. **Early-return chains** → nested `if/else`
7. **Imperative max scan** → `maximum`
8. **Continue/break fold** → tail-recursive `go` over a list
9. **Pair `let` / `|(a,b)|` destructuring** → `fst`/`snd` bindings
10. **Module names** stripped of leading digits for GHC
11. **Setup lets** before imperative patterns skipped then re-wrapped
12. **Indexed linear search** (`i` + `for` + early `Some(i)`) → `elemIndex`
13. **Adjacent order scan** (`prev` + early `false`) → `and (zipWith …)`
14. **Euclidean while** (temp + `%` swap) → recursive `go`
15. **Counting while** (acc-first mutable lets) → `go cnt acc` (order-independent)

## Semantic convergence (validated)

| Imperative Rust | Iterator Rust | Shared Haskell shape |
|-----------------|---------------|----------------------|
| `for` + `if` + `push` | `.filter().map().collect()` | `map f (filter p xs)` |
| `for` + `total += x` | `.sum()` | `sum xs` |

See `tests/rusty_corpus.rs` (`semantic_convergence_*`) and `tests/rusty/mixed/03_imperative_vs_iter.rs`.

## Unsupported corpus (rejected correctly)

`unsafe`, `async`, `extern`, unknown macros, `RefCell`, `Rc`, `thread::spawn`, trait objects (`dyn`), mutably capturing closures, associated types.

These are category D (fundamentally unsuitable for the current model) or C (needs significant infrastructure — e.g. concurrency → STM/async later).

## Differential coverage

`tests/rusty_diff.rs` compares Rust vs Haskell executables for:

- filter/map loop
- continue/break fold
- record rename
- early return
- imperative max
- sum of squares
- Euclidean gcd
- indexed search
- is_sorted
- counting while
- property-style filter+sum over multiple inputs

## How to re-run

```bash
cargo test --test rusty_corpus -- --nocapture
cargo test --test rusty_diff
cargo build --release
./target/release/raskell translate tests/rusty/collections/04_for_filter_push.rs
./target/release/raskell explain tests/rusty/control_flow/04_continue_break.rs
```
