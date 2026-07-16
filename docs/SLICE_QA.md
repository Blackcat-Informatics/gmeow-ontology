<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Slice QA — moving every test bit into the slice structure

This is the operational guide for where a slice's quality assurance lives and how
to move QA *into* the slice structure rather than into bespoke Python. It uses the
**`logic` slice** (`slices/grounding/logic/`) as the baseline, because logic is the
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
slices/grounding/logic/
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
  example-conformance harness only. The grounding kernel is the sole data-scope
  exception: each of its three peers sees the `logic:` + `lang:` + `math:` module
  union, but shape authority remains restricted to the tested slice.
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

## The continuous uplift lane (slice-quality)

Everything above is about **where a hard pass/fail test bit lives** — a
competency/structural/example cell, a SHACL case, an engine golden, or the pytest
residue. The slice-quality lane is a different animal: an **advisory, ratchet-gated
*measuring* instrument** over a slice, not a hard test cell and not a hard
validation gate. It never asserts a bit is right or wrong; it scores how far a
slice has been lifted and holds the gains. Keep the two straight — do not file a
quality *score* as a competency cell, and do not expect a cell to move a tier.

- **Advisory worklist** — `make slice-quality` scores one slice (`SLICE=…`) or the
  whole repo (`--all`) against the rubric and prints a prioritized worklist: each
  slice's roll-up tier plus its capping axis (the weakest axis dragging the meet
  down). This is advisory only — an undeclared slice is scored but **never** gates.
- **Ratchet gate** — `make slice-quality-gate` (now part of `make check`) enforces
  the opt-in ratchet for slices that declare `gmeow:sliceQualityTier`. Its committed
  floors are **ontology-resident** in the slice-quality-rubric slice's `module.ttl`:
  the per-slice roll-up tier floor is a `gmeow:SliceTierFloor` individual
  (`gmeow:floorSlice` + `gmeow:floorTier`) and each per-axis score floor is a
  `gmeow:AxisFloorCommitment` individual (`gmeow:floorSlice` + `gmeow:floorAxis` +
  `gmeow:floorValue`). Those individuals are the canonical source; the read-only TSVs
  `generated/governance/slice-quality-floors.tsv` and
  `generated/governance/slice-quality-axis-floors.tsv` are **generated lossy
  projections** of them (Principle 17), carrying a loss-ledger preservation judgment —
  view them, never hand-edit them. A human raises a floor by editing the individual in
  the rubric slice. Floors are **raise-only**: LOWERING a committed floor is a hard gate
  failure, as is deleting a still-live floor. There is no in-repo way to permit a
  lowering — re-baselining a floor downward is a **maintainer-only decision**, and the
  maintainer applies it out-of-band by authorizing the merge past the resulting red.
  No code path, flag, record, doc, or signal a tool or agent can set ever relaxes the
  ratchet.

This is the sibling of the four blocking *validation* gates in
[`docs/validation-thresholds.md`](./validation-thresholds.md): same ratchet
temperament, different family. Those four are hard validation gates over the whole
ontology; this one measures per-slice quality. Do not conflate the two floor sets.

**The floor-ratchet policy (strictly raise-only).** Both floor levels are non-regression
contracts, read straight from the ontology individuals in the rubric slice: a committed
floor may be **RAISED** freely as a slice earns it (edit the `gmeow:AxisFloorCommitment`
/ `gmeow:SliceTierFloor` individual), and is never forced upward ahead of a real measured
uplift (scores stay objective, uncalibrated intrinsic measures; you do not tune a floor
to a target). **LOWERING a committed floor is a hard gate failure.** There is no in-repo
permit, exemption, or re-anchor signal — re-baselining a floor downward is a
**maintainer-only decision**, exercised out-of-band by the maintainer authorizing the
merge past the red. The gate reads **every** committed axis floor from the ontology (not
just GMN-1) and enforces each in addition to the roll-up-tier ratchet. The gate verdicts,
verbatim from `crates/slice-quality/src/gate.rs`, are:

- `MeasuredBelowDeclared` — the slice's measured roll-up tier fell below the tier
  its manifest declares.
- `DeclaredBelowFloor` — the manifest lowered its declared tier below the committed
  ratchet floor.
- `MeasuredBelowFloor` — a per-axis measured score fell below that axis's committed
  floor (gated in addition to, never instead of, the roll-up-tier ratchet).

Separately, a **floor-monotonicity** check diffs the committed floor individuals against
the merge base. A lowered `gmeow:floorValue`/`gmeow:floorTier` is a **HARD FAIL**, as is
the deletion of a floor individual for a still-live slice/axis. Additions and raises are
clean; greenfield deletion is allowed only once the slice or axis is genuinely gone. When
a measure definition legitimately changes and a floor must be re-baselined downward, that
is the maintainer's call alone — applied out-of-band by authorizing the merge past the
red, never by any in-repo relaxation the gate would honour.

