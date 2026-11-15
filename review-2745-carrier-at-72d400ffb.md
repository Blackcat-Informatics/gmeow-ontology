# Review: term-carrier index constraint, branch paudley/2745-term-carrier-index

- Head reviewed: `72d400ffb68c6abfa1d823f075edd6460dfbb829` (checked with `git rev-parse HEAD` in the worktree, before and after the review)
- Base: `331c554f87153e25c95919c933ecb6aa9f44d20b`; range `331c554f8..72d400ffb` is a single commit
- Review started: Sat 14 Nov 2026 06:53:56 PM CST; report written: Sat 14 Nov 2026 07:08:08 PM CST (`date`)
- Mode: read-only on the repository. No make, cargo, build or test runs. One prebuilt binary
  (`target/release/gmeow convert --from turtle --to ntriples`) was run to parse Turtle for the independent
  scan. Scan scripts and outputs are in `.stage/reviews/scan-2745/` in the main checkout.

## Scope

The requirement: `logic:TermCarrierIndexConstraint` must require `logic:termIndex` exactly where the compiler
does, which is on ordered operands and binders. It must not apply class-wide to `logic:TermCarrier`. It must be
authored as canonical `logic:` and be accepted and lowered by `crates/logic-compile`. It must still reject an
indexless operand and an indexless binder, must not report the 169 rule-variable carriers, and must add no new
violations on authored content.

## Coverage

| File | Lines read | Commit |
|---|---|---|
| full diff `331c554f8..72d400ffb` (11 files) | all | 72d400ffb |
| crates/logic-compile/src/frontend/formula_reader.rs | 230-430, 640-1079 | 72d400ffb |
| crates/logic-compile/src/frontend.rs | 500-620, 2040-2270, 5020-5045 | 72d400ffb |
| crates/logic-compile/src/ir/constraint.rs | 490-834 | 72d400ffb |
| crates/logic-compile/src/projections/shapes.rs | 1036-1075, 1819-1960 | 72d400ffb |
| crates/logic-compile/src/projections/shapes.procedural_tests.rs | 566-585, 1783-1793, 2077-2126 (new test); base 256-296 | 72d400ffb / 331c554f8 |
| crates/logic-compile/src/projections/rdf.rs | 1765-1780, 1850-1900 | 72d400ffb |
| crates/logic-compile/src/relational_core.rs | 991-1051 | 72d400ffb |
| crates/slicetest/src/exec.rs | 800-842 (grep), 880-1099 | 72d400ffb |
| crates/pipeline/src/stages/distribution_catalog.rs | 790-880 | 72d400ffb |
| crates/logic/src/modal/contextual/native.rs | 55-100 | 72d400ffb |
| crates/docs/tests/po_parser_parity.rs | all | 72d400ffb |
| crates/docs-model/src/reviewed_coverage.rs | 1-166; paths.rs all; examples/refresh-reviewed-coverage-golden.rs all | 72d400ffb |
| crates/pipeline/src/stages/conformance/reviewed_translations.rs | all | 72d400ffb |
| crates/docs/tests/fixtures/reviewed_coverage_golden.json | key counts via jq; logic fr.po key list | 72d400ffb |
| .config/nextest.toml | 55-135 | 72d400ffb |
| slices/grounding/logic/module.ttl | 16846-16900, 16955-17060, 17123-17230, 17280-17415 | 72d400ffb |
| slices/grounding/logic/design/LOGIC-IR.md | 30-70, 600-615 | 72d400ffb |
| slices/grounding/logic/tests/structural.ttl | 985-1030 | 72d400ffb |
| slices/grounding/logic/tests/conformance-fixtures/typed-ir-constraints-wellformed.ttl | all | 72d400ffb |
| slices/grounding/logic/tests/counter-examples/typed-ir-{binder,carrier}-without-index.ttl, typed-ir-two-constructors.ttl | all | 72d400ffb |
| slices/grounding/logic/i18n/{fr,zh}.po | changed hunks; msgid byte comparison by script | 72d400ffb |
| slices/core/slice-quality-rubric/module.ttl | logic floor commitments (grep) | 72d400ffb |
| docs/PROJECTION-VOCABULARY-RATCHET.md | 20-85, 170-215, 320-330 | 72d400ffb |
| author evidence `.stage/issue-2745-carrier/` | nextest-final.log, probe-output.txt, golden-refresh.log, golden-keys-from-this-change.txt (read only as claims) | n/a |

