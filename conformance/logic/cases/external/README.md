<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# External correctness corpora

This tree holds **external standard correctness suites** lowered into the GMEOW
conformance per-case anatomy. They are the *independent ground truth* the
endogenous goldens cannot provide: the native conformance goldens grade the Rust
evaluator against output the engine itself produced (proving **stability**),
whereas these corpora grade it against **third-party** standard suites (proving
**soundness**).

The external-corpus convention, the ingestion adapter, the verdict model, and the
licensing policy are established here; individual corpora (TPTP FOL, W3C OWL 2 /
SHACL, FAIR OntoUML/UFO) are vendored on top of it, and the agreement dashboard
consumes the divergence ledger.

## Layout

```text
cases/external/<corpus>/
  corpus.json                 # vendoring metadata + SPDX license (audited)
  <case>/
    profile.json              # verdict_mode = consistency | materialization
    input.logic.ttl           # consistency: a stub; materialization: the logic program
    input.nq                  # the world-scoped RDF EDB (consistency cases)
    source/                   # the verbatim third-party source (SZS .p / manifest.ttl)
    expected/
      verdicts.json           # the engine verdict (blessed) — the gated contract
      budget.json             # (budget cases only) the governor marker
```

Discovery is depth-agnostic (`crates/conformance/src/discover.rs`): a directory
holding both `input.logic.ttl` and `profile.json` is a case, so this three-level
layout auto-registers in both the test harness (`make conformance`) and the
`conformance-report` release artifact.

## The verdict mapping table

The adapter (`crates/conformance/src/external/`) maps each external outcome onto a
first-class runner verdict status:

| External outcome (TPTP SZS / W3C `mf:`)                         | Runner verdict |
|----------------------------------------------------------------|----------------|
| `Theorem` / `Unsatisfiable` / `ContradictoryAxioms` / `mf:PositiveEntailment` | `inconsistent` |
| `Satisfiable` / `CounterSatisfiable` / `mf:NegativeEntailment` | `consistent`   |
| `Unknown` / `GaveUp` / `Timeout` / `ResourceOut` (budget-tripped) | `incomplete`   |

An entailment `A ⊨ C` is decided as the consistency of `A ∪ ¬C`, so a
`PositiveEntailment` / `Theorem` only manifests as `inconsistent` once the negated
conclusion is folded into the EDB.

The **fine-grained SZS token is preserved as provenance** (`profile.json`'s
`szs_status`) and projected to this 3-bucket verdict only at the gate — so
`ContradictoryAxioms` vs `Unsatisfiable` and `CounterSatisfiable` vs `Satisfiable`
stay distinguishable in the ledger, and are never collapsed at ingest.

Every verdict is **genuinely engine-decided** (zero defer): consistency-mode cases
run the native DL consistency path (`gmeow_logic::reason::reason_all`) and the run
hard-fails if the engine reports an undecided construct gap; the budget branch is a
real governor trip. Nothing is faked or looked up.

## Two-lane model

| | Lane A (this gate) | Lane B (heavy oracle) |
|-|--------------------|------------------------|
| target | `make conformance` (required) | non-required, network/Docker |
| scope | small, deterministic, sub-second, decided natively | full third-party corpora, oracle-backed |
| routing | committed goldens + the soundness gate | the divergence ledger (`crates/logic/src/reason/ledger.rs`) |
| `corpus.json` `lane` | `"a"` | `"b"` |

A corpus too large or outside the native fragment is a Lane-B corpus, fetched live
and never required for normal repo use.

## TPTP (first-order) corpus

`tptp-mini/` is a self-authored, license-clean TPTP FOF/CNF corpus whose problem
bodies are **parsed for real** and decided natively:

1. `crates/conformance/src/external/tptp/parser.rs` parses the FOF/CNF body into the
   full-FOL `Formula` IR.
2. `.../tptp/lower_fol.rs` applies the FOL-negation reduction (`A ∪ ¬conjecture`)
   and lowers the EL/DL-expressible fragment to a world-scoped OWL-RDF EDB.
3. The native DL consistency path decides it; the verdict is graded against the
   problem's `% SZS status`.

Regenerate the derived anatomy (`input.nq`, `profile.json`, `expected/verdicts.json`)
from the authored `source/problem.p` files with:

```sh
cargo run -p gmeow-conformance --bin ingest-external -- --vendor-tptp \
  conformance/logic/cases/external/tptp-mini
```

A problem outside the native EL/DL fragment (a function symbol, an existential
conjecture, a genuine disjunction) is an **honest capability gap**, never a wrong
verdict or a silent `incomplete`. Such problems live in `tptp-mini-divergence/`
(source-only, `lane: "divergence"`) and are pinned by `tests/tptp_divergence_gate.rs`,
which asserts the native path *refuses to decide* them.

