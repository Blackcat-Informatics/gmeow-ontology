# Issue #1657: math: tighten the signed radial-field value constraints and register the flagship scenes

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, quality, logic, complexity-medium

## Body

Drained from the local `.deficiencies` ledger (recorded under #1544, merged as PR 1550). Re-verified against current `main`.

Relates to: #1443 (math expressions hardening — the sibling Rust-validator failure-class work, currently in flight) and #1551 (logic:-only grounding for the math slice).

## Problem

The signed compactified radial field grounding shipped with three disclosed fragment limits and one unregistered facility. Each was an accepted boundary at the time; none is tracked.

### 1. The extended-real gate does not catch a non-numeric literal

`math:ExtendedRealValueConstraint` enforces the non-literal-must-be-a-pole case, but the guarded `logic:` fragment has no numeric-datatype test, so a non-numeric literal in an extended-real position passes.

### 2. Measure non-negativity does not forbid a finite negative

`math:MeasureResultNonNegativeConstraint` forbids the `math:NegativeInfinity` pole but not a finite-negative literal. `logic:ValueRangeConstraint` landed on main after this work, so a tightening is now plausible — but `measureResult` spans literal-or-`PositiveInfinity`, so `sh:minInclusive` is not straightforwardly safe. Needs a design pass, not a one-liner.

### 3. `gmeow:FlagshipScenario` is not registered for the radial scenes

The positive-capability radial scenes are not registered as flagship scenarios: the facility needs a native Rust producer plus a single enforceable hard-rule failure. The scenes carry the full non-flagship test surface instead.

### 4. Three examples ride the non-conformant allowlist

`examples/closed-form-functions`, `signed-radial-field-qualitative`, and `signed-radial-field-closed-form` are on the `example_sweep` `NON_CONFORMANT` allowlist (`crates/validate/tests/example_sweep.rs:44`) for standalone closed-world isolation — the same accepted pattern as ~14 sibling math examples. They conform in the merged bundle (`make validate` clean). Worth revisiting as part of any wider decision about that allowlist, not on its own.

## Two things that are correct as shipped — do not "fix" them

- `math:convexityLaw` is authored as **midpoint** convexity, not the full lambda-chord inequality: the continuum of lambda needs scalar arithmetic beyond the first-order guarded fragment. Disclosed in the law, not faked.
- Closed-form leaf-linkage runtime enforcement is a `tests/structural.ttl` MUST cell over authored instances rather than a live `logic:Constraint`, because transitive AST traversal exceeds the guarded fragment.

## Work

- [ ] Give the extended-real gate a numeric-datatype test, or record why the guarded fragment cannot carry one.
- [ ] Decide how to forbid a finite-negative `measureResult` without breaking the literal-or-pole span.
- [ ] Register the radial scenes as flagship scenarios, or state that the non-flagship surface is sufficient.


## Comments (4)

### coderabbitai — 2026-07-27T00:19:05Z

<!-- This is an auto-generated issue plan by CodeRabbit -->
<details>
<summary>🔗 Related PRs</summary>

Blackcat-Informatics/gmeow-ontology#1317 - feat(math): measure, integration & the dimensional-homogeneity gate (MEASURE-AND-DIMENSION) [merged]
Blackcat-Informatics/gmeow-ontology#1357 - feat(math): flagship acceptance manifest — the epic depth bar as a typed gated contract (`#1255`) [merged]
Blackcat-Informatics/gmeow-ontology#1550 - feat(math): signed compactified radial field grounding [merged]
Blackcat-Informatics/gmeow-ontology#1635 - feat(1609): reasoner-derived dimensional-homogeneity gate from logic:Formula ASTs [merged]
Blackcat-Informatics/gmeow-ontology#1639 - feat(1443): math expressions hardening — typed lowering error algebra, attributed normal-form identity, whole-matrix charter discharge gate [open]
</details>

---
<details>
<summary>📝 Issue Planner</summary>

<sub>Check the box below or use the `@coderabbitai plan` command to generate an implementation plan and prompts that you can use with your favorite coding assistant.</sub>

- [ ] <!-- {"checkboxId": "8d4f2b9c-3e1a-4f7c-a9b2-d5e8f1c4a7b9"} --> Create Plan
</details>


---
<details>
<summary> 🧪 Issue enrichment is currently in open beta.</summary>


You can configure auto-planning by selecting labels in the issue_enrichment configuration.

To disable automatic issue enrichment, add the following to your `.coderabbit.yaml`:
```yaml
issue_enrichment:
  auto_enrich:
    enabled: false
```
</details>

💬 Have feedback or questions? Drop into our [discord](https://discord.gg/coderabbit)!

### paudley — 2026-09-04T16:20:51Z

## Status reconciliation

No implementation work has been started on this issue. A worktree (`issue-1657-01M13AQ5`) and branch (`agent/issue-1657-stage-1`) existed and could be mistaken for work in flight, but the branch carried **zero commits of its own** — its tip was just a sync to `main` (`d56923da8`), and the only working-tree change was unrelated local `.gitignore` drift.

Both have been removed, so the stale in-flight signal is gone. This issue is unclaimed and starts from current `main`.

### paudley — 2026-09-06T02:53:08Z

## Item 1 investigated — and one of its two offered options would record something false

> - [ ] Give the extended-real gate a numeric-datatype test, **or record why the guarded fragment cannot carry one.**

The second option is not available, because the guarded fragment is not the obstacle. Recording that it is would be a false claim in exactly the sense P11 warns about. The real obstacle is narrower and more actionable.

### What the constraint actually is

`math:ExtendedRealValueConstraint` (`slices/grounding/math/module.ttl:14834`) lowers to

```
∀ this, s . math:extendedRealValue(s, this) → ( logic:termIsLiteral(this) ∨ logic:termIn(this, {math:PositiveInfinity, math:NegativeInfinity}) )
```

`logic:termIsLiteral` is a **node-kind** test — it admits `"hello"` exactly as readily as `2.5`. That half of the issue is confirmed.

### Why it is a raw formula, and why that is correct

Its own authoring comment (`:14828-14833`) already says: no declarative sugar fits a literal-OR-one-of-two-individuals disjunction, so it is a raw `logic:Formula`. That judgement holds.

### The actual blocker: datatype testing exists only in the DECLARATIVE lane

I enumerated the builtin registry from the compiler rather than from documentation. It is exactly 19 names (`crates/logic-compile/src/frontend.rs`):

`termApplication`, `termDistinct`, `termEqual`, `termGreater`, `termGreaterEqual`, `termIn`, `termIndex`, `termIri`, `termIsBlankOrIri`, `termIsIri`, `termIsLiteral`, `termLess`, `termLessEqual`, `termLiteral`, `termLiteralDatatype`, `termRegex`, `termSequenceMarker`, `termStrStarts`, `termVariable`

**None of them tests a datatype.** `logic:termLiteralDatatype` looks like the exception and is not: it is term-*construction* syntax, legal only alongside `logic:termLiteral` inside a `logic:TermCarrier`, and `frontend.rs:4369-4378` hard-fails any other use. It builds a typed literal; it cannot ask whether one is typed.

Datatype testing does exist in this codebase — `ConstraintComponent::Datatype` → `sh:datatype` — but every producer of it is **declarative**: a `logic:Restriction` whose filler names a datatype IRI (`frontend.rs:1748`), or `logic:onDatatype` + `withRestrictions` facets (`frontend.rs:1820`). Nothing lowers a `logic:Formula` into it.

So the gap is a clean pincer, and neither arm is the guarded fragment:

- the **declarative** lane can express `sh:datatype` but cannot express this disjunction;
- the **procedural** lane can express the disjunction but has no datatype predicate.

### What closing item 1 would actually take

A new formula-level builtin — `logic:termDatatypeIn` is the natural spelling, taking a term and a datatype value-set, mirroring `logic:termIn` — with:

1. the `logic:` term itself, with its full annotation coat and fr/zh translations;
2. reasoner semantics in the native evaluator;
3. a SHACL projection (an `sh:or` over `sh:datatype` members, since the numeric family is a disjunction);
4. conformance cells proving it rejects `"hello"` and admits `2.5`, plus the negative control that it still admits both poles through the other arm.

That is a vocabulary addition, not a one-line tightening, which is why I have not bundled it into the current run of small fixes. Item 2 (`math:MeasureResultNonNegativeConstraint`) plausibly wants the same builtin plus `logic:termGreaterEqual`, so the two should be designed together rather than separately.

**Suggested edit to this issue:** drop "or record why the guarded fragment cannot carry one" from item 1 — the fragment can carry it, and writing otherwise would bake in a false rationale. Replace it with the builtin as the named dependency.

### paudley — 2026-10-20T07:59:11Z

# Signed radial-field value constraints

Implement the full value-validation work in #1657 under Principles 4, 7, 17 and
18. Introduce canonical logic:termIsFiniteNumeric with native finite-numeric
admission and generated SPARQL validation semantics. Preserve arbitrary-precision
integer/decimal values and original RDF identity; refuse text, language literals,
NaN and floating infinities. Add its annotation coat and French/Mandarin catalog
entries together.

Use that predicate in math:ExtendedRealValueConstraint's literal branch. Replace
the insufficient pinned negative-pole measure check with an integrity formula
requiring every result to be PositiveInfinity or a finite non-negative number.
Author explicit conformance fixtures for nonnumeric values, finite negatives,
both infinity poles and positive controls. Do not alter midpoint convexity or
closed-form leaf-linkage policy.

Assess the existing radial scenes' structural, competency and conformance coverage.
The issue explicitly permits the non-flagship surface when sufficient. Retain
the five executable flagship scenarios and state why the two authored scenes are
worked competency examples rather than claim an unimplemented numeric producer.

Test native intrinsic evaluation and generated shape validation against authored
synthetic inputs, including large exact integers/decimals and nonfinite values.
Tests never build a corpus. Run focused Make checks, strict producer-independent
Clippy and normal hooks. Actual whole-corpus acceptance remains a separate
unverified requirement under the CI waiver; do not claim a full gate pass.
Merge through ghprsq and remove the completed worktree/branch.


