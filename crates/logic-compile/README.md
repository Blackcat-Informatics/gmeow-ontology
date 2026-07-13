<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-logic-compile

The pure, **wasm-able** GMEOW logic compiler.

This crate is the parse-and-project half of the logic stack:

```text
RDF 1.2 text ──parse──▶ LogicProgram (IR) ──project──▶ the seven committed artifacts
                                                        (OWL DL, OWL EL, Datalog, N3,
                                                         gUFO, canonical RDF-1.2,
                                                         projection report)
```

It carries **no reasoning-runtime dependencies**. The RDF parse/serialize path rides the
wasm-clean `gmeow-rdf` `gts`
surface (oxigraph-free, the same surface `crates/rdf-wasm` uses), so the entire
compiler builds for `wasm32-unknown-unknown`. `make wasm` gates this and asserts
the dependency tree stays free of the runtime crates.

The reasoning **runtime** — world-indexed stores, native forward/backward evaluation,
certification, and counterfactuals — lives in the sibling `gmeow-logic` crate, which
depends on this one. Two pieces stay runtime-side by design: `lower.rs`
(compiler-IR → runtime `EvalRule`) and
`diagnostics_report` (returns a PyO3-tainted `gmeow_diagnostics::Report`).