And a **floor-coherence** check ties the two committed levels together as a pure
consistency assertion (it compares committed floors against each other, never a measured
score). For a tier-floored slice, every committed axis floor must grade — through that
axis's rubric thresholds — to a tier `≥` the committed tier floor (the *backing
invariant*: the roll-up is a lattice meet, so a tier floor requires every axis floor to
back it); and when a slice is floored on *every* rubric axis, its tier floor must equal
the meet of its axis-floor-implied tiers (*tightness*: a lower tier floor is a dead
guarantee). Either failure reds `make slice-quality-gate`.

**Sweep work is never an issue.** Cross-cutting quality work — "every slice needs
X" — is not filed as an issue and never lands as a mega-PR. Re-scope the sweep into
the rubric (a new or sharpened axis), the curation docs, and the uplift skill, then
discharge it continuously, one **slice-local** capping-axis fix at a time, each its
own small slice-local PR that ratchets one floor. The lane is the sweep.

The operational loop — how to read the worklist, pick the capping axis, land the
slice-local PR, and ratchet the floor — is the contract in the
`gmeow-slice-uplift` skill (`.agents/skills/gmeow-slice-uplift/SKILL.md`). Follow
it there; this section only draws the boundary against the hard test cells above.
The semantic curation strategy — including how to replace term-list tests with
behaviour-connected, production-executed evidence — is in
[`SLICE-UPGRADE.md`](SLICE-UPGRADE.md).

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
- **External oracle / Docker orchestration** — retired rdflib / external OWL 2 DL lanes.
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

**Deleting a test deletes its dossier.** A retention dossier exists *only* to
justify a still-living pytest. So the removal change is not complete until it also
deletes the matching `docs/test-retention/<name>.md`. Every removal issue must
carry this in its acceptance criteria: *delete the pytest, delete its dossier(s),
redirect any constitution citation* — all in the one change. A dossier left behind
after its test is gone is itself a lint failure waiting to happen.

## Migrating a slice's hand-authored `shapes.ttl` to `logic:`

The slice-local `shapes.ttl` in the anatomy above is **transitional**. Under
Principle 17 the only authored validation form is `logic:`; SHACL is a generated
projection. A slice still shipping a hand-authored `shapes.ttl` whose `sh:NodeShape` /
`sh:PropertyShape` blocks lack a `logic:formalizes` back-reference carries per-slice
migration debt, measured by the **Shape Migration** axis (`gmeow:axisShapeMigration`,
producer `shape_migration_axis`): the fraction of its authored shape blocks that *are*
grounded (carry `logic:formalizes`). Its per-shape finding is
`slice-quality.projection.ungrounded-shape`, naming each un-backed block. (Do not
confuse it with `slice-quality.projection.hand-authored-shapes`, the *presence* advisory
of the different `axisMaximalProjection` axis, which fires merely because a `shapes.ttl`
exists at all.) A slice with **no** `shapes.ttl` has nothing to migrate and scores a
vacuous `1.0`. The axis gates four tiers by measured fraction: Grounded `0.60`, Linked
`0.75`, Exemplified `0.85`, Maximal `0.95`.

Its floor is committed like every other axis floor: as a `gmeow:AxisFloorCommitment`
individual (`gmeow:floorSlice` + `gmeow:floorAxis gmeow:axisShapeMigration` +
`gmeow:floorValue`) authored in the slice-quality-rubric slice's `module.ttl` — the
canonical source, of which `generated/governance/slice-quality-axis-floors.tsv` is the
read-only generated projection. The floor-coherence check keeps this axis honest with the
slice's tier floor: its committed floor must grade to a tier at or above the committed
`gmeow:SliceTierFloor`, and on a fully-floored slice the tier floor is exactly the meet of
the axis-implied tiers.

Discharge the debt one slice at a time, under equivalence-before-deletion: re-express each
shape as a `logic:Constraint` (procedural) or an OWL/RDFS axiom (declarative) in
`module.ttl` so it PROJECTS to `generated/shapes/*`, prove the slice's
counter-examples still fail identically against the projected union, **then** retire
the hand-authored shape (or back a genuine ValidationOnly residue with `logic:formalizes`).
Never delete a shape whose check the projection does not yet reproduce — that drops live
enforcement. Full procedure, idioms, and the reasoner-safety rules for cardinality:
[`docs/MIGRATING-SHAPES-TO-LOGIC.md`](./MIGRATING-SHAPES-TO-LOGIC.md).

`axisShapeMigration` is measure-only and advisory. Its hard-enforcement counterpart is
the **projection-vocabulary ratchet**, a `make check` gate that caps hand-authored
ungrounded growth in `shapes.ttl`/`module.ttl` SHACL *and* every other `logic:`-subsumed
projection vocabulary (gUFO, BFO, DOLCE, FnO, EDOAL, SSSOM) per slice, and hard-fails on
any net-new growth: [`docs/PROJECTION-VOCABULARY-RATCHET.md`](./PROJECTION-VOCABULARY-RATCHET.md).