**Not read:** LOGIC-VALIDATION.md and docs/MIGRATING-SHAPES-TO-LOGIC.md in full (grep showed neither names
the changed constraint). Also not read: the rest of exec.rs (shape-graph assembly, `surface.matches_source`),
`violation_where` in shapes.rs (lowered query text taken from the author's probe and the Rust test assertion,
not traced line by line), and the slice-quality axis scorers. The scan did not cover Turtle embedded in Rust
sources, any untracked file, or the generated corpus.

## Evidence

- **Independent scan.** I converted all 324 tracked `.ttl` and `.trig` files that contain the text `argument`
  or `quantifiedVariable` to N-Triples with the purrdf parser. This covers prefix aliases such as `l:`, and
  zero conversions failed. I then indexed every object of `logic:argument` and of `logic:quantifiedVariable`:
  18010 of them. Blank nodes are scoped per file and IRIs are global. Because N-Triples flattens bracketed
  blank-node property lists, nested `[ ... ]` carriers are counted. Exactly 2 objects carry no
  `logic:termIndex`:
  - `ex:unpositionedBinding` (qv), in typed-ir-binder-without-index.ttl
  - `ex:unpositionedCarrier` (arg), in typed-ir-carrier-without-index.ttl

  Each of the 11 non-simple lines that mention either predicate is an annotation on the property itself. No
  triple term contains either predicate.
- **The 169 rule-variable carriers.** In logic/module.ttl plus lang/module.ttl, 169 nodes carry
  `logic:termVariable` or have type `logic:TermCarrier` and lack `logic:termIndex`. This matches the issue's
  count. Every one of them is referenced on a line that contains a `<<( … )>>` triple term, and none is an
  object of `argument` or `quantifiedVariable`.
- **Catalogs.** A script compared each `@x-gmeow-english` literal on both constraints with the msgid under the
  matching msgctxt. All 12 changed or new literals match byte for byte in fr.po and in zh.po. The unchanged
  `TermCarrierIndexConstraint` avoidWhen and howToUse have no catalog entry at base or at head. The sibling
  constraints are the same.
- `prose-check --stdin` over all added diff lines: EXIT=0.
- Author's claims, read but not rerun:
  - the focused nextest selection passed 3 tests
  - the probe derived the target `ObjectsOf(argument)` / `ObjectsOf(quantifiedVariable)` and the lowered
    queries, and the fixtures behaved as intended

## Findings

1. **REQUIRED: slices/grounding/logic/design/LOGIC-IR.md:51-55.** The rewritten sentence says "every
   term-carrier holds exactly one of `logic:termIri` / `logic:termVariable` / `logic:termLiteral` (+ …) /
   `logic:termSequenceMarker`". The compiler accepts a fifth value kind, `logic:termApplication`
   (formula_reader.rs:805-811, 952-964), and `logic:TermCarrierValueConstraint` lists five
   (module.ttl:17042-17049). Read as written, the sentence calls every function-term carrier malformed, which
   is false of the code. The omission existed at base, but this commit rewrote the sentence and kept it, and the
   repository counts a false statement in documentation as a defect. Fix: add `logic:termApplication` to the
   list.
2. **REQUIRED: slices/grounding/logic/module.ttl:17147 (TermCarrierIndexConstraint `gmeow:howToUse`).** The
   text "Require one non-negative integer index on each carrier, then validate…" was true while the constraint
   was class-wide. The commit's new definition (17156) states that it is not a requirement on a carrier in a
   triple-term position of a rule atom. The term now contradicts itself, and the howToUse sentence is false for
   the 169 carriers this change exists to exempt. Fix: say "each logic:argument carrier", and add the fr and zh
   entries, because a changed literal needs them.
