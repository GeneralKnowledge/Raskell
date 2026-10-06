# Roadmap

Updated from the **Rusty Code Stress Test** corpus (`tests/rusty/`, `reports/coverage.md`) — not speculation.

## Established by stress testing

- [x] Iterator chains and imperative map/filter/sum converge on the same IR shapes
- [x] Conditional `for` + `push` → `filter` + `map`
- [x] Mapped reductions (`total += f(x)`) → `sum (map f xs)`
- [x] Record field mutation / `&mut` updates → Haskell record update
- [x] Early-return chains → nested `if/else`
- [x] Continue/break accumulator folds → list `go`
- [x] Imperative max scan → `maximum`
- [x] Pair destructuring in `let` / closures → `fst`/`snd`
- [x] Indexed search loops → `elemIndex`
- [x] Adjacent order scans (`is_sorted`) → `and` / `zipWith`
- [x] Euclidean `while` (swap + remainder) → multi-arg `go`
- [x] Counting `while i < n` (order-independent mut lets) → `go`
- [x] Reject interior mutability / Rc / threads / trait objects cleanly
- [x] Stress corpus + coverage report + differential harness

## Next semantic patterns (not yet forced by corpus failures)

Corpus currently has **0 missed opportunities**. Next candidates come from expanding the corpus, not speculation about syntax:

- [ ] Multi-accumulator folds beyond pairs (three+ state variables)
- [ ] Manual `partition` / two-collection push loops
- [ ] Indexed `enumerate`-style mutations that are not plain `elemIndex`
- [ ] State-machine enums with richer transition tables

## Near term (quality)

- [ ] Smarter `println!`/`format!` for `String` vs `Show` (avoid quoted strings)
- [ ] Explicit `String` builders → efficient Haskell
- [ ] Broader property-based differential suites on pure corpus functions
- [ ] Empty-list edge for adjacent scans (Rust panics on `values[0]`; Haskell `and []` is `True`)

## Medium term

- [ ] Async subset → `IO` / async libraries *(Haskell has a representation; needs infrastructure)*
- [ ] Channels → STM *(same)*
- [ ] Comment preservation / source maps

## Long term

- [ ] Incremental compilation & LSP
- [ ] Broader trait / associated-type story where type classes suffice

## Explicitly out of current model (reject, do not fake)

- `unsafe`, raw FFI, arbitrary macros
- Interior mutability (`RefCell`/`Cell`), `Rc`/`Arc` without a clear erasure
- Threads / OS concurrency (until STM/async work lands)
- Trait objects (`dyn Trait`) — needs existentials
- Closures that capture mutable state
