# Raskell semantic stress coverage

## Headline (regenerate with `cargo test --test rusty_corpus -- --nocapture`)

See latest run output and `reports/corpus_results.json`.

## What this pass proved

### Semantic convergence

| Style | Example | Shared meaning |
|-------|---------|----------------|
| `if` + `push` | `convergence/01_*` | filter + map |
| temp + `push` | `convergence/02_*` | filter + map (temps inlined) |
| `continue` + `push` | `convergence/03_*` | filter + map |
| iterator chain | `convergence/04_*` | filter + map |
| guarded `+= f(x)` | `convergence/05–07_*` | sum ∘ map ∘ filter |
| iterator sum | `convergence/08_*` | sum ∘ map ∘ filter |

### Composable analysis (`loop_analysis.rs`)

Rather than one AST template per feature:

1. Collect loop-local immutable `let`s (including interleaved) and substitute
2. Classify actions: skip / push / accumulate / early return
3. Match computation shapes

This fixed a **false success** where `let doubled = …` produced Haskell mentioning unbound `doubled`.

### Ugly but legitimate Rust

`ugly/01_average_skip_neg.rs` — continue + temp + two accumulators + trailing average → `foldl` over a pair + `div`.

### Ownership

`ownership/07_slice_sum.rs`, `08_first_positive_borrow.rs`, `09_into_iter_map.rs` — borrows/`into_iter` erased when behaviour is clear.

## Unsupported (deliberately)

See `tests/rusty/unsupported/` — classified D (or C for concurrency/async).

## How to re-run

```bash
cargo test --test rusty_corpus -- --nocapture
cargo test --test rusty_diff
./target/release/raskell explain tests/rusty/convergence/06_fold_temp.rs
```