3. **OPTIONAL: slices/grounding/logic/tests/example-conformance.ttl:246.** The comment says "all seven
   ontology-authored constraints", but the module also authors `ModalNecessityAccessibilityConstraint` and
   `ModalPossibilityAccessibilityConstraint` over `logic:Formula` (module.ttl:16984-16998). The fixture
   satisfies them vacuously. The count follows the base's own convention ("six"), so it is imprecise rather
   than a contract break.
4. **OPTIONAL, pre-existing: crates/docs/tests/fixtures/reviewed_coverage_golden.json.** Already stale at base;
   see answer 6. The commit adds 14 more drift keys to a test that already fails, so it does not turn a passing
   test into a failing one. The test still needs a visible owner.

No issue numbers or process references were added. No `owl:` or hand-authored shape was added. The new term
carries `rdfs:label`, `skos:definition` and `rdfs:isDefinedBy`.

## Answers

**1. The compiler's rule.** `logic:termIndex` is demanded only by `parse_term_index`
(formula_reader.rs:1031-1056), which has two callers:
- `parse_term_carriers` (670-693), always called with link `"argument"`, from three places:
  - atomic predications (317)
  - function terms reached through `termApplication` (1020)
  - rule `distinctBody` inequality guards (749)
- `parse_bound_vars` (701-741), over `quantifiedVariable`.

Carriers in rule triple-term positions go through `read_atomic_carrier` (763-796; frontend.rs:2125-2135), which
never asks for an index. In the module, only `logic:argument` and `logic:quantifiedVariable` have range
`logic:TermCarrier` (module.ttl:16864-16880).

The new pair of constraints therefore covers exactly the compiler's set. Function-term operands, guard
operands and sequence-marker carriers are all objects of `logic:argument`, so they are covered. Sugar and
constraint descriptors carry no carriers. Nothing the compiler reads without an index is constrained. The
constraint checks only that an index exists; the frontend still owns exactly-one, non-negative and
contiguity, and both definitions say so truthfully.

**2. Acceptance and lowering.** `logic:Constraint` is excluded from axioms (frontend.rs:532-596). The
integrity root is a `forall` node; its binders sort to `[this, owner]` (or `[this, binder]`); its body is an
`antecedent`/`consequent` implication (formula_reader.rs:397-415). The antecedent is a `relation` atom with
the IRI `logic:argument` (or `logic:quantifiedVariable`) over indexed carriers. `target_from_integrity`
(constraint.rs:723-830) takes focus = `this`; finding no class, sparqlTarget or directType guard and no
subject-position atom, it returns `ShapeTarget::ObjectsOf(logic:argument)`, or the same for
`quantifiedVariable`.

`try_project_block` emits `<…/logic/TermCarrierIndexConstraintProceduralConstraintShape>` with
`sh:targetObjectsOf logic:argument` and `SELECT $this WHERE { ?owner logic:argument $this . FILTER NOT EXISTS
{ $this logic:termIndex ?index . } }`. The binder twin has the same shape over `quantifiedVariable`. The
query text comes from the author's probe and the test assertion; I did not trace `violation_where`.
`admit_component_constraint` is a no-op here because there is no `termSameComponent`, and only one satellite
is present.

The two levels do not interfere. The reader treats `logic:relation logic:argument` as an ordinary IRI-valued
relation (Term::iri, 309-316), and the satellites are read only under their family. Every blank-node carrier
and binder in both integrity trees carries a termIndex, so the module's own constraint records do not
trigger the lowered shapes; the independent scan confirms it.

**3. Authored content.** Only the two intended counter-examples have an indexless object of either predicate.
None of the 169 rule-variable carriers is such an object. The method parses real Turtle, including RDF 1.2
triple terms and nested bracketed blank nodes, so I consider it reliable for tracked `.ttl` and `.trig`. It
does not cover Turtle embedded in Rust tests or producers. The producers I read (rdf.rs,
distribution_catalog.rs, math producers.rs) all emit an index.

