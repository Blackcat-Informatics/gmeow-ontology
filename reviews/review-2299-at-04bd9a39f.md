BLOCKED at 04bd9a39f

# Review: issue 2299 acting-group controls

## Scope

- Worktree `/home/paudley/Active/gmeow-ontology/.worktrees/2299-identify-the-e8-group-action-actor`,
  branch `paudley/2299-identify-the-e8-group-action-actor`, head `04bd9a39fe27ebf6efd85ee0ca68f621ee223b59`,
  tree clean.
- `origin/main` fetched during this review: `c0a3734f8a2447a1289ab1a8babb1e7babd4f436`, which is also the
  merge base. Range `origin/main...HEAD`: one commit, 13 files, 470 insertions, all under
  `slices/grounding/math`.
- Requirement read from the forge: issue 2299 body (including its "Source delivery and remaining
  qualification" section) and both comments.
- Read-only. No producer, gate, harness, reasoning or mutation run.

## Evidence read

- Issue 2299 body and comments; pull request 2550 metadata and file list.
- The whole diff; `examples/e8-symmetry.ttl`; `module.ttl` lines 2190-2609, 12800-12934, the appendix
  restriction and closure lines for the classes the new example instantiates; the three existing actor
  counter-examples and the three conforming fixtures the coder compares against;
  `tests/example-conformance.ttl` header and the actor cells; `design/MATHEMATICS-ALGEBRA.md` in full.
- `crates/slicetest/src/exec.rs` (competency overlay 321-392, structural run and fail-witness 600-727,
  conformance cell 826-935), `native_query.rs` in full, `stores.rs` `source_files`, `paths.rs`
  `example_file`, `repository.rs` 355-366.
- `purrdf-sparql-algebra-4.0.0/src/parser.rs` 1630-1656 (prefix resolution), the version pinned in
  `Cargo.lock`.
- `crates/logic-compile/src/projections/reader_view.rs` 1-22 (the reader view is additive).
- `crates/slice-quality/src/axes.rs` (testing 879-1054, rationale predicates 368-384, GMN-1 1558-1620),
  `reasoner.rs` (65-137, 190-330, 430-470), `report.rs` 344-387 (the scored slice graph),
  `crates/docs-model/src/coverage.rs` 93-115, `crates/gmeow-dev-cli/src/dev_i18n.rs` line 84.
- The coder's `.stage/issue-2299/` logs, `run-checks.sh` and `hand-eval.md`, compared after my own
  working of each query and assertion.

## Required findings

### R1. Two structural assertions do not parse, so they cannot pass or fail

`slices/grounding/math/tests/structural.ttl`

- `ex:saGroupActionActorIsAGroup`, pattern at lines 1606-1616: declares `PREFIX m:` and `PREFIX rdfs:`
  only, and line 1613 uses `l:subClassOf`.
- `ex:saGroupActionIsNotItsActor`, pattern at lines 1621-1629: declares `PREFIX m:` and `PREFIX rdfs:`
  only, and lines 1626 and 1628 use `l:subClassOf`.

The harness hands the pattern string to the engine unchanged (`native_query.rs` 169-185: no prelude,
`base_iri: None`). The parser's prefix table starts empty and an unknown prefix is a syntax error
(`purrdf-sparql-algebra-4.0.0/src/parser.rs` 1630-1656, "undeclared prefix"). `run_ask`
(`exec.rs` 714-719) turns that into a `SparqlEval` error, the cell fails, and `aggregate` fails the math
slice's structural specification. The other nine new patterns that use `l:` declare it.

Consequences: the primary run of both assertions errors before any polarity is read; their fail-witness
runs (`group-action-actor-wrong-kind.ttl`, `group-action-typed-as-group.ttl`) never execute, so the
claim that every ban has a witness that trips it is false for these two; and the slice's structural
specification would red in the producer pass. `hand-eval.md` records both as "Pass" and describes the
path alternative `l:subClassOf` without noticing the missing declaration.

Correction: add `PREFIX l: <https://blackcatinformatics.ca/logic/>` to both patterns. With it, both
work by hand as the coder describes: every authored actor has a type that reaches `math:Group`
(`math:WeylGroup logic:subClassOf math:Group` at module 2435, `math:AutomorphismGroup rdfs:subClassOf
math:Group` at 2209, or `math:Group` itself by the zero-length path), and both named fixtures make the
ban hold over module plus fixture.

### R2. The algebra charter misstates what the validation view covers

`slices/grounding/math/design/MATHEMATICS-ALGEBRA.md` lines 153-154: "They do not show that the
generated validation view rejects the same inputs, which the four actor conformance cells cover".

The four actor cells (`tests/example-conformance.ttl`, `ecGroupActionActorValid`, `...Missing`,
`...WrongKind`, `...Multiple`) cover one conforming declaration and a missing, non-Group and repeated
actor. The table directly above lists further inputs: a different actor on `exa:e8WeylAction`, a law
named by one link only, a descriptor typed as a group, and an axiom that characterizes or derives
`math:actingGroup`. No cell covers those, and the validation view does not reject the first two at all:
`math:GroupAction` carries only the three actor obligations (module 12896-12901), a framed subgroup
satisfies `logic:allValuesFrom math:Group`, and there is no law-link obligation on the class. The
sentence tells a reader of a canonical design set that the validation view rejects every input in the
table. State instead that the cells cover the missing, repeated and non-Group actor, and that a wrong
actor on the named E8 record and a dropped law link are well formed to the validation view and are
caught only by these declared-data controls.

## Optional findings

- O1. Overlap on the E8 record. `saE8WeylActionRecord` already requires the actor, the target, the
  operation and both law links to a quantified Formula; `saE8WeylActionKeepsBothLawLinks` adds only a
  fail-witness for one of those conjuncts, and `cqE8ActingGroup` repeats the actor, target and both-link
  facts on the asserted lane. One of the three could go without losing a named behaviour.
- O2. `saActingGroupIsNeverDerived` has four branches and its fixture exercises one (the chain). The
  rule branch matches `?rule a l:Rule` only, so a rule typed solely by a subclass of `logic:Rule`
  (the repository has `gmeow:DiagnosticMetaRule`) would not be seen; the reader view adds no type
  inference. The same form is the precedent in `slices/core/inquiry/tests/structural.ttl` 388-397.
- O3. `tests/counter-examples/acting-group-functional.ttl` mints `math:actingGroupFunctionality` in the
  slice's own namespace. Authored characteristic records in the repository are blank nodes; a blank node
  here avoids an undeclared `math:` name in a file the slice-quality graph loads.
- O4. `floors.log` ends with `EXIT=0` from a variable `run-checks.sh` sets to 0 and never updates in the
  floors loop; the per-revision `statuses prose=0 translation=0 docmat=0` lines are the real statuses.
  The three mirrors read `module.ttl`, `i18n/` and `docs.md` only, and the first two are byte-identical
  across the range, so equal output is by construction and says nothing about the axes whose inputs
  this change touches (see "Floors" below).
- O5. The "What fails it" column and the two `cqRationale` texts describe failure cases (second actor,
  subgroup as actor, role swap) that no cell exercises on the competency lane; they are hand-worked
  only. True by my working, unexecuted.

## Findings by question

1. Mathematics and modelling. Correct. Aut(A2) has order 12 and contains the negation map, which is not
   in W(A2); W(A2) has order 6; its rotations form a subgroup of order 3; each has index 2 in the next,
   so both `math:normalSubgroupOf` links are true, and the example does not chain them (the property is
   declared non-transitive at module 2277-2282). Each property is used within its definition:
   `math:actingGroup` from descriptor to Group, `math:actsOn` from descriptor to root system,
   `math:weylGroup` root system to Weyl group, `math:hasRootSystem` back, `math:automorphismGroupOf` only
   on the full group. `exag:a2Weyl` carries no `math:AutomorphismGroup` type. The E8 fixture's subgroup
   order 348364800 is half of 696729600. Nothing new is asserted of E8 beyond what `e8-symmetry.ttl`
   declares; the E8 equality stays data on one record and the module-scope bans keep any universal rule
   out.
2. Queries and assertions. My working agrees with `hand-eval.md` for nine assertions and both
   competency questions, and disagrees for the two in R1. Forms match the repository:
   `logic:characterizes` with `logic:characteristicSort` (2371 and 1487 uses), list-valued
   `logic:propertyChainAxiom`, `logic:head [ rdf:reifies <<( s p o )>> ]`, `logic:` spellings retained
   beside the additive reader view. Scopes load what the assertions need: `scopeModule` is `module.ttl`,
   `scopeModuleAndExamples` adds every `examples/*.ttl` (only `e8-symmetry.ttl` and
   `a2-acting-groups.ttl` hold a `math:GroupAction`), and a fail-witness runs over module plus fixture
   without examples. Both competency questions pair `gmeow:cqDataFile` with `gmeow:reasoningNone`. None
   is vacuous once R1 is fixed. All eight named fixtures exist under `tests/counter-examples/`, where all
   26 fail-witnesses in the repository live.
3. Fixtures and cells. The premise that the five new fixtures have conformance cells is wrong: the diff
   adds one cell, `ex:ecA2ActingGroups` (conforms), and the five fixtures are structural fail-witnesses
   with no expected code or source shape to check. No gate requires a counter-example file to be bound
   (docs-model names "an unbound counter-example" as a state; the reasoner clash check reads only
   fixtures bound by a violating cell). For the conforming cell, by reading: every class-local obligation
   on `math:RootSystem`, `math:WeylGroup`, `math:AutomorphismGroup`, `math:AlgebraicStructure` and
   `math:GroupAction` is met, and rank 2 with 6 roots leaves the E8 order constraint inert. The plain
   integer rank against `logic:onDataRange xsd:nonNegativeInteger` I confirm only by precedent
   (`lie-structure-complete.ttl` and `e8-weyl-canonical.ttl` are bound conforming with the same form).
4. Text. No history wording, no issue or pull request numbers, no credits. Apart from R2 the charter
   paragraph and the guide paragraph are true of the module: `math:actingGroup` has no characteristic,
   domain, range, sub-property, inverse, chain or rule (module 2472-2482 and four non-prose mentions in
   total), consistent with its own scope note and the `gmeow:avoidWhen` on `math:GroupAction`.
5. Floors. The scored slice graph is `module.ttl` plus every `.ttl` under `examples/` and `tests/`
   (`report.rs` 377-387), so the new files are scored inputs. By reading: testing reach is a
   reached-over-terms ratio and added corpus can only add reach; documentation maturity dimensions are
   per-term presence facts; provenance honesty reads `saRationale` and `cqRationale` and none of the new
   rationales matches its pattern; the reasoner population is IRI-to-IRI triples on listed predicates
   plus `rdf:type` characteristics, which the fixtures (a `logic:characterizes` record, a list chain) and
   the example do not add; translation and prose score module terms only. GMN-1 coverage (floor 1.0)
   reads module plus `examples/`: the new example uses the same construct kinds as `e8-symmetry.ttl`,
   which is the only basis I have.
6. Evidence. Each log is headed with the worktree path, head `04bd9a39f` and an empty porcelain, and
   records exit 0, subject to O4.
7. Scope and hygiene. Nothing outside `slices/grounding/math`; `module.ttl`, `manifest.ttl`, `mappings/`
   and `i18n/` unchanged; no Python; no credits. The new `@x-gmeow-english` literals are example labels.
   The translation lint scans `module.ttl` and `manifest.ttl` (`dev_i18n.rs` line 84) and the existing
   example labels have no catalog entries, so none are owed.

## Pull request 2550

It is not open. It merged on 2026-11-05 as `c441bfce55033cf9c9ab66e92299ef74de20350d`, an ancestor of
`origin/main`, and is the commit the issue's own addendum cites as the source delivery. It added
`math:actingGroup`, the three class-local obligations, `math:MalformedGroupActionActor`, the actor link on
`exa:e8WeylAction`, the four actor cells and the catalog entries. This branch does not duplicate or
conflict with it and is not superseded: it builds on that representation and touches none of its
module, catalog or citation lines. The coder's position that the representation and record wiring are
already on main is right, and no term or data change is owed by the issue.

## The closing question

The issue's own text keeps it open "for actual actor-boundary and exact-E8/law-link controls over
permitted authenticated inputs", and its fourth acceptance point says "Verify actual actor identity and
relevant missing/wrong-actor boundaries through permitted controls and authenticated inputs". The
addendum also says no successful control is claimed. I read the closing condition as controls that have
run over producer-authenticated inputs, not controls that have been authored. This branch supplies the
controls (after R1 and R2). The issue should stay open until a producer pass records the math slice's
structural, competency and conformance specifications green with these cells in them; R1 shows that
pass would currently fail. It does not need the validation view to reject the wrong-actor and law-link
inputs, because by design it accepts them.

## Not verified

- No execution of any query, assertion, fail-witness or conformance cell; all by reading. Execution
  evidence is not obtainable in this session by the operator's ruling.
- That `examples/a2-acting-groups.ttl` validates with no findings against the generated shapes, and that
  a plain integer satisfies the rank obligation; by precedent only.
- GMN-1 coverage of the new example through the codec.
- That the reader-view projection accepts the two axiom-bearing fixtures without error.
- The composition of every floored axis beyond the functions named above.
- `prose-check` on the two documents: my run could not take the shared build lock in time; the coder's
  `prose.log` records exit 0 at this head.
- Hosted CI state for the branch.
