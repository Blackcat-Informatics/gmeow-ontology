<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Slice QA — moving every test bit into the slice structure

This is the operational guide for where a slice's quality assurance lives and how
to move QA *into* the slice structure rather than into bespoke Python. It uses the
**`logic` slice** (`slices/core/logic/`) as the baseline, because logic is the
canonical, most-developed slice (its `design/` set is normative per the project
baseline) and it exercises every QA layer.

The governing rule is the project's two-layer testing doctrine and `.goals`
(rust-first, python-surface): **QA is declarative ontology data resident in the
slice, run by Rust harnesses.** Bespoke Python is a last resort, allowed only for
assertions no declarative cell and no Rust test can express — and each survivor
must carry a retention dossier (`docs/test-retention/`) plus a removal issue.

## The QA layers (where a test bit goes)

| Layer | Lives in | Run by | What it covers |
|---|---|---|---|
| **Competency** | `slices/<g>/<slice>/tests/competency.ttl` | `crates/slicetest` (Rust) | query-answerability: SPARQL ASK/SELECT + expected outcome |
| **Structural** | `slices/<g>/<slice>/tests/structural.ttl` | `crates/slicetest` | MUST / MUST-NOT graph invariants (SHACL-style ASK over the module) |
| **Example conformance** | `slices/<g>/<slice>/tests/example-conformance.ttl` | `crates/slicetest` | "this example conforms / that counter-example violates code X" |
| **Whole-ontology SHACL** | `crates/validate/tests/conformance_<slice>.rs` + `ontology_conformance.rs` | `crates/validate` (Rust) | SHACL over the **merged** shapes corpus (cross-slice `sh:class` fidelity) |
| **Engine conformance** | `conformance/<engine>/cases/**` (repo root) | `crates/conformance` (Rust) | engine output goldens (logic reasoner: projections, answers, ledger) |
| **Bespoke residue** | `tests/test_*.py` | `pytest` | only what the four layers above cannot express — being culled |

The first three are **slice-resident declarative cells**; they ship with the
slice and are the default home for new QA. The Rust layers are the authority for
SHACL and for engine output. Pytest is the residue, not a layer to grow.

## Slice anatomy (logic baseline)

```text
slices/core/logic/
  manifest.ttl              # sole source of slice identity + tier (gmeow:Slice, sliceTier, sliceConsumer)
  module.ttl                # the slice's vocabulary/axioms
  shapes.ttl                # slice-local SHACL shapes
  examples/                 # positive example individuals (loaded by `make validate`)
  queries/                  # slice-local .rq (competency / projection queries)
  design/                   # normative design docs (logic: the canonical five-doc set)
  docs.md                   # human documentation
  tests/                    # ← all slice-local QA
    competency.ttl          #   CompetencyQuestion cells
    example-conformance.ttl #   ExampleConformance cells
    conformance-fixtures/   #   positive fixtures, slice-scoped only (NOT loaded by global validate)
    counter-examples/       #   negative fixtures, referenced by exampleFile (slice-scoped only)
    fixtures/               #   ABox overlays for competency cqDataFile / structural inputs
```

Notes from the baseline:

- A slice carries **only the cell files it needs**. `logic` ships
  `competency.ttl` and `example-conformance.ttl` (no `structural.ttl`); other
  slices add `structural.ttl`. The harness keys on filename, so absence just
  means "no cells of that kind."
- `tests/conformance-fixtures/` and `tests/counter-examples/` are deliberately
  **outside** `examples/` so the global `make validate` gate never loads them as
  data; they are validated **slice-scoped** (module + slice shapes) by the
  example-conformance harness only.
- `manifest.ttl` is the **only** truth for slice identity and tier. Registering a
  new slice still needs the root `owl:imports` + the self-count edits — see the
  slice-registration notes; QA cells do not change registration.

## The three declarative cell types

The cell vocabulary is the **test-DSL** at `dsl/tests/vocabulary.ttl` (a DSL
module, not a slice). Each class carries `gmeow:useWhen` / `gmeow:avoidWhen` —
read those before authoring. Summary:

| Cell | Class | Use when | Key fields |
|---|---|---|---|
| Competency | `gmeow:CompetencyQuestion` | a query must answer a certain way | `cqQuery` \| `cqQueryFile`; `cqExpectAsk` (ASK) or `cqExpectRow`+`cqExactRows` (SELECT); `cqRationale`; optional `cqReasoning gmeow:reasoningRdfs`; optional `cqDataFile` (ABox overlay, asserted lane only) |
| Structural | `gmeow:StructuralAssertion` | a MUST / MUST-NOT invariant over the module graph | `saPolarity` (`must`/`mustNot`); `saPattern` (ASK); `saScope` (`scopeModule` / `scopeModuleAndExamples`); `saRationale` |
| Example conformance | `gmeow:ExampleConformance` | a specific example must conform / counter-example must violate | `exampleFile` (slice-relative); `expectedOutcome` (`conforms`/`violates`); `expectedViolationCode` (e.g. `shacl.MinCountConstraintComponent`) |

