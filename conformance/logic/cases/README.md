<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The conformance cases root

`conformance/logic/cases/` is the single root of the native logic-conformance
corpus. Discovery is depth- and category-agnostic
(`crates/conformance/src/discover.rs`): any directory holding a `profile.json`
(and, for logic cases, an `input.logic.ttl`) is a case, so every subtree below
auto-registers in the test harness and the release report.

The subtrees fall into two kinds:

* **Endogenous families** (`characteristic/`, `cognition/`, `foundation/`,
  `worlds-*/`, … — most of this tree). Their goldens are what the engine itself
  produced, so they grade **stability**: the same input must keep producing the
  same output.
* **Vendored families** (`external/`, `bench/`). Their fixtures come from
  *third parties*, so they grade **soundness** (`external/`) or **performance**
  (`bench/`) against independent ground truth.

## The one vendored-corpus contract

Both vendored families share ONE admission contract, defined once in
`crates/conformance/src/vendored.rs` and rooted here (see
`paths::vendored_corpus_root`):

* Each vendored corpus carries a `corpus.json` (the `CorpusMeta` schema:
  `name`, `spdx_license`, `source_url`, `version_or_commit`, `refresh_command`,
  `lane`). Parsing is manual and hard-fail — a missing field, an unknown key, or
  an unknown lane is an error, never a silent default.
* The declared SPDX license is audited by `audit_vendorable` against the native
  `gmeow_license` policy BEFORE the corpus is committed. Only an `IMPORT_OK`
  license may live under this root; a `REFERENCE_ONLY` (or unknown) license is a
  hard error and may only be fetched live in the heavy Lane-B lane, never
  committed. Every vendored corpus also earns an entry in the repository-root
  `NOTICE`.
* The `lane` field routes grading: `a` (the fast required native gate), `b` (the
  heavy oracle lane), or `divergence` (the honest capability-gap quarantine whose
  committed verdict is the frozen NATIVE verdict).

Same schema, same license gate, different grading consumer. The contract lives in
a domain-neutral module precisely so the two families share it as one root by
design rather than by two constants happening to agree.

## The two vendored families

* [`external/`](external/README.md) — third-party **correctness** suites (TPTP
  SZS problems, W3C `mf:`/`otest:`/`test:` entailment manifests, FAIR
  OntoUML/UFO models) graded against their published verdicts by
  `stage-conformance`, which feeds the committed `generated/agreement-matrix.md`.
* [`bench/`](bench/README.md) — engine-vs-engine **performance** corpora consumed
  by the `gmeow-bench-engines` harness.
