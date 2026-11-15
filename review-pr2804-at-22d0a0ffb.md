# Delivery recheck: PR 2804 at 22d0a0ffb

- Head: `22d0a0ffb83099ca82c57502a06b2928b43c9f4a`. Checked with `git rev-parse HEAD` in the worktree; it matches the PR's `headRefOid`.
- Previous review: `review-pr2804-at-ed2964811.md`, verdict BLOCKED at `ed29648111aac75580f6625881b5b6997ef78224`.
- Delta reviewed: `git diff ed2964811 22d0a0ffb`. It touches 6 files, +93/-68: `docs/claim-lifecycle.md`, `slices/core/standpoint/docs.md`, the standpoint `fr.po` and `zh.po`, `slices/core/standpoint/module.ttl` and `slices/core/standpoint/tests/competency.ttl`.
- Started: Sat 14 Nov 2026 08:38:08 PM CST. Written: Sat 14 Nov 2026 08:52:10 PM CST.
- Mode: read-only. No builds, `make`, `cargo` or test runs, and nothing in the worktree was changed.

## Scope

This recheck covers the changed work and the findings left open by the first review. Unchanged history is not re-judged.

### What was read

- The complete delta: module.ttl, docs.md, claim-lifecycle.md, competency.ttl, and every changed msgid/msgstr in fr.po and zh.po.
- The PR body, fetched again at this head. The commit message.
- Formula admission:
  - `crates/logic-compile/src/frontend/formula_reader.rs` lines 176-422 and 660-741;
  - `crates/logic-compile/src/frontend/admission.rs` lines 1-140;
  - `crates/logic-compile/src/frontend.rs` lines 5480-5500 and 5676-5712;
  - `crates/logic-compile/src/ir/constraint.rs` lines 520-554 and 723-783.
- Lowering:
  - `crates/logic-compile/src/projections/shapes.rs` lines 1380-1619 and 1925-2011;
  - `crates/logic-compile/src/projections/sparql_lower.rs` lines 40-62.
- Where the reifier edges come from:
  - `crates/logic/src/statement_lowering.rs`;
  - `crates/logic/src/operator.rs` lines 330-375 and 506-600;
  - `crates/gmeow-cli/src/commands.rs` lines 7156-7240;
  - a `git grep` for callers of `lower_reifiers` across `crates/`.
- Validation paths:
  - `crates/logic-compile/src/projections/reader_view.rs`;
  - `crates/validate/src/validate_all.rs` lines 780-830 and 1938-2165;
  - `crates/slicetest/src/exec.rs` lines 171-300;
  - `crates/logic/src/verify/prepared_gates.rs` (constraint list only);
  - `crates/pipeline/src/stages/compile_logic/validation_fixtures.rs`;
  - purrdf-shapes 4.0.0 `src/shape_union.rs` lines 45-70;
  - a search of every purrdf 4.0.0 crate for `reifiedStatement`.
- Related data:
  - `slices/core/standpoint/module.ttl` lines 1134-1320;
  - the agency-records `DefectResolutionAttributionConstraint`;
  - `slices/extensions/narrative/examples/story-building.ttl` lines 68-84;
  - `slices/grounding/logic/module.ttl` lines 17918-17928;
  - `CONSTITUTION.md` lines 192-223.

### What was not read

- `formula_reader.rs` lines 1-175 and 425-660.
- The source-graph checks in `admission.rs` past line 140.
- Any generated `procedural-constraints.ttl`. No materialized `generated/shapes` exists in any local worktree.
- CI results at this head.

### Commands with exit status

- `prose-check docs/claim-lifecycle.md slices/core/standpoint/docs.md`: exit 0.
- `gh pr view 2804 --json body -q .body | prose-check --stdin`: prose-check exit 0, read from `PIPESTATUS`.
- An in-memory catalog comparison of the 5 added literals against fr and zh: every msgid matched byte for byte; 0 orphans; 0 empty msgstr.