Discipline that keeps cells honest:

- **Pin the CODE, not the message.** Example-conformance cells assert the
  constraint-component code (`shacl.<Component>`, from
  `gmeow_validate::findings::finding_from_shacl`); isolate one violation per
  counter-example so `(fixture, code)` still pins it exactly.
- **Reasoning lane.** Cells run over the asserted merged TBox by default;
  `cqReasoning gmeow:reasoningRdfs` opts a competency cell into RDFS closure.
  `cqDataFile` overlays an ABox for one cell, asserted lane only.
- **Scope is closed.** `saScope` is exactly `scopeModule` or
  `scopeModuleAndExamples` — do not mint new scopes.

## How the harnesses discover and run cells

`crates/slicetest` uses `datatest-stable` to discover, by **filename**, every
slice-resident spec and emit one nextest case per file:

```text
slices/**/tests/competency.ttl          → run_competency_file
slices/**/tests/structural.ttl          → run_structural_file
slices/**/tests/example-conformance.ttl → run_conformance_file
```

`counter-examples/*.ttl` never matches the three fixed names, so it is excluded
structurally and only reached via `gmeow:exampleFile`.

Gate map:

- `make slicetest` — the slice-resident cells in isolation (`cargo nextest -p gmeow-slicetest`).
- `make validate` — whole-ontology SHACL + structural lint over `src/` (incl. `examples/`).
- `make rust-test` — all Rust crate tests, including `conformance_<slice>.rs` and `crates/conformance`.
- `make test` — the bespoke pytest residue (shrinking).

## Recipe: move a QA bit into the slice

For each assertion currently in Python (or being newly authored), pick the home:

1. **Is it "a query answers thus"?** → `CompetencyQuestion` in `tests/competency.ttl`.
   Inline `cqQuery` for small queries; `cqQueryFile` (repo-root-relative) for
   shared/large ones. ASK → `cqExpectAsk`; SELECT → enumerate `cqExpectRow` with
   `cqExactRows true`.
2. **Is it a MUST / MUST-NOT shape over the module graph?** (subclass, disjoint,
   domain/range, property character, "term exists", "no preferred/primary term")
   → `StructuralAssertion` in `tests/structural.ttl`. Cross-slice subject? Author
   the cell in the **owning** slice's `structural.ttl` (where the term is defined),
   not the consuming slice — that is the correct home, and it removes the "needs
   merged graph" excuse.
3. **Is it "this example validates / that one is rejected with code X"?** →
   `ExampleConformance` in `tests/example-conformance.ttl`, with the fixture in
   `examples/` (positive, also seen by `make validate`) or
   `tests/conformance-fixtures/` + `tests/counter-examples/` (slice-scoped only).
4. **Is it a SHACL shape over the merged ontology (cross-slice `sh:class`)?** →
   a case in `crates/validate/tests/conformance_<slice>.rs`.
5. **Is it engine output (reasoner projections/answers/ledger)?** → a case under
   `conformance/<engine>/cases/**`, golden-blessed and run by `crates/conformance`.
6. **None of the above** → it is a candidate **keeper**; see below. Most "keepers"
   are actually case 2 in disguise (a structural cell in the owning slice).

## What legitimately stays in pytest (and the price of keeping it)

A pytest test survives **only** if its *substance* is still Python and no cell /
Rust test can express it. The standing categories:

- **Python CLI surface** (Typer apps via `CliRunner`) — until the CLI is Rust.
- **PyO3 seam** — tests the binding marshalling/error-surfacing itself.
- **Live Python tool algorithm** — up-projection, transform, projections,
  mappings, saturate, coverage, crossref, language-tags, GTS shims, music package
  (these are being subsumed: alignment/projection by the Correspondence Calculus;
  the rest by per-tool Rust ports).
- **External oracle / Docker orchestration** — rdflib/ELK/HermiT lanes.
- **Static repo guard** — Python-AST / workflow assertions about the repo.

Every survivor pays two costs, both mandatory:

1. a **retention dossier** at `docs/test-retention/<name>.md` — what it tests, why
   it has no Rust home today, and exactly what migration retires it (kept free of
   issue numbers per the project baseline); and
2. a **removal issue** on GitHub — the Rust-parity work that retires it, routed
   onto the owning epic where one exists (do not duplicate an existing epic).

If you cannot be bothered to write the dossier and issue, delete the test instead.

## Equivalence-before-deletion

Per Principle 6 (greenfield) tempered by Principle 7 (verified by construction):
when moving QA out of pytest, **add the cell / Rust case first, confirm the gate
is green, then delete the pytest** in the same change. For generated artifacts the
committed golden set is the oracle — the new path must regenerate byte- or
graph-isomorphically before the old test (and any orphaned Python module) is
removed. A deleted test that cited a `governance/constitution.ttl`
`meta:artifact` must have that citation redirected to the Rust artifact that now
proves the principle, or the constitution gate reds.
