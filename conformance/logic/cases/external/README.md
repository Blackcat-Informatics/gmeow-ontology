<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# External correctness corpora (#752 / #753)

This tree holds **external standard correctness suites** lowered into the GMEOW
conformance per-case anatomy. They are the *independent ground truth* that the
endogenous goldens (#641) cannot provide: the native conformance goldens grade the
Rust evaluator against output the engine itself produced (proving **stability**),
whereas these corpora grade it against **third-party** standard suites (proving
**soundness**).

X1 (#753) establishes the convention, the ingestion adapter, the verdict model, and
the licensing policy. The heavy full corpora (TPTP FOL, W3C OWL 2 / SHACL, FAIR
OntoUML/UFO) are vendored on top of this by X2–X5 (#754–#757); the agreement
dashboard is X6 (#758).

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

## The verdict mapping table (the #753 deliverable)

The adapter (`crates/conformance/src/external/`) maps each external outcome onto a
first-class runner verdict status:

| External outcome (TPTP SZS / W3C `mf:`)                         | Runner verdict |
|----------------------------------------------------------------|----------------|
| `Theorem` / `Unsatisfiable` / `ContradictoryAxioms` / `mf:PositiveEntailment` | `inconsistent` |
| `Satisfiable` / `CounterSatisfiable` / `mf:NegativeEntailment` | `consistent`   |
| `Unknown` / `GaveUp` / `Timeout` / `ResourceOut` (budget-tripped) | `incomplete`   |

An entailment `A ⊨ C` is decided as the consistency of `A ∪ ¬C`, so a
`PositiveEntailment` / `Theorem` only manifests as `inconsistent` once the negated
conclusion is folded into the EDB (the seed fixtures pre-bake it).

Every verdict is **genuinely engine-decided** (zero defer): consistency-mode cases
run the native DL consistency path (`gmeow_logic::reason::reason_all`) and the run
hard-fails if the engine reports an undecided construct gap; the budget branch is a
real governor trip. Nothing is faked or looked up.

## Two-lane model

| | Lane A (this gate) | Lane B (heavy oracle) |
|-|--------------------|------------------------|
| target | `make conformance` (required) | `make maint-classic-cross-check` (non-required, Docker) |
| scope | small, deterministic, sub-second, decided natively | full third-party corpora, oracle-backed |
| routing | committed goldens + the soundness gate | the divergence ledger (`crates/logic/src/reason/ledger.rs`) |
| `corpus.json` `lane` | `"a"` | `"b"` |

X1 seeds only Lane-A corpora. A corpus too large or outside the native fragment is a
Lane-B corpus (vendored by X2–X5) and is never required for normal repo use.

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
2. Ingest a source to inspect the declared verdict (AC1):

   ```sh
   cargo run -p gmeow-conformance --bin ingest-external -- --szs <case>/source/problem.p
   cargo run -p gmeow-conformance --bin ingest-external -- --manifest <case>/source/manifest.ttl
   ```

3. Author the lowered `input.nq` (consistency) or `input.logic.ttl` (materialization)
   and `profile.json`, then bless the goldens — filtered to the external tree so no
   existing golden is touched. Each corpus records its own corpus-scoped re-bless as
   `corpus.json`'s `refresh_command` (e.g. `GMEOW_CONFORMANCE_BLESS=1 cargo nextest
   run -p gmeow-conformance -E 'test(external/<corpus>/)'`), so refreshing a corpus
   regenerates all of its goldens from the engine. (Mechanical `input.nq` regeneration
   from a source — the general `A ∪ ¬C` FOL-negation reduction — is X2/X3 scope
   (#754–#755); in X1 the negated conclusion is pre-baked by hand, so `input.nq` is
   authored. The step-2 `ingest-external` command inspects the verdict a `source/`
   declares, and `tests/external_soundness.rs` cross-checks it against the committed
   golden.)

   ```sh
   GMEOW_CONFORMANCE_BLESS=1 cargo nextest run -p gmeow-conformance -E 'test(/external\//)'
   ```

4. `make conformance` must stay green and sub-second; the soundness gate
   (`tests/external_soundness.rs`) cross-checks every committed verdict against its
   third-party source.
