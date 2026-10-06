# Architecture overview

Raskell is organised as a classic multi-stage compiler:

1. **Parser** (`src/parser`) — `syn` parses Rust; we lower into a Raskell AST.
2. **Semantic** (`src/semantic`) — name collection, light checks, unsupported-construct scanning.
3. **Translate** (`src/translate`) — pattern recognition + lowering to a semantic IR.
4. **Haskell** (`src/haskell`) — IR → Haskell AST → pretty-printed `.hs`.
5. **CLI** (`src/cli`) — `check`, `translate`, `explain`.

The IR (`src/ir`) deliberately does **not** mirror Rust syntax. Multiple Rust forms
(e.g. `for`+`push` and `.map().collect()`) can lower to the same IR node so the
Haskell backend optimises them consistently.

## Intended user

Primary audience: a **Rust developer who needs Haskell output** and should not have to
author Haskell by hand. The CLI (`check` → `explain` → `translate`) and
[rust-developer-guide.md](./rust-developer-guide.md) are part of the product surface,
not afterthought docs.

## Key lowering policies

- Erase ownership/borrowing when semantics are unambiguous.
- Rewrite recognised mutation into pure arithmetic / HOFs / `go` recursion.
- Map `Option`/`Result`/`?` onto `Maybe`/`Either`/do-notation.
- Map traits/`impl` onto `class`/`instance` with constrained signatures.
- Prefer Prelude/`Data.Maybe`/`Data.Either` over a custom runtime.
- Prefer **semantic convergence**: iterator style and imperative style that mean the same thing should lower to equivalent IR (see `tests/rusty/mixed/03_imperative_vs_iter.rs`).

## Stress corpus

Realistic Rust programs live under `tests/rusty/` (including `convergence/`
variants that attack pattern brittleness, and `ugly/` for legitimate messy
code). Coverage is tracked in `reports/coverage.md`.

## Composable loop analysis

`src/translate/loop_analysis.rs` normalises for-loop bodies before recognition:

1. Substitute loop-local immutable `let`s (temps)
2. Unify `if p { action }` with `if !p { continue }; action`
3. Classify actions, then recognise computations (filter+map, filtered fold, find, …)

Detectors in `patterns.rs` increasingly delegate to this layer instead of
encoding “stmt[0] must be …, stmt[1] must be …” templates.
