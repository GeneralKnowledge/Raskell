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