## Evidence on the coordinator's questions

### 1. Does the closure formula read the correcting standpoint from the reifier?

Yes. The negated existentials for `first`, `second` and `third` each bind two variables: the correcting claim `<x>Correction` and its reifier `<x>CorrectionLink`, with contiguous indices 0 and 1 and fresh names. Each one then requires all of these:

```
reifiedStatementSubject(<x>CorrectionLink, <x>Correction)
reifiedStatementPredicate(<x>CorrectionLink, gmeow:correctsClaim)
reifiedStatementObject(<x>CorrectionLink, <x>)
gmeow:accordingTo(<x>CorrectionLink, ranker)
```

How the variables are bound:

- `ranker` comes from the outer universal.
- `third` comes from the existential in the consequent.
- No outer variable is captured.

The formula is admitted:

- `reconstruct` accepts one constructor family per node.
- `parse_bound_vars` accepts the contiguous plain variables.
- Each `and` has 4 operands.
- `target_from_integrity` collects guard atoms through ∧ and ∃ but never through ¬, so the target is unchanged: `sh:targetClass gmeow:Standpoint`.

The lowering in `shapes.rs` produces this pattern:

```
FILTER NOT EXISTS { ?l <rSS> ?c . ?l <rSP> <correctsClaim> . ?l <rSO> ?x . ?l <accordingTo> ?ranker . }
```

Two cases worked by hand on the reasoning path, where `operator::cli_world_dataset` supplies the lowered edges:

- **The ranker corrects its own ranking.** The new docs.md snippet `ex:reviewedRanking gmeow:correctsClaim ex:mistakenRanking {| gmeow:accordingTo ex:editorialBoard |}` produces a witness. The corrected claim is then not live, and the obligation lapses.
- **Another party makes the correction.** An `accordingTo` that names another party produces no witness. The claim stays live.

The REQUIRED finding F1 from the first review is fixed.

### 2. Are the changed texts true at this head?

Yes. Each of these matches the formula:

- the definition and howToUse of `moreEntrenchedThan`;
- the closure constraint's definition, avoidWhen, howToUse and message;
- docs.md "What the canonical checks enforce", including the new sentence that sets the pair check against the closure check;
- the docs.md correction snippet;
- the rewritten competency comment;
- `docs/claim-lifecycle.md`;
- the PR body.

On the pair-check sentence: the unchanged order constraint does exempt a pair when either claim corrects the other, and it does not check who made the link, exactly as the text says.

All new French and Chinese translations are real translations and carry every clause.

### 3. Does any validation path supply the lowered `logic:reifiedStatement*` edges?

The author says none does. I verified that.

The only producer is `statement_lowering::lower_reifiers`. Its only production caller is `operator::cli_world_dataset`, reached through `operator::derive` from `gmeow logic derive` running on the caller's input. None of these paths call it:

- example SHACL in `make validate`, through `shacl_reader_view`, which emits `rdf:reifies` quads and plain annotation triples;
- merged SHACL;
- slicetest example conformance;
- `validation_fixtures.rs`;
- `prepared_gates.rs`, which covers math constraints only;
- every purrdf 4.0.0 crate.

The consequences are recorded as a separate finding (S1) below.

### 4. Historical order and the displayable sentences

- **Historical order.** "The ranker's present account of what it ranked at t" agrees with leaving a self-corrected claim out at every t. docs.md adds that the corrected claim stays in the record, which is consistent with Principle 10.
- **Displayable sentences.** The global-safety classification is removed from `claim-lifecycle.md` and from docs.md. The remaining sentences say no data sets `gmeow:displayable false` because a claim is disputed, corrected or evaluated, and point to Principle 10. That statement is true and consistent with `CONSTITUTION.md` lines 192-223 and the recorded ruling.

### 5. Other points in the delta

