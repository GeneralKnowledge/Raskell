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

Realistic Rust programs live under `tests/rusty/`. Coverage numbers and missed
patterns are tracked in `reports/coverage.md`. The corpus is part of the
compiler’s definition of “what Raskell understands.”
