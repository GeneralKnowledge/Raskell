# Roadmap

Updated from the **semantic stress** work (`tests/rusty/`, especially `convergence/` and `ugly/`) — not speculation.

## Established

- [x] Iterator and imperative map/filter/sum converge on the same IR shapes
- [x] Loop-local immutable `let`s are substituted before recognition (no unbound temps)
- [x] `if p { … }` and `if !p { continue }; …` unify as the same filter
- [x] Filtered folds with mapped addends (`total += f(x)` under a guard)
- [x] Early `return Some(x)` in a for-loop → `find`
- [x] Filtered multi-accumulators with trailing expressions (e.g. average)
- [x] Record updates, Euclidean while, counting while, indexed search, order scans
- [x] Ownership/borrow erasure for ordinary `&Vec` / slices / `into_iter`
- [x] Stress corpus + differential harness + coverage report

## Pattern recognition direction

Move from “first stmt must be X, second must be Y” toward:

1. Normalise (setup lets, loop temps, skip/continue)
2. Classify actions (push / accumulate / early return / …)
3. Recognise computation (filter+map, fold, find, state machine)

`src/translate/loop_analysis.rs` is the start of that composable layer.

## Next (evidence-driven)

- [ ] Nested loops with independent accumulators
- [ ] Three+ accumulator state without special-casing pairs
- [ ] Manual `partition` (two Vec pushes)
- [ ] Richer state machines than simple enum match
- [ ] Empty-list edge for adjacent scans (`values[0]` panics in Rust)

## Near term (quality)

- [ ] Broader property-based differential suites
- [ ] Smarter `format!` / `String` builders
- [ ] Normalise map-then-filter vs filter-then-map when algebraically equivalent

## Medium / long term

- [ ] Async subset → `IO` / async libraries *(infrastructure)*
- [ ] Channels → STM *(infrastructure)*
- [ ] Associated types / richer traits where type classes suffice
- [ ] Incremental compilation & LSP

## Explicitly out of current model (D)

- `unsafe`, raw FFI, arbitrary macros
- Interior mutability (`RefCell`/`Cell`), `Rc`/`Arc` without clear erasure
- Threads / OS concurrency (until STM/async work lands)
- Trait objects (`dyn Trait`)
- Closures that capture mutable state
