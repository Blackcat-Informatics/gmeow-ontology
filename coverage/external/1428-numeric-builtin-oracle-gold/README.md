<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# per-op numeric-builtin oracle gold (issue #1428)

A frozen, **independently hand-derived** conformance corpus anchoring issue
#1428's acceptance criterion: "Each op separately anchored to an INDEPENDENT
oracle." It covers the native evaluator's exact-ℚ arithmetic
(`+ - * / //`), comparison (`> < >= =< =:=`), SI-dimension commensurability
and composition (`gmeow_math::dimension::DimVector`), and dimensioned-quantity
arithmetic — the numeric builtins implemented in
`crates/logic/src/physical/builtin_eval.rs`.

## Why this exists, and why it is NOT the same as the in-crate tests

`crates/logic/src/physical/builtin_eval.rs` carries `#[test]`s that call its
own `eval()` function directly and assert on `eval()`'s own output — a
tautology (the function is checked against itself). This corpus is the
INDEPENDENT counterpart: every `expected` value below was computed BY HAND
(see each case's `provenance.derivation`) from the mathematical definition of
the operation, never by running the engine and copying its answer. The
consuming gate (`crates/conformance/tests/numeric_builtin_oracle_gold.rs`)
then drives each case through the PUBLIC production query surface
(`gmeow_logic::dispatch::dispatch_query`, the same entry point
`crates/logic/src/dispatch.rs`'s own end-to-end tests and the conformance
harness use) — never the crate-private `eval()` — and asserts the native
engine's answer matches the independently-derived gold.

This corpus lives under `coverage/external/`, OUTSIDE
`conformance/logic/cases/`, specifically so it can **never** be reached by
`crates/conformance/src/bless.rs` (`GMEOW_CONFORMANCE_BLESS`), which only
sweeps `conformance/logic/cases/*/expected/` and re-writes `expected/` from
the engine's own live output. A corpus the bless sweep can reach is not an
independent oracle; this one is architecturally unreachable by it.

## Layout

| path | role |
| --- | --- |
| `cases/*.json` | one file per op instance: the program, any EDB facts, the hand-derived expected result, and its provenance |

Each case JSON has:

* `op` — the op instance name (e.g. `rational_add`, `dimension_compose_mul`).
* `mode` — `"value"` (an `is` generator whose bound variable(s) are checked),
  `"compare"` (a filter whose pass/fail is checked via the resulting answer
  count), or `"gap"` (a domain error that must refuse the whole query).
* `facts` — optional EDB quads to load into the `WorldStore` before running
  the program (used only by the dimension/quantity cases — see below).
* `program` — a minimal `.logic` query program (the same Prolog-ish surface
  `gmeow_logic::query_ir::parse_query_program` parses in production) that
  drives the op via `X is Y op Z` / `L cmp R` builtins.
* `checks` (mode `value`/`compare`) — `{var, value_kind, expected}` triples
  naming which goal variable to inspect, how to interpret its bound surface
  (`integer` / `rational` / `dimension` / `quantity` / `const`), and the
  hand-derived expected lexical form.
* `expect_bindings` (mode `compare`) — the expected answer count (`1` when
  the comparison holds, `0` when it does not).
* `expected_gap_kind` (mode `gap`) — the `math:` failure class we
  independently derived the program should hit (`ZeroDivisor` / `Overflow` /
  `DimensionMismatch`) — see the honesty caveat below.
* `provenance.derivation` — the worked-by-hand arithmetic/vector steps.
* `provenance.note` — the independence declaration (and, for gap cases, the
  observability caveat).

## Construct families covered

| case | family |
| --- | --- |
| `rational_add.json` | exact-ℚ `+` |
| `rational_sub.json` | exact-ℚ `-` |
| `rational_mul.json` | exact-ℚ `*` |
| `rational_exact_div_normalize.json` | exact-ℚ `/`, gcd normalization (2/4 -> 1/2) |
| `rational_exact_div.json` | exact-ℚ `/`, ℚ-path pinned even for a whole-number quotient (6/2 -> 3/1, NOT the integer 3) |
| `truncating_vs_exact_div.json` | `//` (truncating ℤ) vs `/` (exact ℚ) over the SAME operands, contrasted |
| `rational_compare_lt.json` | `<` (holds) |
| `rational_compare_ge_false.json` | `>=` (fails) |
| `dimension_commensurable_eq.json` | `=:=` over two EQUAL ℚ⁷ dimension vectors (length vs length) |
| `dimension_commensurable_neq.json` | `=:=` over two UNEQUAL ℚ⁷ dimension vectors (length vs mass) |
| `dimension_compose_mul.json` | dimension `*` (dimProduct: exponent-vector addition; length composed with inverse-time -> velocity) |
| `dimension_compose_div.json` | dimension `/` (exponent-vector subtraction; area over length -> length) |
| `quantity_add.json` | dimensioned-quantity `+` over a commensurable pair (3 m + 2 m = 5 m) |
| `quantity_add_dimension_mismatch.json` | dimensioned-quantity `+` over an INCOMMENSURABLE pair (3 m + 2 s) — a declared `math:DimensionMismatch` gap |
| `zero_divisor.json` | exact-ℚ `/` by zero — a declared `math:ZeroDivisor` gap |
| `overflow.json` | ℤ `+` at `i64::MAX + 1` — a declared `math:Overflow` gap |

## Discovered constraint: a builtin body requires an arity->=2 goal atom

Driving the corpus surfaced a real routing constraint of the public
`dispatch_query` surface, independent of anything this corpus set out to
test: `crates/logic/src/physical/magic_generic.rs`'s arity-generic backward
core (used whenever the GOAL atom's arity is anything other than 2) refuses
**every** program containing **any** `QBodyLit::Builtin` body literal
unconditionally — "the generic core is positive Datalog only" (its own doc
comment) — regardless of the builtin's operator, operands, or whether it
would actually error. Only the classic BINARY backward core
(`crates/logic/src/physical/magic.rs`, taken when the goal atom is arity 2)
evaluates arithmetic/comparison builtins at all.

Concretely: `ex:ans(D) :- D is 6 / 2.` / `?- ex:ans(D).` (arity 1) is refused
with the SAME `"...does not support Arithmetic..."` message as an actual
zero-divisor or overflow — even though `6 / 2` has a perfectly well-defined
answer — purely because the goal is arity 1. Every case here therefore
carries an unused dummy argument (`X is 0` / `Y is 1`, never inspected by any
`checks` entry) so its head and goal atom are arity >= 2 and the query
genuinely reaches the arithmetic evaluator. This was verified by hand via a
`cargo nextest` bisection before the corpus was finalized (arity-1 variants
of `rational_add`/`zero_divisor`/`overflow`/`dimension_commensurable_eq` all
refused with the identical message regardless of their actual operands, which
is what exposed the routing rule rather than a real domain gap).

`MalformedDimension` is deliberately NOT anchored here: tracing
`parse_value_surface` in `builtin_eval.rs` shows a malformed dimension/quantity
transport literal is discarded via `.ok()` before it ever reaches
`BuiltinOutcome::Error` — the query-evaluator surface can never raise
`BuiltinError::MalformedDimension` from ANY program (only a direct in-crate
unit call to the private `parse_dimension_lex`/`parse_quantity_lex` helpers
can), so there is no independently-drivable case for it.

## Driving dimension / quantity operands through the public surface

There is no query-text literal syntax for a full typed literal (only bare
integers, `Var`s, and double-quoted plain strings — see
`crates/logic/src/query_ir.rs`'s `parse_term`), so a `Value::Dim` /
`Value::Quantity` operand can only enter the evaluator as an EDB fact object
whose RDF literal carries the engine's transport datatype tag
(`urn:gmeow:transport:dimension` / `urn:gmeow:transport:quantity`, documented
in `builtin_eval.rs`'s module doc comment) and lexical form (`n0/d0,...,n6/d6`
seven-way SI exponent vector, or `num/den;n0/d0,...,n6/d6` for a quantity).
This is exactly the pattern `crates/logic/src/dispatch.rs`'s own
`dimension_composition_flows_through_dispatch` end-to-end test already uses
(`purrdf::TermValue::typed_literal` + `WorldStore::insert_quad_terms`), so the
`facts` fixtures here are not a novel shortcut — they reproduce an existing,
already-production-exercised entry point.

## Honesty caveat: gap-kind granularity is NOT observable at the public surface

`gmeow_logic::dispatch::dispatch_query` — the only public entry point that
runs a program — reports every domain gap (zero divisor, overflow, dimension
mismatch, and the unreachable malformed-dimension) as the SAME opaque
`NativeOutcome::Unsupported(UnsupportedKind::Arithmetic)`, which
`dispatch_query` turns into the fixed error message `"native backward engine
does not support Arithmetic; query refused because no fallback engine
remains"`. `UnsupportedKind::Arithmetic` carries no payload anywhere in the
crate (`crates/logic/src/physical/seminaive.rs`,
`crates/logic/src/physical/magic.rs`, `crates/logic/src/physical/magic_generic.rs`,
`crates/logic/src/physical/resolve_fol.rs` all raise the same bare variant) —
the fine-grained `BuiltinError` (`ZeroDivisor` / `Overflow` /
`DimensionMismatch` / `MalformedDimension`) lives and dies inside the
crate-private `eval()` call and is discarded before any public API returns.

The three gap cases here (`zero_divisor.json`, `overflow.json`,
`quantity_add_dimension_mismatch.json`) therefore prove the honest, weaker,
but still real and non-tautological claim the public surface CAN support:
each program is independently derived (by hand) to hit a specific domain
error, and the native engine really does REFUSE it (never silently computes
a wrong number, wraps around, or ignores dimensional incompatibility). Their
`expected_gap_kind` records which `math:` class we derived by hand, for
documentation; the consuming gate cannot verify that specific tag through
`dispatch_query` and does not claim to.

## The offline gate

`crates/conformance/tests/numeric_builtin_oracle_gold.rs` reads every
`cases/*.json`, builds a `gmeow_logic::store::WorldStore` from `facts`, parses
`program` via `gmeow_logic::query_ir::parse_query_program`, and resolves it
via `gmeow_logic::dispatch::dispatch_query` under
`gmeow_logic::profile_gate::PROCEDURAL_PROLOG_PROFILE` (arithmetic/comparison
builtins are gated to this profile) — then asserts the outcome against the
frozen, hand-derived `expected`/`checks`/`expect_bindings`/`expected_gap_kind`.
It needs no Docker, Java, network, or Python, and is deterministic, so it runs
in the default `cargo nextest -p gmeow-conformance` gate.

## Bilinear-distance (G2) gold

Not yet included: the bilinear-form / Gram-matrix distance builtin lands
separately. Its independent gold will be appended to `cases/` once that
builtin exists on the evaluator, following the same hand-derivation
discipline as every case above.
