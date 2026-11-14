VERDICT: PASS 98bf53bad92c879b7e264504ca96da6d4cf98d69

# Review of PR 2688 at 98bf53bad (issue 2685: a transitive partial order between standpoints as dated claims), delta from 4ac550e6b

Read-only review from `.worktrees/2685-vocabulary-a-transitive-partial-order` (HEAD equals the stated head; `gh pr view` agrees). The delta is four files under `slices/core/standpoint/`: `module.ttl` (+85/-14), `docs.md`, `i18n/fr.po`, `i18n/zh.po`. The Rust change reviewed at 4ac550e6b is not touched by the delta. Nothing was built or run. The fixer's account (`.stage/terms-sync/fix2-pr-2688.md`) was treated as a claim and each statement checked against the files.

## Prior REQUIRED findings (`review-pr2688-at-4ac550e6b.md`)

### R1 (boundary record claimed closure under transitivity is not first-order expressible): fixed, option (a)

`gmeow:moreEntrenchedThanRankerClosureConstraint` (`module.ttl:1208-1288`) authors the per-ranker closure obligation as a `logic:Constraint`; the false boundary record is deleted; no text in the slice still says "three or more", "ClosureBoundary" or "not first-order expressible" (grep of the slice, `docs/`, `crates/`; the only `moreEntrenchedThan` mention outside the slice is the irreflexivity test comment). The PR reply to the automated reviewer (4575452100) retracts the earlier over-claim and matches the head.

Formula, read against its precedents (the attribution constraint at `module.ttl:1131-1152` and the order constraint at `:1154-1205`):

- Scoping. Outer `forall` over `this, middle, lower, first, second, ranker`; every non-focus variable is positively bound in the antecedent (`first`/`second` by the reifier atoms, `middle`/`lower` by `reifiedStatementObject`, `ranker` by `accordingTo`); the attribution constraint has the same "extra universal bound by the guard" form (`other`). The six negated existentials in the antecedent use fresh binders (`firstSuccessor`, `firstCorrection`, `firstWithdrawal`, `secondSuccessor`, `secondCorrection`, `secondWithdrawal`) that each point at the already bound `first` or `second`. The consequent's own binder `third` and its three negated binders (`thirdSuccessor`, `thirdCorrection`, `thirdWithdrawal`) are fresh, and the consequent reuses only `this`, `lower` and `ranker` from outside. No name collides.
- Direction. `succeedsClaim`, `correctsClaim`, `retractsClaim` take the successor, correction or withdrawal as argument 0 and the claim as argument 1, as in the order constraint; "live" (object of none of the three) therefore matches the definition.
- Lowering. In `crates/logic-compile/src/projections/shapes.rs` the guard is lowered with `lower_positive` and the consequent's negation with `lower_negative`. The `rdf:type this Standpoint` atom becomes the target class; each `Not(Exists(atom))` in the guard becomes `FILTER NOT EXISTS`; the negated existential consequent becomes `FILTER NOT EXISTS { reifier triples . FILTER NOT EXISTS {...} x3 }`. Each construct has a lowering rule, and nested `FILTER NOT EXISTS` inside a `FILTER NOT EXISTS` group is valid SPARQL with `?third` bound in the same group. The combination (negated existentials in the antecedent plus a consequent existential that itself holds negated existentials) has no single precedent, as the fixer says; each part has one, and I found nothing in the lowering that refuses it. I did not run it.
- Meaning. The cycle argument holds. For live claims of one ranker forming a cycle a1>a2>...>an>a1, the closure forces a live a1>a3, then a1>a4, down to a live a1>an. That claim and the live an>a1 are a reversed pair; for two live claims no lifecycle relation points at either (one that did would make that claim not live), so the order constraint's exemptions do not apply and it rejects them. A cycle of one is the self edge (irreflexivity); of two, the order constraint. A closure obligation whose only spanning claim is not live is also a violation, as the definition says ("the concluding claim must itself be live"). So nothing second-order remains.

Retiring the boundary is honest. The residue the new text names (the checks read the record as it stands, compare no dates, apply where the first standpoint is typed `gmeow:Standpoint`, and do not check who made a lifecycle link) is described as a choice and a scope limit, not as inexpressibility. The old record's only content was the false claim, and nothing else in the repository references it, so removing it leaves no dangling reference.

### R2 (historical-order rule differed between `howToUse` and `docs.md`, did not cover retraction, claimed determinism): fixed

`module.ttl:185` (`howToUse`) and `docs.md:93-101` now carry the same sentences for the historical order: superseded kept before its successor took effect and out from then on; corrected out at every t; retracted kept before the retraction took effect and out from then on, with the reason (a retraction withdraws endorsement and does not erase that the claim was made); `gmeow:displayable` ignored. The displayed-order rule is identical in both. The closing determinism sentence now names the four inputs (graph, ranker, t, use) and is true given the effect-date rule below.