**4. Slice tests.**
- `structural.ttl`: the path `logic:integrity/logic:forall` → `antecedent/relation` and
  `consequent/exists/relation` matches the stored tree exactly. The `FILTER NOT EXISTS {… logic:trigger …}`
  check is weak but correct.
- Carrier counter-example: an atom with `relation` and an indexless operand. It violates only
  TermCarrierIndex. FormulaConstructor sees one family, the implication and modal guards are not triggered,
  the carrier has exactly one value kind, and there is no datatype.
- Binder counter-example: an `exists` node whose binder lacks an index and whose body operand is indexed. It
  violates only BoundVariableCarrierIndex.
- Shape names: `procedural_shape_iri` = namespace + pascal(local) + `ProceduralConstraintShape`
  (shapes.rs:1036-1064), so both pins are correct. The harness requires only that some
  `SPARQLConstraintComponent` finding comes from the pinned shape (exec.rs:979-998); soleness is not
  declared.
- Pass fixture: the added `exists` node satisfies every typed-IR constraint. The harness validates module ∪
  example (exec.rs ~838-842), so the conforms cell also depends on the module being clean, which the scan
  supports.

**5. Rust test.** `objects_of_guard_over_argument_lowers_to_an_executable_target_objects_of_constraint` parses
authored Turtle through `parse_logic_str`, projects through `project_procedural_constraint`, and executes the
shape on synthetic data. It protects a named behaviour: an object-position guard over a reserved IR predicate
lowers to `sh:targetObjectsOf` and executes, with the levels kept apart and unreached carriers not focused. It
uses no corpus or producer. Its exact-string assertion is somewhat brittle, but the behavioural half earns
its keep.

**6. Text.**
- Definitions, messages, examples, useWhen, avoidWhen, the conformance rationales and the math-lift comment
  (lift.tests.rs:387-389) are true. Exceptions: findings 1 and 2.
- The BoundVariable avoidWhen ("reordering … can change the meaning") is defensible: block position decides
  the focus variable (constraint.rs:730) and repeats the existing `logic:quantifiedVariable` definition.
- The French carries every clause and reads correctly. The Chinese keeps the structure and the `logic:`
  identifiers; 约束变量 is the standard term for "bound variable".
- Golden: `corpus_reviewed_coverage_matches_frozen_golden` asserts that the golden equals the producer
  observation, built from `ontology_catalogs::discover`, the same set the refresh example uses. The test runs
  in the required default nextest profile. The golden holds 148 catalogs and 28027 keys (logic fr.po: 5851
  keys against 6046 msgctxt at base). 184 slice catalogs exist on disk, and the author's run of the official
  `make maint-refresh-reviewed-coverage-golden` produced 188 catalogs and 64031 entries. The golden is
  therefore about 36000 keys stale at base, and the claim holds. The test was red at base and stays red; this
  commit flips no passing test. Inference, not run: main CI should be red on that test unless something
  outside the code I read changes the observation.

**7. Ratchet and floors.**
- Ratchet: no guarded-vocabulary triple was added or removed. RDFS is counted only for `subClassOf` and
  `subPropertyOf`, and annotations and signatures do not count. Moving from sugar typing to `logic:Constraint`
  cannot raise a ceiling.
- Floors: the logic slice holds 1.0 floors (rubric module) on axes including TranslationCoverage,
  Documentation and ShapeMigration.
  - The new term carries the full annotation set, and fr and zh entries for every literal.
  - Its sibling constraint individuals carry fewer catalog entries and already score 1.0, so no fall is
    expected.
  - axisGmn1Coverage and axisGmnGlyphOptimality, if they score per term, are unverified: I did not read
    their scorers.

## What only a producer pass can prove

- The two constraints lower in the real pipeline and the derived shapes reach the slice conformance surface.
- The four conformance cells give the outcomes they declare, and the `structural.ttl` ASK holds on the stored
  module.
- No generated or bundle-level graph contains an indexless `logic:argument` or `logic:quantifiedVariable`
  object, and no slice-quality axis falls.
- The pinned `surface.matches_source` resolves the CURIE pins.

VERDICT: BLOCKED at 72d400ffb68c6abfa1d823f075edd6460dfbb829
