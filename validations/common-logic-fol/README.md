<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Common Logic / first-order cross-check lane

Cross-checks gmeow's Common Logic (CLIF) export against an **external, general first-order
theorem prover** — [E](https://eprover.org/) — expressing the `native ⊇ oracle` invariant as a
falsifiable PASS/BOUNDARY contract: whatever native says is consistent must not be refutable by
E, and whatever native derives must be independently confirmable by E.

The **structural** and **round-trip** halves of the CLIF dialect (CLIF ⇄ `LogicProgram` IR,
`PreservationKind::Exact`) are already proven inside the repo's `make check` gate (the
`crates/logic-compile/src/clif` unit tests and the `crates/logic/tests/cl_ingest.rs` in-gate
ingest-and-reason proof). This lane is the **external, empirical** confirmation — driving a real
first-order ATP outside GMEOW's own reasoning stack — which needs Docker (or a local `eprover`
install), so it lives here, run on demand, never in CI or `make check` / `maint-*`.

## What it proves

Two checks, both against `eprover`:

- **Check A — foundation consistency.** The full CLIF export of the `logic:` foundation
  (`generated/cl/gmeow.clif`) is translated to TPTP-FOF and handed to E for saturation (no
  conjecture). E must not report `Unsatisfiable` / `ContradictoryAxioms` / `Theorem` — any of
  those would mean the exported foundation's FOL translation is inconsistent, directly
  contradicting native's own `consistent` verdict over the same program. `Satisfiable` /
  `CounterSatisfiable` is the expected outcome for a program that is (mostly) a Horn/rule set;
  `Unknown` / `GaveUp` / `ResourceOut` / `Timeout` is treated as prover incompleteness (a budget
  limit, not a divergence) and is also OK.
- **Check B — ingest entailment (the load-bearing check).** The externally-authored sample
  genealogy KB (`conformance/logic/cl-ingest/sample-kb.clif` + its EDB,
  `sample-kb.edb.nq`) is translated to TPTP-FOF with the conjecture
  `ancestor(alice, carol)` appended, and handed to E. E is expected to report `Theorem` —
  independently confirming, via a wholly separate first-order engine, the exact transitive
  `ancestor` entailment the native engine derives in-gate
  (`crates/logic/tests/cl_ingest.rs::sample_kb_clif_ingest_and_reason_derives_ancestor_closure`).
  `CounterSatisfiable` / `Satisfiable` here is a hard **BOUNDARY**: E would be refuting an
  entailment native asserts, a real divergence. `Unknown` / timeout is noted but not
  hard-BOUNDARYed (this conjecture is small enough to prove near-instantly; a timeout here would
  itself be surprising, worth investigating, but is not by itself proof of unsoundness).

## How the translation works

`clif2tptp/` is a lane-local Rust binary that **reuses** the real CLIF reader —
`gmeow_logic_compile::clif::parse_clif_str` — to reconstruct the `LogicProgram` IR from a
gmeow-dialect CLIF file's `;; @@gmeow-rdf-meta@@` channel (never a hand-rolled CLIF parser), then
renders that IR to TPTP-FOF: IRIs become single-quoted TPTP constants, literals become distinct
`'lit|<lexical>|<datatype>'` constants (kept out of the IRI value space), and `?var`s become TPTP
variables. `Formula` nodes (the full-FOL fragment beyond Horn+NAF) translate structurally
(`And`/`Or`/`Not`/`Implies`/`Iff`/`Forall`/`Exists`). A Common Logic sequence marker
(`Term::SequenceMarker`) is a hard, named BOUNDARY — it is not FOF-expressible — and the
translator exits non-zero rather than silently dropping it. The generic (predicate, subject,
object) relational-core shape is all the translator needs; the foundation CLIF carries the
`logic:` program (Horn rules + full-FOL formulas), not a DL TBox, so no DL-specific encoding
(e.g. unary/binary predicate distinctions beyond arity) is required.

```text
clif2tptp <clif-file> [--edb <nquads-file>] [--conjecture <pred-iri> <subj-iri> <obj-iri>]
```

## Prerequisites

- A Rust toolchain (to build `clif2tptp` — it is deliberately detached from the main cargo
  workspace via an empty `[workspace]` table, so it builds standalone).
- Docker, to build the `eprover`-carrying image — **or** a locally installed `eprover` on
  `PATH` (the probe uses it directly if present, skipping Docker).

```bash
make -C validations/common-logic-fol validate
```

`validate` builds the `cl-fol-eprover` Docker image (`make build-image`) then runs `probe.sh`,
which builds `clif2tptp`, runs both checks, and writes the outcome to `result.txt`.

## Output contract

The probe prints, and writes to `result.txt`, exactly one of:

- `PASS foundation FOL-consistent + sample-KB ancestor entailment confirmed by E prover`
- `BOUNDARY <check>: <SZS token / reason>` — naming the exact failing check and the E prover SZS
  status (or translator boundary) responsible.

## Honest caveat

E is a **general first-order automated theorem prover**, not a native Common Logic reasoner —
it knows nothing of CLIF, RDF, or the `logic:` foundation's own semantics. The CL-conformance
claim this lane supports rests entirely on the **semantics-preserving CLIF → TPTP-FOF
translation** of the IR that `parse_clif_str` reconstructs — it is NOT native CLIF parsing by an
external tool, because no mature, freely available native CL/CLIF reasoner exists to drive
directly. E is used because it is a well-tested, independent implementation of first-order
semantics, which is exactly the cross-check surface needed: a divergence here would mean either
the CLIF export or the `clif2tptp` translation encodes something semantically different from
what native computed. The end-state (SOTA) this lane is a stepping stone toward is a **native CL
reasoner operating directly over the full-FOL IR** (`gmeow_logic_compile::ir::Formula`), closing
even this translation-layer gap; see `docs/APPLIED_CATEGORY_THEORY/` for the current status of
that native-engine direction. Check A is a satisfiability sanity check (a Horn/rule program being
satisfiable is the unsurprising, expected case); **Check B is the load-bearing empirical
confirmation** — it is the one place this lane can actually falsify a native ⊇ oracle claim.

This lane is **deliberately outside** `make check`, the `maint-*` maintainer lanes, and CI — see
`validations/README.md`.