## OntoUML (foundation-discipline) corpus

`ontouml-mini/` is a self-authored, license-clean corpus in the **OntoUML metamodel
vocabulary** (`https://w3id.org/ontouml#`, the serialization the FAIR OntoUML/UFO model
catalog uses). Unlike the TPTP corpus (a consistency verdict), an OntoUML case is a
**foundation-lowering** case: it is decided by the five native OntoUML disciplines, not
the DL consistency path.

1. `crates/conformance/src/external/ontouml/model.rs` parses the model's
   `ontouml:Class`/`stereotype`, `ontouml:Generalization`, and mediation `ontouml:Relation`.
2. `.../ontouml/lower.rs` lowers it to a world-scoped, all-IRI `logic:` stereotype ABox
   (`logic:subClassOf` edges, `logic:mediates` roles, `owl:FunctionalProperty` markers).
3. `gmeow_logic::foundation::evaluate` runs the disciplines; the fired `logic:Discipline`
   set is compared to the case's **documented anti-pattern** (`profile.json`'s
   `documented_antipattern`, preserved verbatim and projected to the pass/gap comparison
   only at the gate).

The documented anti-pattern is externally decided — the OntoUML community's anti-pattern
catalogue (`https://ontouml.readthedocs.io/en/latest/anti-patterns/`). Each Lane-A case
reproduces one documented shape; **clean-control** cases (no `documented_antipattern`) must
fire NOTHING — a fired discipline there is a soundness false positive the vendor gate rejects.

| ontouml-mini case | documented anti-pattern | native disciplines fired |
|-------------------|-------------------------|--------------------------|
| `free-role`             | FreeRole              | FreeRole, MixIden |
| `mix-rig`               | MixRig                | FreeRole, MixIden, MixRig |
| `mix-iden`              | MixIden               | MixIden |
| `rel-comp`              | RelComp               | RelComp |
| `stereotype-cardinality`| StereotypeCardinality | FreeRole, StereotypeCardinality |
| `clean-kind-role`       | — (clean)             | *(none)* |
| `clean-relator`         | — (clean)             | *(none)* |

A native discipline set that CONTAINS the documented anti-pattern is agreement (extra
disciplines beyond the documented one are a disclosed superset). Regenerate the derived
anatomy (`input.nq`, `input.logic.ttl`, `profile.json`, `expected/materialized.nq`,
`expected/verdicts.json`) from the authored `source/model.ttl` files with:

```sh
cargo run -p gmeow-conformance --bin ingest-external -- --vendor-ontouml \
  conformance/logic/cases/external/ontouml-mini
```

A documented anti-pattern the native disciplines cannot reproduce is an **honest gap**,
never a wrong verdict. Such cases live in `ontouml-mini-divergence/` (source-only,
`lane: "divergence"`), pinned by `tests/ontouml_divergence_gate.rs`:

- `heterogeneous-collective` (HetColl) — a `collective` stereotype outside the endurant-
  sortal + relator fragment → an out-of-fragment **capability gap**.
- `repeatable-relator` (RepRel) — a well-formed relator whose repeatability the structural
  disciplines do not check → the model lowers cleanly and fires nothing → a **coverage gap**.

### Lane-B: the full FAIR OntoUML/UFO catalog

The real catalog (`github.com/OntoUML/ontouml-models`) is **CC BY-SA 4.0** — `ReferenceOnly`
under the native license policy — so it is **never committed**. The Lane-B grader lowers a
live-fetched subset gap-tolerantly, audits each model's own license from its `metadata.ttl`,
and records every divergence (a fired discipline on a presumed-clean model, or a capability
gap) as a `gmeow:Finding` graph:

```sh
make maint-ontouml-corpus                                     # populate .tmp/ontouml first, or:
make maint-ontouml-corpus ONTOUML_SUBSET_URL=<catalog-subset-tarball>
# → generated/conformance/divergence-ontouml.nq
```

### Lane-B: the full TPTP distribution

The real TPTP distribution has **per-problem licenses** and is never vendored. The
Lane-B grader parses and decides a live-fetched decidable subset gap-tolerantly,
recording every divergence (`Agree` / `CorpusOnly` / `DlGap`) as a `gmeow:Finding`
graph:

```sh
make maint-tptp-corpus                                   # populate .tmp/tptp first, or:
make maint-tptp-corpus TPTP_SUBSET_URL=<decidable-subset-tarball>
# → generated/conformance/divergence-tptp.nq
```

A problem the native fragment cannot decide is recorded as an honest `DlGap` row —
the documented path from the tiny committed Lane-A corpus to the full set.

## Entailment (refutation) corpus