- The commit message is accurate.
- The PR body's "Not shown" bullet about a fixture is honest. As S1 shows, it understates the problem: no pipeline validation path evaluates the constraint on correctly lowered data.
- The PR body's remark "if the dependency gate objects" is moot. Example and test edges carry no semantics (purrdf-slice 4.0.0 `ownership.rs`), which the first review verified.

## Findings

### Status of earlier findings

- **F1 REQUIRED (first review): resolved.** The closure formula now reads `accordingTo` from the `gmeow:correctsClaim` reifier (see question 1).
- **F2 OPTIONAL: addressed.** The competency comment now says the question cannot detect a derived supersession and that the structural assertion is the guard.
- **F3 OPTIONAL: addressed.** The global-safety classification is removed.
- **F4 OPTIONAL: answered.** No fixture can exercise the rule because no validation path supplies the lowered edges. I verified that reason (S1).
- **F5 OPTIONAL: addressed.** The historical-order text is now consistent.
- **F6 OPTIONAL: unchanged.** Old-reading text remains in events, attestation, dreaming and the test fixtures. It still needs a visible owner on the follow-on lifecycle work.
- **F7 OPTIONAL: unchanged.** Claim tokens become inferred `gmeow:Entity` through the domain and range of `correctsClaim`.

### S1: a separate defect that predates this PR. It does not block this PR's contract, but it needs a tracked owner before merge.

25 `logic:integrity` constraints in 8 slices read `logic:reifiedStatementSubject`, `logic:reifiedStatementPredicate` or `logic:reifiedStatementObject`:

| Slice | Constraints |
|---|---|
| standpoint | 3 |
| agency-records | 14 |
| narrative | 2 |
| refinement-cues | 2 |
| affect | 1 |
| record-metadata | 1 |
| environment-records | 1 |
| dreaming | 1 |

None of the validation paths listed under question 3 supplies those edges, so the lowered SHACL behaves the same way regardless of the data:

- **Reifier atoms in the guard or under negation: the constraint never fires.**
  - The closure constraint's guard requires `rSS(first, this)`.
  - The order constraint's consequent, ¬∃(… rSS …), is always true.
- **Reifier atoms in a positive consequent: the constraint fires on every focus node that matches the guard**, even when the data is correct.
  - `gmeow:moreEntrenchedThanAttributionConstraint` (`module.ttl` lines 1134-1166) would reject every `moreEntrenchedThan` edge from a Standpoint. No example contains such an edge, so nobody has seen this.
  - The agency-records `gmeow:DefectResolutionAttributionConstraint` has the same shape and defaults to Violation.

One point is unresolved: `slices/extensions/narrative/examples/story-building.ttl` lines 74-76 assert `ex:defectRecord gmeow:defectFixedBy ex:fixArtifact` with a reifier that carries `wasAttributedTo` but no lowered edges. According to the code I read, example SHACL should report a Violation on it. If `make validate` passes on main, some mechanism I did not find is involved. A producer pass, or a recorded example-validation report, would settle the question.

A side effect: the "What the canonical checks enforce" text in standpoint docs.md describes checks that the gates do not run on data today. That was true before this PR too.

Why this does not block: the PR's contract is the meaning of the correction, and its formula is correct for the semantics the reasoning path uses. The defect lives in the SHACL data view or in constraint lowering, which the issue does not cover. Before merge it should be filed as its own issue with the evidence above (25 constraints, the call graph of the edge producer, the narrative example), so it has a visible owner.

### O1: OPTIONAL

The howToUse of `moreEntrenchedThan` in `module.ttl` explains historical order but leaves out docs.md's sentence that the corrected claim stays in the record either way. Adding it would make the two texts agree and make the Principle 10 retention explicit in the shipped annotation.

## What only a producer pass can prove

- The generated `procedural-constraints.ttl` text for the closure constraint.
- Whether example SHACL reports the narrative defect-record Violation. This decides the unresolved point in S1.
- Gate status at this head: `make check` and CI.

VERDICT: PASS at 22d0a0ffb83099ca82c57502a06b2928b43c9f4a