## The added effect-date rule

"A lifecycle relation takes effect at the earliest `gmeow:validFrom` on its own statement; one that carries no such date counts as in effect at every t." This was not in the review, and it is within the contract: "took effect" is what the retracted and superseded cases hinge on, and without it the determinism sentence is false. It is consistent with `docs/claim-lifecycle.md`: that guide says to annotate each relation with `gmeow:validFrom` as its "effective time", and gives the three historical interpretations the text follows (succession keeps the earlier claim valid for its earlier interval; correction rejects the earlier validity; retraction withdraws endorsement without establishing falsity). The guide is silent on a missing date and on several dates, so the two choices (earliest; undated counts as always in effect) are the author's, scoped to this term's orders only; the closure constraint's undated-link treatment (any link at all makes a claim not live) agrees with the undated-in-effect choice. The rule changes no lifecycle semantics for other terms.

## Texts and catalogs

- Term definition (`:180`), `howToUse` (`:185`), pair-constraint definition (`:1158`), closure definition, `avoidWhen`, `howToUse`, `useWhen`, `logic:message`, `docs.md` (fourth enforced record, cycle paragraph, "What is not enforced") agree with each other and with the formula.
- Read-only comparison script (outside the repository) over `module.ttl` against both catalogs: 28 changed or added English literals, 141 catalog entries each, 0 problems, 0 catalog `msgid` without an English literal, for `fr` and `zh`. The three boundary-record entries are gone from both; five entries for the closure constraint exist in both; `fragmentBoundaryReason`-style plain literals needed none. Spot read of the French closure entries follows the English clause for clause.
- No issue or PR number or process wording in any added line (grep of the delta).
- Hosted checks: only the automated review and the licence check are reported on this head; no generation job result exists, so nothing from a producer run supports or contradicts the above.

## OPTIONAL findings

- O1 (carried, now more salient). No counter-example or competency case exercises any of the three constraints. The closure constraint has the most complex formula in the slice and a vacuous binding (a wrong variable, a mis-directed lifecycle atom) would pass every gate silently. A chain without its closing claim, the same chain with it, a chain with a retracted premise, and a cross-ranker chain would give teeth, once a producer run can confirm the violation codes. `docs/MIGRATING-SHAPES-TO-LOGIC.md` asks for a pass and a fail witness per constraint.
- O2. The native relational-core lowering (`crates/logic-compile/src/relational_core.rs`, `negate_consequent_conjunct`) refuses an existential consequent whose body is a conjunction; that is true of the existing attribution constraint as well, so the new constraint is no worse, but whether either is meant to enter that lane is for a producer run to show.
- O3. The closure definition ends with "A universal-to-existential formula over the lowered reifier components.", copied from the attribution constraint; it says nothing a reader can use. Drop it from both or replace it with what the shape means.
- O4. The effect-date rule's two choices (earliest date; undated counts as always in effect) are worth one line on the lifecycle guide so other consumers of the lifecycle relations decide the same way; not needed here.
- O5 (fixer's own note): the `fr` displayed-order sentence says "avant t" where the English says "by t"; it predates the change.

## Limits (only a full producer run can show)

That the closure constraint lowers, runs, rejects a chain without its closing claim and accepts one with it; the annotation-completeness gate, derived shapes, reasoning consistency, projection-vocabulary ratchet, slice-quality floors, translation gates (`msgfmt`, keying), `all_axioms_project`, and the hosted generation jobs.

## Coverage

Read: `git diff 4ac550e6b 98bf53bad` in full for `module.ttl` and `docs.md`, and the changed lines of `fr.po`; `module.ttl` 1128-1290 (attribution constraint, order constraint, new closure constraint); `docs/claim-lifecycle.md` 1-80; `crates/logic-compile/src/projections/shapes.rs` 1380-1830 (lowering of guard, negation, exists, forall); `crates/logic-compile/src/ir/constraint.rs` 466-500 and `relational_core.rs` 960-1340 and 1757-1797; `.stage/terms-sync/fix2-pr-2688.md` (all); the author's reply 4575452100; `gh pr checks 2688`; `review-pr2688-at-4ac550e6b.md` (all); repo-wide grep for the retired record and the old wording; `/tmp/review-scripts/po_check.py` run on the slice.
Not read: the whole of `zh.po` beyond the script comparison; the rest of `module.ttl` and the Rust change (unchanged since the earlier review); the SPARQL lowering of `reifiedStatement*` atoms (shared with the unchanged order constraint); CI logs.