`entailment-mini/` is a self-authored, license-clean corpus of W3C
`otest:`-style **entailment** tests (`PositiveEntailmentTest` /
`NegativeEntailmentTest`), each carrying BOTH an inline RDF/XML premise
(`otest:rdfXmlPremiseOntology`) and an inline RDF/XML conclusion
(`otest:rdfXmlConclusionOntology`). An entailment `A ⊨ C` is decided by
**refutation**: it holds iff `premise ∪ ¬C` is inconsistent (the native
`gmeow_logic::entail::dl_entails` reduction, the same one the `gmeow entails` CLI
ships). The vendored case's `input.nq` IS that reduced EDB, so the conformance
harness reproduces the frozen verdict by re-running `dl_consistency`:

- a `PositiveEntailmentTest` whose entailment holds reduces to `inconsistent`;
- a `NegativeEntailmentTest` reduces to `consistent`.

The negation is minted by the shared calculus in a reserved namespace
(`https://blackcatinformatics.ca/logic/entail/reserved#`) with a content-addressed
suffix — sound for arbitrary IRIs, and the minter hard-fails if the input
vocabulary already contains a reserved IRI. Regenerate with:

```sh
cargo run -p gmeow-conformance --bin ingest-external -- --vendor-entailment \
  conformance/logic/cases/external/entailment-mini/_source.rdf \
  conformance/logic/cases/external/entailment-mini
```

A conclusion outside the soundly-refutable single-EDB fragment — a **role assertion**
(role negation is not EL-expressible) — is an honest structured *reasoner-fragment*
gap vendored to `entailment-mini-divergence/` with its `gmeow:gapShape` token
(`role-assertion`) in `profile.json` (the data the pipeline reifies as
`gmeow:CapabilityGap`). A **multi-triple** conjunction (`A ⊨ {t₁…tₙ}`) is different in
kind: `dl_entails` decides it perfectly well, as `n` independent refutations — it is
only the frozen, single-EDB `input.nq` vendoring format that cannot freeze a
conjunctive conclusion as one case, so it is vendored with the
`vendoring-multi-goal` token (a vendoring-FORMAT limit, not a reasoner gap;
[`gmeow_logic::entail::CapabilityGapShape::is_reasoner_fragment_gap`] returns `false`
for it). The `entailment_mini_gate` test pins the non-gap coverage floor and the
exact gap set.

The upstream W3C OWL 2 / RDFCore entailment suites express their premises and
conclusions as **reference documents** (not inline), so they are graded live only
in the non-required Lane-B lane, never vendored here.

## Licensing & vendoring policy

Cases are **not** part of the published CC BY 4.0 ontology, but committing them still
requires license compatibility. `corpus.json` declares the SPDX license, audited by
the native policy in `crates/conformance/src/license.rs`
(`policy_for_license` → `IMPORT_OK` | `REFERENCE_ONLY`, conservative: unknown →
`REFERENCE_ONLY`). Only `IMPORT_OK` corpora are vendored; a `REFERENCE_ONLY` /
unknown-licensed suite is a hard error and may only be fetched live in Lane B, never
committed. Every vendored corpus also gets an entry in the repository-root `NOTICE`.

SPDX headers ride on every text artifact that supports comments (`*.logic.ttl`,
`*.p`, `*.ttl`, `*.md`); N-Quads EDB files carry their license via the corpus.

## Refresh procedure

Following the `imports/targets/` snapshot pattern:

1. Vendor the upstream source under `<corpus>/<case>/source/` and record the
   `source_url` + `version_or_commit` in `corpus.json` (license audited).
2. Ingest a source to inspect the declared verdict:

   ```sh
   cargo run -p gmeow-conformance --bin ingest-external -- --szs <case>/source/problem.p
   cargo run -p gmeow-conformance --bin ingest-external -- --manifest <case>/source/manifest.ttl
   ```

3. Derive the lowered anatomy. For a TPTP corpus this is fully mechanical —
   `--vendor-tptp` regenerates `input.nq` from `source/problem.p` via the parse →
   FOL-negation reduction → EL/DL lowering pipeline (see above). For the seed W3C /
   SZS corpora the `input.nq` is authored and the goldens are blessed, filtered to
   the external tree so no existing golden is touched; each corpus records its own
   corpus-scoped re-bless as `corpus.json`'s `refresh_command`.

   ```sh
   GMEOW_CONFORMANCE_BLESS=1 cargo nextest run -p gmeow-conformance -E 'test(/external\//)'
   ```

4. `make conformance` must stay green and sub-second; the soundness gate
   (`tests/external_soundness.rs`) cross-checks every committed verdict against its
   third-party source, and (for TPTP cases) that the `szs_status` provenance matches
   the source's `% SZS status` line.
