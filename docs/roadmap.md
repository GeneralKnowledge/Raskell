# Roadmap

## Done (recent)

- [x] Broader `while` / stateful loop recognition → tail-recursive `go`
- [x] More iterator adapters (`zip`, `chain`, `enumerate`, `take`, `skip`, `flat_map`, …)
- [x] Generics on functions and data types
- [x] Traits → type classes (+ basic `impl` methods / inherent methods)
- [x] Error propagation (`?`) → `Either` / `Maybe` do-notation
- [x] Everyday stdlib / string / Option / Result method mappings
- [x] Rust-developer workflow guide (`docs/rust-developer-guide.md`)
- [x] Richer `explain` output (per-function notes + full generated module)

## Near term

- [ ] Explicit `String` builders → efficient Haskell equivalents
- [ ] Smarter `println!`/`format!` for `String` vs `Show` values (avoid quoted strings)
- [ ] Broader inherent/`impl` method coverage and associated functions
- [ ] Clearer diagnostics for “almost supported” mutation patterns

## Medium term

- [ ] Async subset → `IO` / async libraries
- [ ] Property-based differential testing
- [ ] Richer ownership / borrow-aware transforms
- [ ] Comment preservation

## Long term

- [ ] Channels → STM
- [ ] Source maps
- [ ] Incremental compilation & LSP
